use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Instant,
};

use krometrail_core::{
    ArtifactCacheDisposition, ArtifactEpochSelection, ArtifactGeneration,
    ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGenerationResult, ArtifactHandle,
    ArtifactId, ArtifactLookup, ArtifactOutcome, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, ErrorCode, FrameId, FrameSource, IdSource,
    KrometrailError, NonEmptyText, PortFuture, Result,
};
use temporal_vision::{
    ArtifactKind, SharedAdjacentAnalysis, analyze_adjacent_pairs, generator_descriptor,
};

use super::{
    cache::{CacheIdentityInput, cache_metadata},
    decode::DECODER_PROFILE,
    epoch::{
        ADAPTER_VERSION, EpochPlan, WorkCancellation, bounded_plan, decode_plan, validate_and_plan,
    },
    generators::{
        PreparedGenerator, analysis_noise_floor, estimated_normalized_bytes, generate,
        needs_change_masks, normalization_identity, normalize, prepare_generator,
        reserved_output_bytes, vision_error,
    },
    scheduler::{
        ArtifactScheduler, ArtifactWorkLimits, cancelled_error, controlled, deadline_error,
        limit_error, resource_limit_error,
    },
    single_flight::{FlightArtifact, FlightArtifacts, FlightValue, SingleFlight},
};

#[derive(Clone)]
pub(crate) struct TemporalVisionArtifactService {
    frames: Arc<dyn FrameSource>,
    artifacts: Arc<dyn ArtifactStore>,
    ids: Arc<dyn IdSource>,
    scheduler: Arc<ArtifactScheduler>,
    flights: Arc<SingleFlight>,
}

#[derive(Clone)]
struct Slot {
    epoch_index: usize,
    generator_index: usize,
    kind: ArtifactKind,
    cache: krometrail_core::ArtifactCacheMetadata,
    sources: Vec<ArtifactSourceFingerprint>,
    prepared: Arc<PreparedGenerator>,
    plan: EpochPlan,
}

#[derive(Clone)]
struct WorkSlot(Slot);

type AnalysisCohortKey = (usize, Vec<u8>, u16, Vec<(FrameId, usize)>);

struct Available {
    artifact: FlightArtifact,
    disposition: ArtifactCacheDisposition,
    grace_overridden: bool,
}

impl TemporalVisionArtifactService {
    pub(crate) fn new(
        frames: Arc<dyn FrameSource>,
        artifacts: Arc<dyn ArtifactStore>,
        ids: Arc<dyn IdSource>,
        limits: ArtifactWorkLimits,
    ) -> Result<Self> {
        Ok(Self {
            frames,
            artifacts,
            ids,
            scheduler: Arc::new(ArtifactScheduler::new(limits)?),
            flights: Arc::new(SingleFlight::new()),
        })
    }

    async fn generate_inner(
        self,
        request: ArtifactGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> Result<ArtifactGenerationResult> {
        let now = Instant::now();
        let service_deadline = now
            .checked_add(self.scheduler.limits().max_wall_time)
            .ok_or_else(deadline_error)?;
        let deadline = context
            .deadline
            .map_or(service_deadline, |caller| caller.min(service_deadline));
        if deadline <= now {
            return Err(deadline_error());
        }
        if context.is_cancelled() {
            return Err(cancelled_error());
        }
        let _request_permit = self
            .scheduler
            .acquire_request(deadline, context.cancellation.as_ref())
            .await?;

        let frames = controlled(
            self.frames.frames_by_id(request.range().frame_ids.clone()),
            deadline,
            context.cancellation.as_ref(),
        )
        .await??;
        let planning_cancel = WorkCancellation::default();
        let range = request.range().clone();
        let markers = request.markers().to_vec();
        let limits = self.scheduler.limits();
        let plans = controlled(
            self.scheduler.run_blocking(deadline, &planning_cancel, {
                let planning_cancel = planning_cancel.clone();
                move || {
                    validate_and_plan(
                        &range,
                        frames,
                        &markers,
                        limits.adaptation(),
                        &planning_cancel,
                    )
                }
            }),
            deadline,
            context.cancellation.as_ref(),
        )
        .await??;
        let plans = select_epoch_plans(plans, context.epoch_selection)?;
        let plans = Arc::new(plans);

        let potential_outputs = plans
            .len()
            .checked_mul(
                request
                    .generators()
                    .iter()
                    .map(|generator| generator.output_kinds().len())
                    .sum::<usize>(),
            )
            .ok_or_else(|| limit_error("artifact output count overflows"))?;
        if potential_outputs > limits.max_outputs.get() {
            return Err(limit_error(
                "artifact output count exceeds the configured limit",
            ));
        }

        let mut result_slots: Vec<Option<std::result::Result<Available, KrometrailError>>> =
            (0..potential_outputs).map(|_| None).collect();
        let mut slot_metadata = Vec::with_capacity(potential_outputs);
        let mut slots = Vec::new();
        let mut ordinal = 0_usize;
        for (epoch_index, plan) in plans.iter().enumerate() {
            for (generator_index, generator) in request.generators().iter().enumerate() {
                let generator_plan = match plan_for_generator(generator, plan, limits) {
                    Ok(generator_plan) => generator_plan,
                    Err(error) => {
                        if request.failure_policy()
                            == krometrail_core::ArtifactFailurePolicy::RequireAll
                        {
                            return Err(error);
                        }
                        for kind in generator.output_kinds() {
                            slot_metadata.push((
                                plan.descriptor.index,
                                generator_index as u32,
                                *kind,
                            ));
                            result_slots[ordinal] = Some(Err(error.clone()));
                            ordinal += 1;
                        }
                        continue;
                    }
                };
                match prepare_generator(
                    generator,
                    request.generator_anchor_was_defaulted(generator_index),
                    &generator_plan,
                    limits,
                ) {
                    Ok(prepared) => {
                        let prepared = Arc::new(prepared);
                        for (kind, canonical_parameters) in &prepared.canonical_parameters {
                            let cache = cache_metadata(CacheIdentityInput {
                                session_id: request.range().session_id,
                                target_id: request.range().target_id,
                                sources: &plan.cache_sources,
                                artifact_kind: *kind,
                                canonical_parameters,
                                descriptor: generator_descriptor(*kind),
                                adapter_version: ADAPTER_VERSION,
                                decoder_profile: DECODER_PROFILE,
                            });
                            slot_metadata.push((
                                plan.descriptor.index,
                                generator_index as u32,
                                *kind,
                            ));
                            slots.push((
                                ordinal,
                                Slot {
                                    epoch_index,
                                    generator_index,
                                    kind: *kind,
                                    cache,
                                    sources: plan.source_fingerprints.clone(),
                                    prepared: Arc::clone(&prepared),
                                    plan: generator_plan.clone(),
                                },
                            ));
                            ordinal += 1;
                        }
                    }
                    Err(error) => {
                        if request.failure_policy()
                            == krometrail_core::ArtifactFailurePolicy::RequireAll
                        {
                            return Err(error);
                        }
                        for kind in generator.output_kinds() {
                            slot_metadata.push((
                                plan.descriptor.index,
                                generator_index as u32,
                                *kind,
                            ));
                            result_slots[ordinal] = Some(Err(error.clone()));
                            ordinal += 1;
                        }
                    }
                }
            }
        }

        let mut missing = Vec::new();
        let mut artifact_grace_overridden = false;
        for (slot_index, slot) in &slots {
            match controlled(
                self.artifacts
                    .lookup_artifact(slot.cache.cache_key, slot.sources.clone()),
                deadline,
                context.cancellation.as_ref(),
            )
            .await??
            {
                ArtifactLookup::Hit(artifact) => {
                    result_slots[*slot_index] = Some(Ok(Available {
                        artifact: (*artifact).into(),
                        disposition: ArtifactCacheDisposition::Hit,
                        grace_overridden: false,
                    }))
                }
                ArtifactLookup::Miss => missing.push((*slot_index, WorkSlot(slot.clone()), false)),
                ArtifactLookup::Invalidated => {
                    missing.push((*slot_index, WorkSlot(slot.clone()), true))
                }
            }
        }

        if !missing.is_empty() {
            let keys = missing
                .iter()
                .map(|(_, slot, _)| slot.0.cache.cache_key)
                .collect();
            let waiter = self.flights.join(keys);
            if waiter.is_leader {
                let flight = waiter.flight();
                let service = self.clone();
                let plans = Arc::clone(&plans);
                let work_slots = missing.iter().map(|(_, slot, _)| slot.clone()).collect();
                tokio::spawn(async move {
                    let result = service
                        .run_flight(plans, work_slots, flight.cancellation(), service_deadline)
                        .await;
                    flight.complete(result).await;
                });
            }
            let artifacts = waiter.wait(deadline, context.cancellation.clone()).await?;
            for (slot_index, slot, invalidated) in missing {
                let value = artifacts
                    .get(&slot.0.cache.cache_key)
                    .ok_or_else(|| generation_error("single-flight result omitted an output"))?
                    .clone();
                match value {
                    Ok(value) => {
                        result_slots[slot_index] = Some(Ok(Available {
                            artifact: value.artifact,
                            grace_overridden: value.grace_overridden,
                            disposition: if value.generated {
                                if invalidated {
                                    ArtifactCacheDisposition::RegeneratedAfterInvalidation
                                } else {
                                    ArtifactCacheDisposition::Generated
                                }
                            } else {
                                ArtifactCacheDisposition::Hit
                            },
                        }))
                    }
                    Err(error) => {
                        if is_fatal(&error)
                            || request.failure_policy()
                                == krometrail_core::ArtifactFailurePolicy::RequireAll
                        {
                            return Err(error);
                        }
                        result_slots[slot_index] = Some(Err(error));
                    }
                }
            }
        }

        for result in &result_slots {
            if let Some(Ok(available)) = result {
                artifact_grace_overridden |= available.grace_overridden;
            }
        }
        let outcomes = result_slots
            .into_iter()
            .zip(slot_metadata)
            .map(|(result, metadata)| {
                let (epoch_index, generator_index, kind) = metadata;
                match result.expect("every deterministic artifact slot is assigned") {
                    Ok(available) => ArtifactOutcome::Available {
                        epoch_index,
                        generator_index,
                        artifact: handle(available.artifact, available.disposition),
                    },
                    Err(error) => ArtifactOutcome::Unavailable {
                        epoch_index,
                        generator_index,
                        artifact_kind: kind,
                        error,
                    },
                }
            })
            .collect();
        Ok(ArtifactGenerationResult {
            range: request.range().clone(),
            epochs: plans.iter().map(|plan| plan.descriptor.clone()).collect(),
            outcomes,
            artifact_grace_overridden,
        })
    }

    async fn run_flight(
        self,
        plans: Arc<Vec<EpochPlan>>,
        slots: Vec<WorkSlot>,
        cancellation: WorkCancellation,
        deadline: Instant,
    ) -> std::result::Result<FlightArtifacts, KrometrailError> {
        let mut result = HashMap::new();
        let mut pending_slots = Vec::new();
        for slot in slots {
            cancellation.check()?;
            match self
                .artifacts
                .lookup_artifact(slot.0.cache.cache_key, slot.0.sources.clone())
                .await?
            {
                ArtifactLookup::Hit(artifact) => {
                    result.insert(
                        slot.0.cache.cache_key,
                        Ok(FlightValue {
                            artifact: (*artifact).into(),
                            generated: false,
                            grace_overridden: false,
                        }),
                    );
                }
                ArtifactLookup::Miss | ArtifactLookup::Invalidated => pending_slots.push(slot),
            }
        }
        if pending_slots.is_empty() {
            return Ok(result);
        }

        let mut groups: BTreeMap<(usize, usize), Vec<WorkSlot>> = BTreeMap::new();
        for slot in pending_slots {
            groups
                .entry((slot.0.epoch_index, slot.0.generator_index))
                .or_default()
                .push(slot);
        }

        let mut cohort_counts = BTreeMap::<AnalysisCohortKey, usize>::new();
        let mut cohort_wants_masks = BTreeMap::<AnalysisCohortKey, bool>::new();
        for group in groups.values() {
            let prepared = &group[0].0.prepared;
            let Some(noise_floor) = analysis_noise_floor(prepared) else {
                continue;
            };
            let Some(identity) = normalization_identity(prepared)? else {
                continue;
            };
            let key = (
                group[0].0.epoch_index,
                identity.to_vec(),
                noise_floor,
                analysis_plan_identity(&group[0].0.plan),
            );
            *cohort_counts.entry(key.clone()).or_default() += 1;
            *cohort_wants_masks.entry(key).or_default() |= needs_change_masks(prepared);
        }
        let mut shared_analyses =
            BTreeMap::<AnalysisCohortKey, Arc<SharedAdjacentAnalysis<FrameId>>>::new();
        // Change masks remain live as long as their cohort can be reused. Keep their
        // corresponding scheduler permits for the same lifetime so the shared-analysis cache
        // cannot grow outside the combined-request memory budget.
        let mut shared_memory_permits = Vec::new();

        let generator_semaphore = self.scheduler.generator_semaphore();

        let mut total_output_bytes = 0_usize;

        for ((_epoch_index, _generator_index), group) in groups {
            cancellation.check()?;
            let plan = &group[0].0.plan;
            let mut normalized_bytes = 0_usize;
            for slot in &group {
                if normalization_identity(&slot.0.prepared)?.is_some() {
                    normalized_bytes = estimated_normalized_bytes(&slot.0.prepared, plan)?;
                    break;
                }
            }
            let output_reservation = group.iter().try_fold(0_usize, |current, slot| {
                Ok::<_, KrometrailError>(current.max(reserved_output_bytes(
                    &slot.0.prepared,
                    self.scheduler.limits(),
                )?))
            })?;
            let base_reservation = plan
                .decoded_bytes
                .checked_add(normalized_bytes)
                .and_then(|value| value.checked_add(output_reservation))
                .ok_or_else(|| limit_error("combined artifact memory estimate overflows"))?;
            let cohort_key = {
                let prepared = &group[0].0.prepared;
                analysis_noise_floor(prepared)
                    .zip(normalization_identity(prepared)?)
                    .map(|(noise_floor, identity)| {
                        (
                            group[0].0.epoch_index,
                            identity.to_vec(),
                            noise_floor,
                            analysis_plan_identity(&group[0].0.plan),
                        )
                    })
            };
            let estimated_mask_bytes = if cohort_key
                .as_ref()
                .is_some_and(|key| cohort_counts.get(key).copied().unwrap_or(0) > 1)
                && cohort_key
                    .as_ref()
                    .and_then(|key| cohort_wants_masks.get(key).copied())
                    .unwrap_or(false)
            {
                estimated_change_mask_bytes(plan)?
            } else {
                0
            };
            let shared_mask_bytes = if normalized_bytes
                .checked_add(estimated_mask_bytes)
                .is_some_and(|value| value <= self.scheduler.limits().max_normalized_bytes.get())
                && base_reservation
                    .checked_add(estimated_mask_bytes)
                    .is_some_and(|value| {
                        value <= self.scheduler.limits().max_combined_request_bytes.get()
                    }) {
                estimated_mask_bytes
            } else {
                0
            };
            if normalized_bytes > self.scheduler.limits().max_normalized_bytes.get()
                || base_reservation
                    .checked_add(shared_mask_bytes)
                    .is_none_or(|value| {
                        value > self.scheduler.limits().max_combined_request_bytes.get()
                    })
            {
                let error = limit_error("artifact generator exceeds memory limits");
                for slot in group {
                    result.insert(slot.0.cache.cache_key, Err(error.clone()));
                }
                continue;
            }
            let memory = match self
                .scheduler
                .acquire_memory(base_reservation, deadline, &cancellation)
                .await
            {
                Ok(memory) => memory,
                Err(error) if !is_fatal(&error) => {
                    for slot in group {
                        result.insert(slot.0.cache.cache_key, Err(error.clone()));
                    }
                    continue;
                }
                Err(error) => return Err(error),
            };
            let plan_for_decode = plan.clone();
            let adaptation = self.scheduler.limits().adaptation();
            let token = cancellation.clone();
            let epoch = match self
                .scheduler
                .run_blocking(deadline, &cancellation, move || {
                    decode_plan(plan_for_decode, adaptation, &token)
                })
                .await
                .map(Arc::new)
            {
                Ok(input) => input,
                Err(error) => {
                    drop(memory);
                    if is_fatal(&error) {
                        return Err(error);
                    }
                    for slot in group {
                        result.insert(slot.0.cache.cache_key, Err(error.clone()));
                    }
                    continue;
                }
            };
            let prepared = Arc::clone(&group[0].0.prepared);
            let normalized_sequence = if normalization_identity(&prepared)?.is_some() {
                let epoch_for_normalize = Arc::clone(&epoch);
                let prepared_for_normalize = Arc::clone(&prepared);
                let limits = self.scheduler.limits();
                match self
                    .scheduler
                    .run_blocking(deadline, &cancellation, move || {
                        normalize(&epoch_for_normalize, &prepared_for_normalize, limits)
                    })
                    .await
                {
                    Ok(Some(value)) => Some(value),
                    Ok(None) => None,
                    Err(error) => {
                        drop(memory);
                        if is_fatal(&error) {
                            return Err(error);
                        }
                        for slot in group {
                            result.insert(slot.0.cache.cache_key, Err(error.clone()));
                        }
                        continue;
                    }
                }
            } else {
                None
            };

            let shared_analysis = if let Some(key) = cohort_key
                .filter(|key| cohort_counts.get(key).copied().unwrap_or(0) > 1)
                .filter(|_| estimated_mask_bytes == 0 || shared_mask_bytes > 0)
            {
                let want_change_masks = cohort_wants_masks.get(&key).copied().unwrap_or(false);
                if let Some(existing) = shared_analyses.get(&key) {
                    Some(Arc::clone(existing))
                } else if let Some(normalized) = normalized_sequence.as_ref() {
                    let mask_permit = if shared_mask_bytes > 0 {
                        match self
                            .scheduler
                            .acquire_memory(shared_mask_bytes, deadline, &cancellation)
                            .await
                        {
                            Ok(permit) => Some(permit),
                            Err(error) if is_fatal(&error) => return Err(error),
                            Err(_) => None,
                        }
                    } else {
                        None
                    };
                    if shared_mask_bytes > 0 && mask_permit.is_none() {
                        None
                    } else {
                        let normalized_for_analysis = Arc::clone(normalized);
                        let measurement = temporal_vision::MeasurementParameters::new(key.2);
                        let token = cancellation.clone();
                        let built = self
                            .scheduler
                            .run_blocking(deadline, &cancellation, move || {
                                token.check()?;
                                analyze_adjacent_pairs(
                                    &normalized_for_analysis,
                                    measurement,
                                    want_change_masks,
                                )
                                .map_err(vision_error)
                            })
                            .await;
                        match built {
                            Ok(value) => {
                                let value = Arc::new(value);
                                if let Some(permit) = mask_permit {
                                    shared_memory_permits.push(permit);
                                }
                                shared_analyses.insert(key, Arc::clone(&value));
                                Some(value)
                            }
                            Err(error) if is_fatal(&error) => return Err(error),
                            Err(_) => None,
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let ids: Vec<_> = prepared
                .request
                .output_kinds()
                .iter()
                .map(|_| ArtifactId::from_uuid(*self.ids.next().as_uuid()))
                .collect();
            let _generator = self
                .scheduler
                .acquire_generator(Arc::clone(&generator_semaphore), deadline, &cancellation)
                .await?;
            let epoch_for_generate = Arc::clone(&epoch);
            let prepared_for_generate = Arc::clone(&prepared);
            let normalized_for_generate = normalized_sequence.clone();
            let limits = self.scheduler.limits();
            let generated = self
                .scheduler
                .run_blocking(deadline, &cancellation, move || {
                    generate(
                        &epoch_for_generate,
                        &prepared_for_generate,
                        &ids,
                        normalized_for_generate.as_deref(),
                        shared_analysis.as_deref(),
                        limits,
                    )
                })
                .await;
            let outputs = match generated {
                Ok(outputs) => outputs,
                Err(error) => {
                    if is_fatal(&error) {
                        return Err(error);
                    }
                    for slot in group {
                        result.insert(slot.0.cache.cache_key, Err(error.clone()));
                    }
                    continue;
                }
            };
            drop(memory);
            let by_kind: HashMap<_, _> = outputs
                .into_iter()
                .map(|output| (output.kind, output))
                .collect();
            for slot in group {
                cancellation.check()?;
                let Some(output) = by_kind.get(&slot.0.kind) else {
                    result.insert(
                        slot.0.cache.cache_key,
                        Err(generation_error(
                            "generator omitted requested artifact kind",
                        )),
                    );
                    continue;
                };
                total_output_bytes = total_output_bytes
                    .checked_add(output.bytes.len())
                    .ok_or_else(|| limit_error("generated output byte count overflows"))?;
                if total_output_bytes > self.scheduler.limits().max_output_bytes_total.get() {
                    result.insert(
                        slot.0.cache.cache_key,
                        Err(limit_error("generated outputs exceed total byte limit")),
                    );
                    continue;
                }
                cancellation.check()?;
                let publication = ArtifactPublication::new(
                    plans[slot.0.epoch_index].frames[0].metadata().session_id(),
                    plans[slot.0.epoch_index].frames[0].metadata().target_id(),
                    slot.0.sources.clone(),
                    slot.0.cache.clone(),
                    output.manifest.clone(),
                    NonEmptyText::new("image/png").unwrap(),
                    Arc::clone(&output.bytes),
                )?
                .with_cancellation(Arc::new(cancellation.clone()));
                match self.artifacts.publish_artifact(publication).await {
                    Ok(ArtifactPublish::Published(artifact, grace_overridden)) => {
                        result.insert(
                            slot.0.cache.cache_key,
                            Ok(FlightValue {
                                artifact: artifact.into(),
                                generated: true,
                                grace_overridden,
                            }),
                        );
                    }
                    Ok(ArtifactPublish::Existing(artifact)) => {
                        result.insert(
                            slot.0.cache.cache_key,
                            Ok(FlightValue {
                                artifact: artifact.into(),
                                generated: false,
                                grace_overridden: false,
                            }),
                        );
                    }
                    Err(error) if is_fatal(&error) => return Err(error),
                    Err(error) => {
                        result.insert(slot.0.cache.cache_key, Err(error));
                    }
                }
            }
        }
        Ok(result)
    }
}

pub(crate) fn select_epoch_plans(
    plans: Vec<EpochPlan>,
    selection: ArtifactEpochSelection,
) -> Result<Vec<EpochPlan>> {
    let ArtifactEpochSelection::Anchor(anchor) = selection else {
        return Ok(plans);
    };
    let anchor = anchor.as_nanos();
    let selected = plans
        .iter()
        .enumerate()
        .min_by_key(|(_, plan)| {
            let start = plan
                .frames
                .first()
                .expect("validated epoch plan is non-empty")
                .metadata()
                .session_time()
                .as_nanos();
            let end = plan
                .frames
                .last()
                .expect("validated epoch plan is non-empty")
                .metadata()
                .session_time()
                .as_nanos();
            let distance = if anchor < start {
                start - anchor
            } else {
                anchor.saturating_sub(end)
            };
            (distance, plan.descriptor.index)
        })
        .map(|(index, _)| index)
        .ok_or_else(|| generation_error("artifact planning produced no visual epochs"))?;
    Ok(vec![
        plans
            .into_iter()
            .nth(selected)
            .expect("selected epoch plan exists"),
    ])
}

fn plan_for_generator(
    generator: &krometrail_core::ArtifactGeneratorRequest,
    plan: &EpochPlan,
    limits: ArtifactWorkLimits,
) -> Result<EpochPlan> {
    let generator_plan = match generator {
        krometrail_core::ArtifactGeneratorRequest::Storyboard(request) => {
            bounded_plan(plan, usize::from(request.tile_limit), None)
        }
        krometrail_core::ArtifactGeneratorRequest::RegionFilmstrip(request) => {
            bounded_plan(plan, usize::from(request.tile_limit), request.locator)
        }
        krometrail_core::ArtifactGeneratorRequest::DifferenceMap(request) => {
            let effective_max_frames = analysis_effective_max_frames(plan, limits)?;
            if matches!(
                request.frequency_mode,
                temporal_vision::FrequencyMode::Count | temporal_vision::FrequencyMode::Magnitude
            ) && plan.frames.len() > effective_max_frames
            {
                Err(resource_limit_error(
                    "count-mode and magnitude-mode difference map source frames",
                    plan.frames.len(),
                    effective_max_frames,
                    "narrow the range, or switch to frequency_mode normalized_frequency",
                ))
            } else {
                plan_for_analysis_sampling(
                    request.sampling,
                    plan,
                    limits,
                    effective_max_frames,
                    explicit_reference_frame(request.reference),
                )
            }
        }
        krometrail_core::ArtifactGeneratorRequest::MotionHistory(request) => {
            let effective_max_frames = analysis_effective_max_frames(plan, limits)?;
            plan_for_analysis_sampling(
                request.sampling,
                plan,
                limits,
                effective_max_frames,
                explicit_reference_frame(request.reference),
            )
        }
    }?;
    if generator_plan.decoded_bytes > limits.max_decoded_bytes.get() {
        return Err(resource_limit_error(
            "decoded source-frame bytes",
            generator_plan.decoded_bytes,
            limits.max_decoded_bytes.get(),
            "narrow the range; normalization.scale does not reduce this limit because source frames are decoded at their original dimensions before normalization",
        ));
    }
    Ok(generator_plan)
}

/// Only an explicitly named reference frame has to survive bounded sampling.
/// `First` and `Last` are always retained because uniform selection keeps both
/// endpoints of the source plan.
fn explicit_reference_frame(
    selector: krometrail_core::FrameSelector,
) -> Option<krometrail_core::FrameId> {
    match selector {
        krometrail_core::FrameSelector::Frame(id) => Some(id),
        krometrail_core::FrameSelector::First | krometrail_core::FrameSelector::Last => None,
    }
}

fn plan_for_analysis_sampling(
    sampling: krometrail_core::ArtifactSampling,
    plan: &EpochPlan,
    limits: ArtifactWorkLimits,
    effective_max_frames: usize,
    include_frame_id: Option<krometrail_core::FrameId>,
) -> Result<EpochPlan> {
    match sampling {
        krometrail_core::ArtifactSampling::Exhaustive => {
            if plan.frames.len() > effective_max_frames {
                Err(resource_limit_error(
                    "exhaustive analysis source plan",
                    format!(
                        "{} frames and {} decoded bytes",
                        plan.frames.len(),
                        plan.decoded_bytes
                    ),
                    format!(
                        "{} frames and {} decoded bytes",
                        limits.max_source_frames.get(),
                        limits.max_decoded_bytes.get()
                    ),
                    format!(
                        "narrow the resolved range so at most {} frames fall inside it, or use uniform_bounded sampling which analyzes a bounded subset of any range",
                        limits.max_source_frames.get()
                    ),
                ))
            } else {
                Ok(plan.clone())
            }
        }
        krometrail_core::ArtifactSampling::UniformBounded => {
            bounded_plan(plan, effective_max_frames, include_frame_id)
        }
    }
}

pub(crate) fn analysis_effective_max_frames(
    plan: &EpochPlan,
    limits: ArtifactWorkLimits,
) -> Result<usize> {
    let per_frame_decoded_bytes = plan.frames.iter().try_fold(0, |maximum, frame| {
        Ok::<_, KrometrailError>(maximum.max(super::epoch::decoded_len(frame)?))
    })?;
    // The non-empty-plan invariant already keeps this above zero; the floor makes the
    // division structurally safe rather than invariant-dependent.
    let byte_frame_limit = limits.max_decoded_bytes.get() / per_frame_decoded_bytes.max(1);
    Ok(limits.max_source_frames.get().min(byte_frame_limit).max(1))
}

fn estimated_change_mask_bytes(plan: &EpochPlan) -> Result<usize> {
    let pixels = usize::try_from(plan.descriptor.image.width())
        .ok()
        .and_then(|width| {
            usize::try_from(plan.descriptor.image.height())
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| limit_error("change-mask byte estimate overflows"))?;
    let bytes_per_pair = pixels
        .checked_add(7)
        .and_then(|value| value.checked_div(8))
        .ok_or_else(|| limit_error("change-mask byte estimate overflows"))?;
    plan.frames
        .len()
        .saturating_sub(1)
        .checked_mul(bytes_per_pair)
        .ok_or_else(|| limit_error("change-mask byte estimate overflows"))
}

fn analysis_plan_identity(plan: &EpochPlan) -> Vec<(FrameId, usize)> {
    plan.frames
        .iter()
        .zip(&plan.source_indices)
        .map(|(frame, source_index)| (frame.metadata().id(), *source_index))
        .collect()
}

impl ArtifactGeneration for TemporalVisionArtifactService {
    fn generate(
        &self,
        request: ArtifactGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<ArtifactGenerationResult>> {
        let service = self.clone();
        Box::pin(async move { service.generate_inner(request, context).await })
    }
}

fn handle(artifact: FlightArtifact, disposition: ArtifactCacheDisposition) -> Box<ArtifactHandle> {
    Box::new(ArtifactHandle {
        artifact_id: *artifact.manifest.artifact_id(),
        cache: disposition,
        media_type: artifact.media_type,
        encoded_byte_len: artifact.encoded_byte_len,
        manifest: artifact.manifest,
    })
}

fn is_fatal(error: &KrometrailError) -> bool {
    matches!(error.code, ErrorCode::Cancelled | ErrorCode::NotFound)
}

fn generation_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message).unwrap(),
    )
}

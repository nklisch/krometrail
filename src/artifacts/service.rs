use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Instant,
};

use krometrail_core::{
    ArtifactCacheDisposition, ArtifactEpochSelection, ArtifactGeneration,
    ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGenerationResult, ArtifactHandle,
    ArtifactId, ArtifactLookup, ArtifactOutcome, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, ErrorCode, FrameSource, IdSource, KrometrailError,
    NonEmptyText, PortFuture, Result,
};
use temporal_vision::{ArtifactKind, generator_descriptor};

use super::{
    cache::{CacheIdentityInput, cache_metadata},
    decode::DECODER_PROFILE,
    epoch::{
        ADAPTER_VERSION, EpochPlan, WorkCancellation, bounded_plan, decode_plan, validate_and_plan,
    },
    generators::{
        PreparedGenerator, estimated_normalized_bytes, generate, normalization_identity, normalize,
        prepare_generator, reserved_output_bytes,
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

struct Available {
    artifact: FlightArtifact,
    disposition: ArtifactCacheDisposition,
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
                match prepare_generator(generator, &generator_plan, limits) {
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
            let reservation = plan
                .decoded_bytes
                .checked_add(normalized_bytes)
                .and_then(|value| value.checked_add(output_reservation))
                .ok_or_else(|| limit_error("combined artifact memory estimate overflows"))?;
            if normalized_bytes > self.scheduler.limits().max_normalized_bytes.get()
                || reservation > self.scheduler.limits().max_combined_request_bytes.get()
            {
                let error = limit_error("artifact generator exceeds memory limits");
                for slot in group {
                    result.insert(slot.0.cache.cache_key, Err(error.clone()));
                }
                continue;
            }
            let memory = match self
                .scheduler
                .acquire_memory(reservation, deadline, &cancellation)
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
                    Ok(ArtifactPublish::Published(artifact)) => {
                        result.insert(
                            slot.0.cache.cache_key,
                            Ok(FlightValue {
                                artifact: artifact.into(),
                                generated: true,
                            }),
                        );
                    }
                    Ok(ArtifactPublish::Existing(artifact)) => {
                        result.insert(
                            slot.0.cache.cache_key,
                            Ok(FlightValue {
                                artifact: artifact.into(),
                                generated: false,
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
                plan_for_analysis_sampling(request.sampling, plan, limits, effective_max_frames)
            }
        }
        krometrail_core::ArtifactGeneratorRequest::MotionHistory(request) => {
            let effective_max_frames = analysis_effective_max_frames(plan, limits)?;
            plan_for_analysis_sampling(request.sampling, plan, limits, effective_max_frames)
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

fn plan_for_analysis_sampling(
    sampling: krometrail_core::ArtifactSampling,
    plan: &EpochPlan,
    limits: ArtifactWorkLimits,
    effective_max_frames: usize,
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
                    "narrow the range or set sampling to uniform_bounded",
                ))
            } else {
                Ok(plan.clone())
            }
        }
        krometrail_core::ArtifactSampling::UniformBounded => {
            bounded_plan(plan, effective_max_frames, None)
        }
    }
}

fn analysis_effective_max_frames(plan: &EpochPlan, limits: ArtifactWorkLimits) -> Result<usize> {
    let per_frame_decoded_bytes = plan.frames.iter().try_fold(0, |maximum, frame| {
        Ok::<_, KrometrailError>(maximum.max(super::epoch::decoded_len(frame)?))
    })?;
    let byte_frame_limit = limits.max_decoded_bytes.get() / per_frame_decoded_bytes;
    Ok(limits.max_source_frames.get().min(byte_frame_limit).max(1))
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

fn handle(artifact: FlightArtifact, disposition: ArtifactCacheDisposition) -> ArtifactHandle {
    ArtifactHandle {
        artifact_id: *artifact.manifest.artifact_id(),
        cache: disposition,
        media_type: artifact.media_type,
        encoded_byte_len: artifact.encoded_byte_len,
        manifest: artifact.manifest,
    }
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

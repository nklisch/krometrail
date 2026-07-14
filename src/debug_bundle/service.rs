//! The one application service that composes a temporal debug bundle.
//!
//! Implements the exact seven-step sequence: validate and acquire a permit,
//! resolve the range exactly once, load bounded marker evidence, generate
//! artifacts at most once, extract focus from storyboard traces, query context
//! exactly once, and compose the deterministic non-diagnostic bundle. One
//! absolute deadline and cancellation signal guard every await. Fatal lifetime
//! errors discard partial work; independent component failures produce usable
//! degraded bundles when another component remains useful.

use std::{
    collections::BTreeMap,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

use krometrail_core::{
    ArtifactGeneration, ArtifactGenerationContext, ArtifactOutcome, BundleArtifactEvidence,
    BundleContextEvidence, BundleDegradation, BundleWarning, InteractionAnchor,
    InteractionAnchorSource, InteractionId, ObservationKind, ObservationPayloadRef, PortFuture,
    ResolvedAnchorReference, ResolvedRange, Result, TemporalContextQuery, TemporalContextRequest,
    TemporalDebugBundle, TemporalDebugBundleContext, TemporalDebugBundleRequest,
    TemporalDebugBundles, TemporalQuery, TimelineRangeQuery, TimelineRangeSlice, TimelineStore,
};
use std::num::NonZeroU16;
use tokio::sync::Semaphore;

use super::{
    BrowserEventEvidenceState, TemporalDebugEvidenceStore, VisualEvidenceState, assemble_markers,
    compose_header, controlled, default_artifact_request,
    error::{cancelled_error, evidence_lifetime_error, no_useful_evidence_error, permit_error},
    extract_focus_times,
};
use crate::debug_bundle::{MarkerEvidence, build_effective_policy};

/// Bounds concurrent bundle orchestration and the maximum wall time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BundleWorkLimits {
    pub max_active_requests: NonZeroUsize,
    pub max_wall_time: Duration,
}

impl Default for BundleWorkLimits {
    fn default() -> Self {
        Self {
            max_active_requests: NonZeroUsize::new(2).expect("default permit count is non-zero"),
            max_wall_time: Duration::from_secs(20),
        }
    }
}

/// The one bundle service over existing inward ports.
pub(crate) struct TemporalDebugBundleService {
    queries: Arc<dyn TemporalQuery>,
    timeline: Arc<dyn TemporalDebugEvidenceStore>,
    artifacts: Arc<dyn ArtifactGeneration>,
    context: Arc<dyn TemporalContextQuery>,
    permits: Arc<Semaphore>,
    limits: BundleWorkLimits,
}

impl TemporalDebugBundleService {
    pub(crate) fn new(
        queries: Arc<dyn TemporalQuery>,
        timeline: Arc<dyn TemporalDebugEvidenceStore>,
        artifacts: Arc<dyn ArtifactGeneration>,
        context: Arc<dyn TemporalContextQuery>,
        limits: BundleWorkLimits,
    ) -> Result<Self> {
        Ok(Self {
            queries,
            timeline,
            artifacts,
            context,
            permits: Arc::new(Semaphore::new(limits.max_active_requests.get())),
            limits,
        })
    }

    async fn bundle_inner(
        &self,
        request: TemporalDebugBundleRequest,
        context: TemporalDebugBundleContext,
    ) -> Result<TemporalDebugBundle> {
        // Step 1: compute the effective absolute deadline, check immediate
        // cancellation, and acquire one of the global active-bundle permits
        // through the same controlled cancellation/deadline wrapper that
        // guards every later await. The permit bounds concurrent orchestration
        // independently of the artifact service's own permits. Acquiring the
        // permit through `controlled` lets a queued request time out or be
        // cancelled without waiting for an in-flight bundle to release first.
        let now = Instant::now();
        let wall_deadline = now + self.limits.max_wall_time;
        let bundle_deadline = context
            .deadline
            .map(|caller| caller.min(wall_deadline))
            .unwrap_or(wall_deadline);
        if bundle_deadline <= now || context.is_cancelled() {
            return Err(cancelled_error());
        }
        let cancellation = context.cancellation.as_ref();
        let permit = controlled(self.permits.acquire(), bundle_deadline, cancellation).await;
        let _permit = match permit {
            Ok(Ok(acquired)) => acquired,
            // The semaphore was closed; this is fatal but not cancellation.
            Ok(Err(_)) => return Err(permit_error()),
            // Controlled wrapper fired cancellation or the bundle deadline.
            Err(controlled_error) => return Err(controlled_error),
        };

        // Step 2: resolve the range exactly once. Range failure is whole-request
        // failure; the owned query is cloned so the original can be returned in
        // the bundle's requested_query field.
        let (query, caller_markers, orientation) = request.into_parts();
        let range = controlled(
            self.queries.resolve_range(query.clone()),
            bundle_deadline,
            cancellation,
        )
        .await??;

        // Step 3: load bounded marker evidence. Marker-context failure is a
        // degradation, not fatal: the bundle continues with caller markers and
        // the mandatory anchor marker. All store reads complete before visual work.
        let marker_evidence = self
            .load_marker_evidence(&range, bundle_deadline, cancellation)
            .await;
        let (markers, marker_warnings, marker_degradation) = match marker_evidence {
            Ok(evidence) => {
                let assembled = assemble_markers(MarkerEvidence {
                    range: &range,
                    caller_markers: &caller_markers,
                    timeline: &evidence.timeline,
                    interactions: &evidence.interactions,
                })?;
                (assembled.markers, assembled.warnings, None)
            }
            Err(error) => {
                // Fatal cancellation/deadline must not degrade.
                if error.code == krometrail_core::ErrorCode::Cancelled
                    || Instant::now() >= bundle_deadline
                {
                    return Err(error);
                }
                let empty = empty_timeline_slice();
                let assembled = assemble_markers(MarkerEvidence {
                    range: &range,
                    caller_markers: &caller_markers,
                    timeline: &empty,
                    interactions: &BTreeMap::new(),
                })?;
                (assembled.markers, assembled.warnings, Some(error))
            }
        };

        // Step 4: materialize the exact v1 artifact request and generate at most
        // once. The artifact service receives the same absolute deadline and
        // cancellation through ArtifactGenerationContext.
        let artifact_request = default_artifact_request(&range, &markers, orientation)?;
        let artifact_context = ArtifactGenerationContext {
            deadline: Some(bundle_deadline),
            cancellation: context.cancellation.clone(),
        };
        let artifact_result = controlled(
            self.artifacts.generate(artifact_request, artifact_context),
            bundle_deadline,
            cancellation,
        )
        .await;
        let (artifact_evidence, artifact_outcomes) = match artifact_result {
            Ok(Ok(result)) => (
                BundleArtifactEvidence::Available(result.clone()),
                result.outcomes,
            ),
            Ok(Err(error)) => {
                if is_fatal_after_resolution(&error, bundle_deadline) {
                    return Err(evidence_lifetime_error(&range));
                }
                (
                    BundleArtifactEvidence::Unavailable {
                        error: error.clone(),
                    },
                    Vec::new(),
                )
            }
            Err(error) => return Err(error), // controlled timeout/cancel → fatal
        };

        // Step 5: extract at most 16 focus times only from available storyboard
        // manifests. Never invokes measurement or selection APIs.
        let focus_times = extract_focus_times(&artifact_outcomes);
        let has_focus = !focus_times.is_empty();

        // Step 6: construct the compact context request with the same resolved
        // range, default all-class/debug filter, compact limit 24, and the focus
        // times. Query context exactly once.
        let effective = build_effective_policy(&range, orientation, focus_times)?;
        let context_request = TemporalContextRequest::new(
            range.clone(),
            None,
            effective.event_filter.clone(),
            effective.event_selection.clone(),
            effective.focus_times.clone(),
        )?;
        let context_result = controlled(
            self.context.context(context_request),
            bundle_deadline,
            cancellation,
        )
        .await;
        let context_evidence = match context_result {
            Ok(Ok(context)) => BundleContextEvidence::Available(context),
            Ok(Err(error)) => {
                if is_fatal_after_resolution(&error, bundle_deadline) {
                    return Err(evidence_lifetime_error(&range));
                }
                BundleContextEvidence::Unavailable { error }
            }
            Err(error) => return Err(error), // controlled timeout/cancel → fatal
        };

        // Step 7: compose the deterministic bundle. Fail if no useful evidence
        // remains (both artifact outcomes and context are unavailable).
        let mut degradations = Vec::new();
        if let Some(error) = marker_degradation {
            degradations.push(BundleDegradation::MarkerContextUnavailable { error });
        }
        if let BundleArtifactEvidence::Available(result) = &artifact_evidence {
            let total = u16::try_from(result.outcomes.len()).unwrap_or(u16::MAX);
            let unavailable = u16::try_from(
                result
                    .outcomes
                    .iter()
                    .filter(|o| matches!(o, ArtifactOutcome::Unavailable { .. }))
                    .count(),
            )
            .unwrap_or(u16::MAX);
            if unavailable > 0 && unavailable <= total {
                degradations
                    .push(BundleDegradation::ArtifactOutcomesUnavailable { unavailable, total });
            }
        } else {
            degradations.push(BundleDegradation::ArtifactRequestUnavailable);
        }
        if matches!(context_evidence, BundleContextEvidence::Unavailable { .. }) {
            degradations.push(BundleDegradation::ContextUnavailable);
        }

        let artifact_has_outcome = match &artifact_evidence {
            BundleArtifactEvidence::Available(result) => result
                .outcomes
                .iter()
                .any(|o| matches!(o, ArtifactOutcome::Available { .. })),
            BundleArtifactEvidence::Unavailable { .. } => false,
        };
        if !artifact_has_outcome
            && matches!(context_evidence, BundleContextEvidence::Unavailable { .. })
        {
            return Err(no_useful_evidence_error(&range));
        }

        let visual_state = classify_visual_evidence(&artifact_outcomes, has_focus);
        let browser_event_state = match &context_evidence {
            BundleContextEvidence::Available(context) => BrowserEventEvidenceState::Available {
                selected: context.browser_events.returned_count,
            },
            BundleContextEvidence::Unavailable { .. } => BrowserEventEvidenceState::Unavailable,
        };
        let header = compose_header(
            &range,
            &artifact_outcomes,
            visual_state,
            browser_event_state,
        )?;
        let mut warnings = Vec::new();
        if range.resolved_anchor.requested_time != range.resolved_anchor.effective_time {
            warnings.push(BundleWarning::AnchorAdjustedForRetention {
                requested: range.resolved_anchor.requested_time,
                effective: range.resolved_anchor.effective_time,
            });
        }
        warnings.extend(marker_warnings);
        if !has_focus {
            warnings.push(BundleWarning::NoMajorVisualChangeFocus);
        }

        TemporalDebugBundle::new(
            query,
            range,
            effective,
            header,
            markers,
            artifact_evidence,
            context_evidence,
            warnings,
            degradations,
        )
    }

    /// Loads the bounded marker timeline and selected interaction anchors.
    /// All store reads complete (return) before artifact generation begins.
    async fn load_marker_evidence(
        &self,
        range: &ResolvedRange,
        deadline: Instant,
        cancellation: Option<&Arc<dyn krometrail_core::CancellationSignal>>,
    ) -> std::result::Result<MarkerLoad, krometrail_core::KrometrailError> {
        let query = TimelineRangeQuery::new(
            range.session_id,
            range.target_id,
            range.resolved_range,
            vec![
                ObservationKind::InteractionBoundary,
                ObservationKind::Navigation,
                ObservationKind::Marker,
            ],
            NonZeroU16::new(krometrail_core::MAX_BUNDLE_TIMELINE_ROWS)
                .expect("bundle timeline row cap is non-zero"),
        )?;
        let timeline =
            controlled(self.timeline.selected_range(query), deadline, cancellation).await??;

        let mut interactions: BTreeMap<InteractionId, InteractionAnchor> = BTreeMap::new();
        for observation in &timeline.observations {
            if let ObservationPayloadRef::Interaction(interaction_id) = observation.payload() {
                if interactions.contains_key(interaction_id) {
                    continue;
                }
                if let Some(anchor) = controlled(
                    self.timeline.interaction_anchor(*interaction_id),
                    deadline,
                    cancellation,
                )
                .await??
                {
                    interactions.insert(*interaction_id, anchor);
                }
            }
        }
        // Ensure the resolved anchor's interaction is loaded for the mandatory
        // anchor marker label, even if its boundary observation was truncated.
        if let ResolvedAnchorReference::Interaction { interaction_id } =
            &range.resolved_anchor.reference
            && !interactions.contains_key(interaction_id)
        {
            if let Some(anchor) = controlled(
                self.timeline.interaction_anchor(*interaction_id),
                deadline,
                cancellation,
            )
            .await??
            {
                interactions.insert(*interaction_id, anchor);
            }
        }
        Ok(MarkerLoad {
            timeline,
            interactions,
        })
    }
}

struct MarkerLoad {
    timeline: TimelineRangeSlice,
    interactions: BTreeMap<InteractionId, InteractionAnchor>,
}

fn empty_timeline_slice() -> TimelineRangeSlice {
    TimelineRangeSlice {
        matched_count: 0,
        observations: Vec::new(),
        truncated: false,
    }
}

/// Returns true if an error after successful range resolution is fatal
/// (evidence lifetime, cancellation, or deadline elapsed).
fn is_fatal_after_resolution(error: &krometrail_core::KrometrailError, deadline: Instant) -> bool {
    if error.code == krometrail_core::ErrorCode::NotFound {
        return true;
    }
    if error.code == krometrail_core::ErrorCode::Cancelled {
        return true;
    }
    Instant::now() >= deadline
}

/// Classifies the typed visual evidence state from the available storyboard
/// outcomes and whether focus extraction produced any major-change times.
///
/// This replaces a boolean `has_focus` flag at the header boundary so an
/// unavailable storyboard is never reported as "no visual change" — that would
/// assert a measurement the bundle never made. Only an available storyboard
/// trace without measured change may be reported as `MeasuredNoChange`.
fn classify_visual_evidence(outcomes: &[ArtifactOutcome], has_focus: bool) -> VisualEvidenceState {
    let has_storyboard_trace = outcomes.iter().any(|outcome| match outcome {
        ArtifactOutcome::Available { artifact, .. }
            if artifact.manifest.artifact_kind() == temporal_vision::ArtifactKind::Storyboard =>
        {
            artifact.manifest.storyboard_selection().is_some()
        }
        _ => false,
    });
    if !has_storyboard_trace {
        return VisualEvidenceState::Unavailable;
    }
    if has_focus {
        VisualEvidenceState::MeasuredChange
    } else {
        VisualEvidenceState::MeasuredNoChange
    }
}

impl TemporalDebugBundles for TemporalDebugBundleService {
    fn bundle(
        &self,
        request: TemporalDebugBundleRequest,
        context: TemporalDebugBundleContext,
    ) -> PortFuture<'_, Result<TemporalDebugBundle>> {
        let service = self.clone();
        Box::pin(async move { service.bundle_inner(request, context).await })
    }
}

impl Clone for TemporalDebugBundleService {
    fn clone(&self) -> Self {
        Self {
            queries: Arc::clone(&self.queries),
            timeline: Arc::clone(&self.timeline),
            artifacts: Arc::clone(&self.artifacts),
            context: Arc::clone(&self.context),
            permits: Arc::clone(&self.permits),
            limits: self.limits,
        }
    }
}

// Suppress unused-import warning for re-exported types that the service uses
// through trait methods rather than direct reference.
#[allow(unused_imports)]
use super::error::{cancelled_error as _, deadline_error as _};

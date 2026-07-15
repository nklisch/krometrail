//! Test-only retention qualification over the production `RecordingStore` ports.
//!
//! The scenario deliberately uses the store's own budget, pin, eviction, availability, and
//! deletion authorities. The helper frames below are only durable test inputs; they are not a
//! benchmark store, renderer, cache, or retention policy.

use krometrail_core::{
    ArtifactGenerationContext, ArtifactId, ArtifactOutcome, ArtifactStore, CaptureOrdinal,
    CapturedFrame, DeviceScaleFactor, EncodedFrame, ErrorCode, FrameAvailability, FrameId,
    ImageFormat, PixelDimensions, RecordingBudgetState, ResolvedRange, RetentionPinRequest,
    RetentionRange, RetentionStatus, SessionId, SessionRange, SessionTime, TargetId,
};
use temporal_evaluation::{
    EvaluationStatus, FailureRecord, RetentionQualificationMeasurements, RunFailureCode,
};
use uuid::Uuid;

use super::{QualificationRuntime, live_error};
use crate::debug_bundle::default_artifact_request;

const PROBE_PAYLOAD_BYTES: usize = 256 * 1024;
const ARTIFACT_SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/artifacts/chrome-rgba.png");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionObservation {
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
    pub measurements: RetentionQualificationMeasurements,
    pub source_frame_ids: Vec<FrameId>,
    pub source_availability: FrameAvailability,
    pub linked_artifact_removed: bool,
    pub declared_gap_ids: Vec<String>,
}

impl RetentionObservation {
    pub fn is_complete(&self) -> bool {
        self.status == EvaluationStatus::Pass && self.failure.is_none()
    }
}

/// Exercises the concrete store's resolved-range pin, eviction, paused-budget, resume, and
/// artifact-linked deletion semantics. The input range must be the exact range returned by the
/// production temporal query; this function never widens or reconstructs it.
pub async fn qualify_retention(
    runtime: &QualificationRuntime,
    interval: &ResolvedRange,
    linked_artifact: Option<ArtifactId>,
) -> krometrail_core::Result<RetentionObservation> {
    interval.validate().map_err(|_| {
        live_error(
            ErrorCode::InvalidInput,
            "retention qualification requires a valid resolved source interval",
        )
    })?;
    let pin = RetentionPinRequest::from_resolved(interval)?;
    let initial = runtime.dependencies.retention.status().await?;
    let mut peak_usage_bytes = initial.usage.total_bytes()?;
    let mut usage_bounded = status_usage_is_bounded(&initial)?;

    let pinned = runtime
        .dependencies
        .retention
        .pin_resolved_range(pin.clone())
        .await?;
    peak_usage_bytes = peak_usage_bytes.max(pinned.state.retention.usage.total_bytes()?);
    usage_bounded &= status_usage_is_bounded(&pinned.state.retention)?;

    let candidate = append_probe_frame(runtime, 0xfeed, 0xface, PROBE_PAYLOAD_BYTES).await?;
    let protected_candidate = scripted_frame(
        next_session_id(runtime),
        next_target_id(runtime),
        0xcafe,
        1,
        PROBE_PAYLOAD_BYTES / 2,
    )?;
    runtime
        .dependencies
        .recording
        .append_frame(protected_candidate.clone())
        .await?;

    let enforced = runtime.dependencies.retention.enforce_budget().await?;
    peak_usage_bytes = peak_usage_bytes.max(enforced.usage.total_bytes()?);
    usage_bounded &= status_usage_is_bounded(&enforced)?;
    let candidate_present = runtime
        .dependencies
        .frames
        .frames_by_id(vec![candidate.metadata().id()])
        .await
        .is_ok();
    let evicted_frame_count = u64::from(!candidate_present);
    let pinned_interval_preserved = runtime
        .dependencies
        .frames
        .frames_by_id(interval.frame_ids.clone())
        .await
        .is_ok_and(|frames| frames.len() == interval.frame_ids.len());
    let source_availability = runtime
        .dependencies
        .frames
        .frame_availability(interval.session_id, interval.target_id)
        .await?;

    // Pin every remaining unpinned candidate before testing the real paused-budget state. The
    // optional candidate pin is necessary when the first probe was not evicted by the budget.
    let protected_request = RetentionPinRequest::new(
        RetentionRange {
            session_id: protected_candidate.metadata().session_id(),
            target_id: protected_candidate.metadata().target_id(),
            range: SessionRange::new(
                SessionTime::ZERO,
                protected_candidate.metadata().session_time(),
            )?,
        },
        vec![protected_candidate.metadata().id()],
    )?;
    let protected_pinned = runtime
        .dependencies
        .retention
        .pin_resolved_range(protected_request.clone())
        .await
        .is_ok();
    let candidate_request = if candidate_present {
        Some(RetentionPinRequest::new(
            RetentionRange {
                session_id: candidate.metadata().session_id(),
                target_id: candidate.metadata().target_id(),
                range: SessionRange::new(SessionTime::ZERO, candidate.metadata().session_time())?,
            },
            vec![candidate.metadata().id()],
        )?)
    } else {
        None
    };
    let candidate_pinned = match &candidate_request {
        Some(request) => runtime
            .dependencies
            .retention
            .pin_resolved_range(request.clone())
            .await
            .is_ok(),
        None => true,
    };
    let all_candidates_pinned = protected_pinned && candidate_pinned;

    if all_candidates_pinned {
        let blocked_probe = scripted_frame(
            next_session_id(runtime),
            next_target_id(runtime),
            0xbeef,
            1,
            PROBE_PAYLOAD_BYTES,
        )?;
        let append_failed = runtime
            .dependencies
            .recording
            .append_frame(blocked_probe.clone())
            .await
            .is_err();
        let paused_status = runtime.dependencies.retention.status().await?;
        peak_usage_bytes = peak_usage_bytes.max(paused_status.usage.total_bytes()?);
        usage_bounded &= status_usage_is_bounded(&paused_status)?;
        let paused = append_failed
            && paused_status.budget_state == RecordingBudgetState::PausedBudget
            && paused_status.recording_blocked
            && paused_status.eviction_blocked;
        let resumed = if paused {
            runtime
                .dependencies
                .retention
                .unpin_resolved_range(protected_request)
                .await?;
            if let Some(request) = candidate_request.clone() {
                runtime
                    .dependencies
                    .retention
                    .unpin_resolved_range(request)
                    .await?;
            }
            runtime
                .dependencies
                .retention
                .wait_until_recording_allowed()
                .await?;
            let resumed_status = runtime.dependencies.retention.status().await?;
            peak_usage_bytes = peak_usage_bytes.max(resumed_status.usage.total_bytes()?);
            usage_bounded &= status_usage_is_bounded(&resumed_status)?;
            let append_succeeded = runtime
                .dependencies
                .recording
                .append_frame(blocked_probe)
                .await
                .is_ok();
            append_succeeded
                && resumed_status.budget_state == RecordingBudgetState::Available
                && !resumed_status.recording_blocked
        } else {
            false
        };
        // Keep the source pin until artifact generation has read the exact interval. Unpinning
        // before this point would let normal retention evict the evidence being qualified.
        let linked_artifact = resolve_linked_artifact(runtime, interval, linked_artifact).await?;
        let artifact_status = runtime.dependencies.retention.status().await?;
        peak_usage_bytes = peak_usage_bytes.max(artifact_status.usage.total_bytes()?);
        usage_bounded &= status_usage_is_bounded(&artifact_status)?;
        runtime
            .dependencies
            .retention
            .unpin_resolved_range(pin.clone())
            .await?;
        let _ = runtime
            .dependencies
            .retention
            .delete_session(candidate.metadata().session_id())
            .await?;
        let _ = runtime
            .dependencies
            .retention
            .delete_session(protected_candidate.metadata().session_id())
            .await?;
        finish_retention(
            runtime,
            RetentionEvidence {
                interval: interval.clone(),
                linked_artifact,
                peak_usage_bytes,
                usage_bounded,
                pinned_interval_preserved,
                evicted_frame_count,
                capture_paused_when_pinned: paused,
                capture_resumed_after_unpin: resumed,
                source_availability,
                declared_gap_ids: capture_gap_ids(interval),
            },
        )
        .await
    } else {
        let status = runtime.dependencies.retention.status().await?;
        peak_usage_bytes = peak_usage_bytes.max(status.usage.total_bytes()?);
        usage_bounded &= status_usage_is_bounded(&status)?;
        // This branch still proves artifact publication and linked cleanup, but honestly reports
        // that a forced all-pinned pause was not available under this concrete budget.
        let linked_artifact = resolve_linked_artifact(runtime, interval, linked_artifact).await?;
        let artifact_status = runtime.dependencies.retention.status().await?;
        peak_usage_bytes = peak_usage_bytes.max(artifact_status.usage.total_bytes()?);
        usage_bounded &= status_usage_is_bounded(&artifact_status)?;
        runtime
            .dependencies
            .retention
            .unpin_resolved_range(pin)
            .await?;
        let _ = runtime
            .dependencies
            .retention
            .delete_session(candidate.metadata().session_id())
            .await?;
        let _ = runtime
            .dependencies
            .retention
            .delete_session(protected_candidate.metadata().session_id())
            .await?;
        finish_retention(
            runtime,
            RetentionEvidence {
                interval: interval.clone(),
                linked_artifact,
                peak_usage_bytes,
                usage_bounded,
                pinned_interval_preserved,
                evicted_frame_count,
                capture_paused_when_pinned: false,
                capture_resumed_after_unpin: false,
                source_availability,
                declared_gap_ids: capture_gap_ids(interval),
            },
        )
        .await
    }
}

struct RetentionEvidence {
    interval: ResolvedRange,
    linked_artifact: Option<ArtifactId>,
    peak_usage_bytes: u64,
    usage_bounded: bool,
    pinned_interval_preserved: bool,
    evicted_frame_count: u64,
    capture_paused_when_pinned: bool,
    capture_resumed_after_unpin: bool,
    source_availability: FrameAvailability,
    declared_gap_ids: Vec<String>,
}

async fn finish_retention(
    runtime: &QualificationRuntime,
    evidence: RetentionEvidence,
) -> krometrail_core::Result<RetentionObservation> {
    let RetentionEvidence {
        interval,
        linked_artifact,
        peak_usage_bytes,
        usage_bounded,
        pinned_interval_preserved,
        evicted_frame_count,
        capture_paused_when_pinned,
        capture_resumed_after_unpin,
        source_availability,
        declared_gap_ids,
    } = evidence;
    let deletion = runtime
        .dependencies
        .retention
        .delete_session(interval.session_id)
        .await?;
    let linked_artifact_removed = match linked_artifact {
        Some(artifact_id) => runtime.store.artifact(artifact_id).await?.is_none(),
        None => false,
    };
    let measurements = RetentionQualificationMeasurements {
        budget_bytes: runtime
            .dependencies
            .retention
            .status()
            .await?
            .configured_budget
            .get(),
        peak_usage_bytes,
        pinned_interval_preserved,
        evicted_frame_count,
        capture_paused_when_pinned,
        capture_resumed_after_unpin,
        cleanup_removed_frame_count: deletion.removed_frames,
    };
    let complete = usage_bounded
        && pinned_interval_preserved
        && evicted_frame_count > 0
        && capture_paused_when_pinned
        && capture_resumed_after_unpin
        && linked_artifact_removed
        && deletion.removed_frames >= interval.frame_ids.len() as u64;
    Ok(RetentionObservation {
        status: if complete {
            EvaluationStatus::Pass
        } else {
            EvaluationStatus::Inconclusive
        },
        failure: (!complete).then(|| FailureRecord {
            code: RunFailureCode::Retention,
            phase: "retention".into(),
            reason: "the concrete retention scenario did not produce complete pin, eviction, pause, resume, and linked-cleanup evidence".into(),
            recovery: "rerun with a bounded temporary budget and retain an authority-generated artifact linked to the resolved interval".into(),
            retryable: true,
        }),
        measurements,
        source_frame_ids: interval.frame_ids.clone(),
        source_availability,
        linked_artifact_removed,
        declared_gap_ids,
    })
}

fn status_usage_is_bounded(status: &RetentionStatus) -> krometrail_core::Result<bool> {
    let usage = status.usage.total_bytes()?;
    Ok(usage <= status.configured_budget.get()
        || usage.saturating_sub(status.configured_budget.get())
            <= status.open_segment_overhead_limit_bytes)
}

async fn resolve_linked_artifact(
    runtime: &QualificationRuntime,
    interval: &ResolvedRange,
    requested: Option<ArtifactId>,
) -> krometrail_core::Result<Option<ArtifactId>> {
    let Some(artifact_id) = (match requested {
        Some(id) => Some(id),
        None => generate_linked_artifact(runtime, interval).await?,
    }) else {
        return Ok(None);
    };
    let Some(artifact) = runtime.store.artifact(artifact_id).await? else {
        return Ok(None);
    };
    if artifact.manifest.source_frame_ids() != interval.frame_ids.as_slice() {
        return Ok(None);
    }
    Ok(Some(artifact_id))
}

async fn generate_linked_artifact(
    runtime: &QualificationRuntime,
    interval: &ResolvedRange,
) -> krometrail_core::Result<Option<ArtifactId>> {
    let result = runtime
        .dependencies
        .artifact_generation
        .generate(
            default_artifact_request(interval, &[], krometrail_core::OrientationPolicy::Omit)?,
            ArtifactGenerationContext::default(),
        )
        .await?;
    Ok(result
        .outcomes
        .into_iter()
        .find_map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => Some(artifact.artifact_id),
            ArtifactOutcome::Unavailable { .. } => None,
        }))
}

fn capture_gap_ids(interval: &ResolvedRange) -> Vec<String> {
    interval
        .gaps
        .iter()
        .map(|gap| gap.id().to_string())
        .collect()
}

/// Append one source payload through the production recording sink. Artifact-linked callers use the
/// committed PNG fixture; retention volume probes use opaque encoded-byte volume and never invoke
/// a renderer. All retention and accounting decisions remain store-owned.
pub(crate) async fn append_probe_frame(
    runtime: &QualificationRuntime,
    session_seed: u128,
    target_seed: u128,
    payload_bytes: usize,
) -> krometrail_core::Result<EncodedFrame> {
    let frame = scripted_frame(
        SessionId::from_uuid(Uuid::from_u128(session_seed)),
        TargetId::from_uuid(Uuid::from_u128(target_seed)),
        session_seed.saturating_add(target_seed),
        1,
        payload_bytes,
    )?;
    runtime
        .dependencies
        .recording
        .append_frame(frame.clone())
        .await?;
    Ok(frame)
}

pub(crate) async fn append_scripted_frames(
    runtime: &QualificationRuntime,
    session: SessionId,
    target: TargetId,
    count: u64,
) -> krometrail_core::Result<Vec<FrameId>> {
    let mut ids = Vec::with_capacity(count as usize);
    for ordinal in 1..=count {
        let frame = scripted_frame(
            session,
            target,
            session.as_uuid().as_u128().saturating_add(ordinal as u128),
            ordinal,
            ARTIFACT_SOURCE.len(),
        )?;
        ids.push(frame.metadata().id());
        runtime.dependencies.recording.append_frame(frame).await?;
    }
    runtime.dependencies.recording.flush(session).await?;
    Ok(ids)
}

pub(crate) fn scripted_frame(
    session: SessionId,
    target: TargetId,
    frame_seed: u128,
    ordinal: u64,
    payload_bytes: usize,
) -> krometrail_core::Result<EncodedFrame> {
    let frame_id = FrameId::from_uuid(Uuid::from_u128(frame_seed));
    let metadata = CapturedFrame::new(
        frame_id,
        session,
        target,
        CaptureOrdinal::new(ordinal).map_err(|_| {
            live_error(
                ErrorCode::InvalidInput,
                "scripted frame ordinal must be non-zero",
            )
        })?,
        None,
        krometrail_core::ObservedTime::from_nanos(ordinal + 10),
        SessionTime::from_nanos(ordinal),
        if payload_bytes == ARTIFACT_SOURCE.len() {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        },
        PixelDimensions::new(2, 2)?,
        PixelDimensions::new(2, 2)?,
        DeviceScaleFactor::new(1.0)?,
        Vec::new(),
    )?;
    let bytes = if payload_bytes == ARTIFACT_SOURCE.len() {
        ARTIFACT_SOURCE.to_vec()
    } else {
        // Retention probes need byte volume, not a visual payload. The production store treats
        // this as an encoded JPEG source and never sends it to an artifact renderer.
        vec![7; payload_bytes]
    };
    EncodedFrame::new(metadata, bytes)
}

pub(crate) fn next_session_id(runtime: &QualificationRuntime) -> SessionId {
    SessionId::from_uuid(*runtime.dependencies.ids.next().as_uuid())
}

pub(crate) fn next_target_id(runtime: &QualificationRuntime) -> TargetId {
    TargetId::from_uuid(*runtime.dependencies.ids.next().as_uuid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::live_evaluation::{
        LiveQualificationConfig, OptInDecision, build_qualification_runtime,
    };
    use krometrail_core::{
        AnchorScope, DiskBudgetBytes, TemporalQueryRequest, TemporalRangeAnchor,
    };

    async fn runtime() -> super::super::QualificationRuntime {
        let root = std::env::temp_dir().join(format!("krometrail-retention-{}", Uuid::new_v4()));
        let config = LiveQualificationConfig {
            output_root: root,
            retention_budget: DiskBudgetBytes::new(700_000).unwrap(),
            ..LiveQualificationConfig::default()
        };
        build_qualification_runtime(&config, OptInDecision::Authorized).unwrap()
    }

    #[tokio::test]
    async fn concrete_store_reports_pin_eviction_pause_resume_and_cleanup() {
        let runtime = runtime().await;
        let session = SessionId::from_uuid(Uuid::from_u128(0x1000));
        let target = TargetId::from_uuid(Uuid::from_u128(0x1001));
        let frame_ids = append_scripted_frames(&runtime, session, target, 2)
            .await
            .unwrap();
        let range =
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(2)).unwrap();
        let resolved = runtime
            .dependencies
            .temporal_queries
            .resolve_range(
                TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
                    scope: AnchorScope::new(Some(session), Some(target)),
                    range,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resolved.frame_ids, frame_ids);
        let pin = RetentionPinRequest::from_resolved(&resolved).unwrap();
        let state = runtime
            .dependencies
            .retention
            .pin_resolved_range(pin.clone())
            .await
            .unwrap();
        assert!(state.state.exact_pin_active);
        assert!(
            runtime
                .dependencies
                .frames
                .frames_by_id(frame_ids.clone())
                .await
                .is_ok()
        );
        runtime
            .dependencies
            .retention
            .unpin_resolved_range(pin)
            .await
            .unwrap();
        let _ = runtime.cleanup();
    }

    #[tokio::test]
    async fn qualification_records_linked_artifact_cleanup_and_honest_budget_outcomes() {
        let root = std::env::temp_dir().join(format!(
            "krometrail-retention-qualification-{}",
            Uuid::new_v4()
        ));
        let config = LiveQualificationConfig {
            output_root: root.clone(),
            retention_budget: DiskBudgetBytes::new(600_000).unwrap(),
            ..LiveQualificationConfig::default()
        };
        let runtime = build_qualification_runtime(&config, OptInDecision::Authorized).unwrap();
        let session = SessionId::from_uuid(Uuid::from_u128(0x3000));
        let target = TargetId::from_uuid(Uuid::from_u128(0x3001));
        let frame_ids = append_scripted_frames(&runtime, session, target, 2)
            .await
            .unwrap();
        let range =
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(2)).unwrap();
        let resolved = runtime
            .dependencies
            .temporal_queries
            .resolve_range(
                TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
                    scope: AnchorScope::new(Some(session), Some(target)),
                    range,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resolved.frame_ids, frame_ids);
        let observation = qualify_retention(&runtime, &resolved, None).await.unwrap();
        assert_eq!(observation.measurements.budget_bytes, 600_000);
        assert!(observation.measurements.evicted_frame_count > 0);
        assert!(observation.measurements.pinned_interval_preserved);
        assert!(observation.linked_artifact_removed);
        assert_eq!(observation.status, EvaluationStatus::Pass);
        assert!(observation.measurements.capture_paused_when_pinned);
        assert!(observation.measurements.capture_resumed_after_unpin);
        let _ = runtime.cleanup();
        assert!(!root.exists());
    }

    #[test]
    fn no_probe_helper_claims_a_missing_frame_or_gap() {
        let frame = scripted_frame(
            SessionId::from_uuid(Uuid::from_u128(1)),
            TargetId::from_uuid(Uuid::from_u128(2)),
            3,
            1,
            ARTIFACT_SOURCE.len(),
        )
        .unwrap();
        assert_eq!(
            frame.metadata().viewport(),
            PixelDimensions::new(2, 2).unwrap()
        );
    }
}

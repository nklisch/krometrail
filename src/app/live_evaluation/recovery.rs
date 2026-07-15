//! Test-only recovery qualification over the existing segment, index, and artifact authorities.
//!
//! The scenario performs a controlled interruption by dropping the production runtime with an
//! open segment, then reopens the same temporary root through `open_storage_with_budget`. It does
//! not write SQLite rows or segment records outside the store's own ports.

use krometrail_core::{
    AnchorScope, ArtifactGenerationContext, ArtifactOutcome, ArtifactStore, CaptureGap,
    CaptureGapReason, CaptureGapStore, ErrorCode, SessionRange, SessionTime, TemporalQueryRequest,
    TemporalRangeAnchor,
};
use temporal_evaluation::{
    EvaluationStatus, FailureRecord, RecoveryQualificationMeasurements, RunFailureCode,
};
use uuid::Uuid;

use super::{
    LiveQualificationConfig, OptInDecision, build_qualification_runtime, live_error,
    retention::scripted_frame,
};
use crate::debug_bundle::default_artifact_request;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryObservation {
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
    pub measurements: RecoveryQualificationMeasurements,
    pub recovery_report: krometrail_store::RecoveryReport,
    pub reopened_frame_count: u64,
    pub reconciled_gap_ids: Vec<String>,
    pub corrupt_artifact_removed: bool,
    pub usage_before_bytes: u64,
    pub usage_after_bytes: u64,
}

/// Run one interruption/reopen cycle against a single temporary production store.
pub async fn qualify_recovery(
    config: &LiveQualificationConfig,
) -> krometrail_core::Result<RecoveryObservation> {
    let first = build_qualification_runtime(config, OptInDecision::Authorized)?;
    let session = super::retention::next_session_id(&first);
    let target = super::retention::next_target_id(&first);
    let mut frame_ids = Vec::new();
    for ordinal in 1..=2 {
        let frame = scripted_frame(
            session,
            target,
            session.as_uuid().as_u128().saturating_add(ordinal as u128),
            ordinal,
            83,
        )?;
        frame_ids.push(frame.metadata().id());
        first.dependencies.recording.append_frame(frame).await?;
    }
    let range = SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(2))?;
    let gap = CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(0xdecafbad)),
        session,
        target,
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(2))?,
        krometrail_core::ObservedTime::from_nanos(3),
        CaptureGapReason::FrameRejected,
        Some(std::num::NonZeroU64::new(1).expect("one missing frame")),
        Some("controlled interruption boundary".into()),
    )?;
    first.dependencies.recording.append_gap(gap.clone()).await?;
    let resolved = first
        .dependencies
        .temporal_queries
        .resolve_range(TemporalQueryRequest::strict(
            TemporalRangeAnchor::SessionTime {
                scope: AnchorScope::new(Some(session), Some(target)),
                range,
            },
        )?)
        .await?;
    if resolved.frame_ids != frame_ids {
        return Err(live_error(
            ErrorCode::EvidenceInvalidated,
            "recovery setup did not resolve the exact appended source frames",
        ));
    }
    let artifact_request =
        default_artifact_request(&resolved, &[], krometrail_core::OrientationPolicy::Omit)?;
    let generated = first
        .dependencies
        .artifact_generation
        .generate(artifact_request, ArtifactGenerationContext::default())
        .await?;
    let artifact_id = generated.outcomes.iter().find_map(|outcome| match outcome {
        ArtifactOutcome::Available { artifact, .. } => Some(artifact.artifact_id),
        ArtifactOutcome::Unavailable { .. } => None,
    });
    let usage_before_bytes = first
        .dependencies
        .retention
        .status()
        .await?
        .usage
        .total_bytes()?;
    // Inject the fault through the store's private artifact authority, then drop rather than
    // clean the runtime. The next composition call owns the same temporary root and invokes the
    // production recovery path before accepting IO.
    let corrupt_artifact_injected = if let Some(artifact_id) = artifact_id {
        krometrail_store::qualification_support::inject_corrupt_ready_artifact(
            &first.store,
            artifact_id,
        )
        .is_ok()
    } else {
        false
    };
    drop(first);

    let reopened = build_qualification_runtime(config, OptInDecision::Authorized)?;
    let report = reopened.recovery.clone();
    let reopened_frame_count = reopened
        .dependencies
        .frames
        .frames_by_id(frame_ids.clone())
        .await
        .map(|frames| frames.len() as u64)
        .unwrap_or(0);
    let gaps = reopened
        .dependencies
        .gaps
        .gaps(session, target, range)
        .await?;
    let after = reopened.dependencies.retention.status().await?;
    let artifact_bytes = after.usage.artifact_bytes;
    let recovered_artifact_absent = match artifact_id {
        Some(id) => reopened.store.artifact(id).await?.is_none(),
        None => false,
    };
    let recovery_files_absent = match artifact_id {
        Some(id) => krometrail_store::qualification_support::artifact_recovery_files_absent(
            &reopened.store,
            id,
        )?,
        None => false,
    };
    let corrupt_artifact_removed =
        corrupt_artifact_injected && recovered_artifact_absent && recovery_files_absent;
    let usage_accounted =
        after.usage.total_bytes()? <= after.configured_budget.get() || after.recording_blocked;
    let reconciled =
        reopened_frame_count == frame_ids.len() as u64 && gaps == vec![gap] && usage_accounted;
    let trailing_segment_repaired = report.open_segments_sealed > 0
        || report.segments_repaired > 0
        || report.bytes_truncated > 0;
    let staged_artifacts_recovered = corrupt_artifact_removed && artifact_bytes == 0;
    let complete = reopened_frame_count == frame_ids.len() as u64
        && reconciled
        && trailing_segment_repaired
        && staged_artifacts_recovered;
    let measurements = RecoveryQualificationMeasurements {
        reopened: true,
        reconciled,
        recovered_frame_count: report.frames_recovered,
        removed_frame_count: report.frames_removed,
        trailing_segment_repaired,
        staged_artifacts_recovered,
    };
    let result = RecoveryObservation {
        status: if complete {
            EvaluationStatus::Pass
        } else {
            EvaluationStatus::Inconclusive
        },
        failure: (!complete).then(|| FailureRecord {
            code: RunFailureCode::CorruptSource,
            phase: "recovery".into(),
            reason: "controlled recovery did not establish complete segment, gap, artifact, and usage reconciliation".into(),
            recovery: "repeat the interruption with a retained source interval and inspect the store recovery report".into(),
            retryable: true,
        }),
        measurements,
        recovery_report: report,
        reopened_frame_count,
        reconciled_gap_ids: gaps.iter().map(|value| value.id().to_string()).collect(),
        corrupt_artifact_removed,
        usage_before_bytes,
        usage_after_bytes: after.usage.total_bytes()?,
    };
    let _ = reopened.cleanup();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::live_evaluation::LiveQualificationConfig;
    use krometrail_core::DiskBudgetBytes;

    #[tokio::test]
    async fn recovery_reopens_open_segments_and_reconciles_gaps_without_chrome() {
        let root = std::env::temp_dir().join(format!("krometrail-recovery-{}", Uuid::new_v4()));
        let config = LiveQualificationConfig {
            output_root: root.clone(),
            retention_budget: DiskBudgetBytes::new(2_000_000).unwrap(),
            ..LiveQualificationConfig::default()
        };
        let result = qualify_recovery(&config).await.unwrap();
        assert!(result.measurements.reopened);
        assert_eq!(result.reopened_frame_count, 2);
        assert_eq!(result.reconciled_gap_ids.len(), 1);
        assert!(result.measurements.trailing_segment_repaired);
        assert!(result.corrupt_artifact_removed);
        assert!(!root.exists());
    }
}

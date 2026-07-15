//! Test-only query and artifact latency qualification over existing production ports.
//!
//! The two decisive limits are copied from `docs/EVALUATION.md` exactly: cached temporal bundle
//! retrieval is below one second and uncached storyboard/difference-map generation is below five
//! seconds. They are consulted only after the authority-returned interval proves a two-second
//! 1920x1080 profile; an 800x450 capture-fidelity interval is explicitly ineligible.

use std::time::Instant;

use krometrail_core::{
    AnchorScope, ArtifactCacheDisposition, ArtifactCacheKey, ArtifactCacheMetadata,
    ArtifactGenerationContext, ArtifactOutcome, ArtifactStore, BundleArtifactEvidence,
    OrientationPolicy, ResolvedRange, SessionRange, TemporalDebugBundle,
    TemporalDebugBundleContext, TemporalDebugBundleRequest, TemporalQueryRequest,
    TemporalRangeAnchor,
};
use temporal_evaluation::{
    CacheDisposition, EvaluationStatus, FailureRecord, LatencyQualificationMeasurements,
    RunFailureCode, SourceInterval, Viewport,
};

use super::{QualificationRuntime, live_error};
use crate::debug_bundle::default_artifact_request;

pub const PERFORMANCE_PROFILE_ID: &str = "evaluation-2s-1080p";
pub const CACHED_TEMPORAL_BUNDLE_THRESHOLD_MS: u64 = 1_000;
pub const UNCACHED_ARTIFACT_THRESHOLD_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerformanceProfile {
    pub viewport: Viewport,
    pub range_duration_ms: u64,
}

impl PerformanceProfile {
    pub const TWO_SECOND_1080P: Self = Self {
        viewport: Viewport {
            width: 1_920,
            height: 1_080,
        },
        range_duration_ms: 2_000,
    };

    pub fn accepts(self, range: SessionRange, frame_dimensions: &[(u32, u32)]) -> bool {
        range
            .end()
            .as_nanos()
            .saturating_sub(range.start().as_nanos())
            == self.range_duration_ms.saturating_mul(1_000_000)
            && !frame_dimensions.is_empty()
            && frame_dimensions.iter().all(|&(width, height)| {
                (width, height) == (self.viewport.width, self.viewport.height)
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyOperation {
    TemporalBundle,
    Storyboard,
    DifferenceMap,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactIdentityObservation {
    pub operation: LatencyOperation,
    pub cache: ArtifactCacheDisposition,
    pub artifact_id: krometrail_core::ArtifactId,
    pub cache_key: ArtifactCacheKey,
    pub cache_metadata: ArtifactCacheMetadata,
    pub manifest: krometrail_core::ArtifactManifest,
    pub source_frame_ids: Vec<krometrail_core::FrameId>,
    pub output_dimensions: temporal_vision::PixelDimensions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LatencySample {
    pub operation: LatencyOperation,
    pub cache: CacheDisposition,
    pub elapsed_ms: u64,
    pub identities: Vec<ArtifactIdentityObservation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LatencyObservation {
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
    pub profile: PerformanceProfile,
    pub source_interval_id: String,
    pub source_range: SessionRange,
    pub frame_dimensions: Vec<(u32, u32)>,
    pub samples: Vec<LatencySample>,
    pub measurements: LatencyQualificationMeasurements,
}

/// Measure one exact resolved range through the production artifact and bundle services.
///
/// The `SourceInterval` is the evaluation-facing projection of the same resolver result. The
/// `ResolvedRange` is retained alongside it so the existing core ports receive the original
/// authority value rather than a reconstructed range.
pub async fn measure_latency(
    runtime: &QualificationRuntime,
    interval: &SourceInterval,
    resolved_range: &ResolvedRange,
) -> krometrail_core::Result<LatencyObservation> {
    interval.validate().map_err(|_| {
        live_error(
            krometrail_core::ErrorCode::InvalidInput,
            "latency qualification requires a valid source interval",
        )
    })?;
    validate_same_interval(interval, resolved_range)?;
    let metadata = runtime
        .dependencies
        .frames
        .frame_metadata_by_id(resolved_range.frame_ids.clone())
        .await?;
    let frame_dimensions = metadata
        .iter()
        .map(|frame| (frame.image().width(), frame.image().height()))
        .collect::<Vec<_>>();
    let profile = PerformanceProfile::TWO_SECOND_1080P;
    let profile_valid = profile.accepts(resolved_range.resolved_range, &frame_dimensions);
    if !profile_valid {
        return Ok(ineligible_latency(
            interval,
            resolved_range,
            frame_dimensions,
        ));
    }

    // Use the existing policy helper. Excluding the optional orientation makes this a direct
    // storyboard/difference-map pair while retaining the product's generator and cache identity.
    let direct_request = default_artifact_request(resolved_range, &[], OrientationPolicy::Omit)?;
    let uncached_started = Instant::now();
    let direct_uncached = runtime
        .dependencies
        .artifact_generation
        .generate(direct_request.clone(), ArtifactGenerationContext::default())
        .await?;
    let uncached_elapsed_ms = uncached_started.elapsed().as_millis() as u64;
    let uncached_identities = artifact_identities(
        runtime,
        LatencyOperation::Storyboard,
        &direct_uncached,
        resolved_range,
    )
    .await?;
    let warm_started = Instant::now();
    let direct_warm = runtime
        .dependencies
        .artifact_generation
        .generate(direct_request, ArtifactGenerationContext::default())
        .await?;
    let warm_elapsed_ms = warm_started.elapsed().as_millis() as u64;
    let warm_identities = artifact_identities(
        runtime,
        LatencyOperation::Storyboard,
        &direct_warm,
        resolved_range,
    )
    .await?;

    let bundle_query = TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
        scope: AnchorScope::new(
            Some(resolved_range.session_id),
            Some(resolved_range.target_id),
        ),
        range: resolved_range.resolved_range,
    })?;
    let bundle_request = TemporalDebugBundleRequest::default_policy(bundle_query)?;
    let first_bundle_started = Instant::now();
    let first_bundle = runtime
        .dependencies
        .temporal_debug_bundles
        .bundle(
            bundle_request.clone(),
            TemporalDebugBundleContext::default(),
        )
        .await?;
    let first_bundle_elapsed_ms = first_bundle_started.elapsed().as_millis() as u64;
    validate_bundle_range(&first_bundle, resolved_range)?;
    let first_bundle_identities = bundle_identities(runtime, &first_bundle, resolved_range).await?;
    let second_bundle_started = Instant::now();
    let second_bundle = runtime
        .dependencies
        .temporal_debug_bundles
        .bundle(bundle_request, TemporalDebugBundleContext::default())
        .await?;
    let second_bundle_elapsed_ms = second_bundle_started.elapsed().as_millis() as u64;
    validate_bundle_range(&second_bundle, resolved_range)?;
    let second_bundle_identities =
        bundle_identities(runtime, &second_bundle, resolved_range).await?;

    let uncached_cache = aggregate_cache_disposition(&uncached_identities);
    let warm_artifact_cache = aggregate_cache_disposition(&warm_identities);
    let direct_complete = direct_uncached.outcomes.len() == 2
        && direct_warm.outcomes.len() == 2
        && uncached_identities.len() == 2
        && warm_identities.len() == 2
        && uncached_cache == CacheDisposition::Cold
        && warm_artifact_cache == CacheDisposition::Warm
        && uncached_identities
            .iter()
            .all(|item| item.cache == ArtifactCacheDisposition::Generated)
        && warm_identities
            .iter()
            .all(|item| item.cache == ArtifactCacheDisposition::Hit)
        && same_identity_set(&uncached_identities, &warm_identities);
    // The production default bundle also persists its orientation composite. The direct artifact
    // measurement remains the exact two-generator storyboard/difference-map path; bundle timing
    // accounts for every authority-returned output instead of dropping that third identity.
    // The direct request above warms the shared storyboard and difference-map entries before the
    // first bundle call. Its exact artifact dispositions are therefore mixed: the shared
    // difference map is a hit while bundle-specific outputs are generated. The direct pair still
    // proves the decisive uncached generation and all-hit repeat independently.
    let first_bundle_cache = aggregate_cache_disposition(&first_bundle_identities);
    let second_bundle_cache = aggregate_cache_disposition(&second_bundle_identities);
    let bundle_complete = first_bundle_identities.len() == 3
        && second_bundle_identities.len() == 3
        && first_bundle_identities.iter().all(|item| {
            matches!(
                item.cache,
                ArtifactCacheDisposition::Generated | ArtifactCacheDisposition::Hit
            )
        })
        && second_bundle_identities
            .iter()
            .all(|item| item.cache == ArtifactCacheDisposition::Hit)
        && second_bundle_cache == CacheDisposition::Warm
        && same_identity_set(&first_bundle_identities, &second_bundle_identities);
    let cached_bundle_ok = second_bundle_elapsed_ms < CACHED_TEMPORAL_BUNDLE_THRESHOLD_MS;
    let uncached_artifact_ok = uncached_elapsed_ms < UNCACHED_ARTIFACT_THRESHOLD_MS;
    let complete = direct_complete && bundle_complete && cached_bundle_ok && uncached_artifact_ok;
    let status = if complete {
        EvaluationStatus::Pass
    } else if direct_complete && bundle_complete {
        EvaluationStatus::Fail
    } else {
        EvaluationStatus::Inconclusive
    };
    let warm_bundle_cache = if warm_artifact_cache == CacheDisposition::Warm
        && second_bundle_cache == CacheDisposition::Warm
    {
        CacheDisposition::Warm
    } else {
        CacheDisposition::Unavailable
    };
    let measurements = LatencyQualificationMeasurements {
        source_interval_id: interval.interval_id.clone(),
        viewport: profile.viewport,
        frame_width: profile.viewport.width,
        frame_height: profile.viewport.height,
        warm_cache: warm_bundle_cache,
        temporal_query_elapsed_ms: vec![first_bundle_elapsed_ms, second_bundle_elapsed_ms],
        artifact_elapsed_ms: vec![uncached_elapsed_ms, warm_elapsed_ms],
        sample_count: 4,
        threshold_profile_ids: vec![
            "evaluation-cached-temporal-bundle-below-1s".into(),
            "evaluation-uncached-storyboard-difference-map-below-5s".into(),
        ],
    };
    Ok(LatencyObservation {
        status,
        failure: (status != EvaluationStatus::Pass).then(|| FailureRecord {
            code: if direct_complete && bundle_complete {
                RunFailureCode::Threshold
            } else {
                RunFailureCode::InsufficientEvidence
            },
            phase: "latency".into(),
            reason: if direct_complete && bundle_complete {
                "authority-backed two-second 1080p latency exceeded an EVALUATION threshold".into()
            } else {
                "latency evidence did not preserve complete authority-returned cache and output identities".into()
            },
            recovery: "retain the same two-second 1920x1080 source interval and rerun the existing bundle and artifact ports".into(),
            retryable: true,
        }),
        profile,
        source_interval_id: interval.interval_id.clone(),
        source_range: resolved_range.resolved_range,
        frame_dimensions,
        samples: vec![
            LatencySample {
                operation: LatencyOperation::Storyboard,
                cache: uncached_cache,
                elapsed_ms: uncached_elapsed_ms,
                identities: identities_for_operation(
                    &uncached_identities,
                    LatencyOperation::Storyboard,
                ),
            },
            LatencySample {
                operation: LatencyOperation::DifferenceMap,
                cache: uncached_cache,
                elapsed_ms: uncached_elapsed_ms,
                identities: identities_for_operation(
                    &uncached_identities,
                    LatencyOperation::DifferenceMap,
                ),
            },
            LatencySample {
                operation: LatencyOperation::Storyboard,
                cache: warm_artifact_cache,
                elapsed_ms: warm_elapsed_ms,
                identities: identities_for_operation(&warm_identities, LatencyOperation::Storyboard),
            },
            LatencySample {
                operation: LatencyOperation::DifferenceMap,
                cache: warm_artifact_cache,
                elapsed_ms: warm_elapsed_ms,
                identities: identities_for_operation(
                    &warm_identities,
                    LatencyOperation::DifferenceMap,
                ),
            },
            LatencySample {
                operation: LatencyOperation::TemporalBundle,
                cache: first_bundle_cache,
                elapsed_ms: first_bundle_elapsed_ms,
                identities: first_bundle_identities,
            },
            LatencySample {
                operation: LatencyOperation::TemporalBundle,
                cache: second_bundle_cache,
                elapsed_ms: second_bundle_elapsed_ms,
                identities: second_bundle_identities,
            },
        ],
        measurements,
    })
}

fn ineligible_latency(
    interval: &SourceInterval,
    resolved_range: &ResolvedRange,
    frame_dimensions: Vec<(u32, u32)>,
) -> LatencyObservation {
    let measurements = LatencyQualificationMeasurements {
        source_interval_id: interval.interval_id.clone(),
        viewport: PerformanceProfile::TWO_SECOND_1080P.viewport,
        frame_width: frame_dimensions.first().map_or(0, |value| value.0),
        frame_height: frame_dimensions.first().map_or(0, |value| value.1),
        warm_cache: CacheDisposition::Unavailable,
        temporal_query_elapsed_ms: Vec::new(),
        artifact_elapsed_ms: Vec::new(),
        sample_count: 0,
        threshold_profile_ids: Vec::new(),
    };
    LatencyObservation {
        status: EvaluationStatus::Blocked,
        failure: Some(FailureRecord {
            code: RunFailureCode::Unavailable,
            phase: "latency_profile".into(),
            reason: "the authority-returned interval is not a two-second 1920x1080 performance profile".into(),
            recovery: "collect a distinct two-second 1920x1080 interval; do not apply latency limits to capture-fidelity dimensions".into(),
            retryable: true,
        }),
        profile: PerformanceProfile::TWO_SECOND_1080P,
        source_interval_id: interval.interval_id.clone(),
        source_range: resolved_range.resolved_range,
        frame_dimensions,
        samples: Vec::new(),
        measurements,
    }
}

fn validate_same_interval(
    interval: &SourceInterval,
    resolved: &ResolvedRange,
) -> krometrail_core::Result<()> {
    let session = interval
        .session_scope
        .session_id
        .parse::<uuid::Uuid>()
        .map_err(|_| {
            live_error(
                krometrail_core::ErrorCode::EvidenceInvalidated,
                "source interval session identity is invalid",
            )
        })?;
    let target = interval
        .session_scope
        .target_id
        .parse::<uuid::Uuid>()
        .map_err(|_| {
            live_error(
                krometrail_core::ErrorCode::EvidenceInvalidated,
                "source interval target identity is invalid",
            )
        })?;
    if *resolved.session_id.as_uuid() != session
        || *resolved.target_id.as_uuid() != target
        || resolved.requested_range.start().as_nanos() != interval.requested_range.start_ns
        || resolved.requested_range.end().as_nanos() != interval.requested_range.end_ns
        || resolved.resolved_range.start().as_nanos() != interval.resolved_range.start_ns
        || resolved.resolved_range.end().as_nanos() != interval.resolved_range.end_ns
        || resolved
            .frame_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            != interval
                .frames
                .iter()
                .map(|frame| frame.id.clone())
                .collect::<Vec<_>>()
    {
        return Err(live_error(
            krometrail_core::ErrorCode::EvidenceInvalidated,
            "latency inputs do not reference one authority-returned source interval",
        ));
    }
    Ok(())
}

fn validate_bundle_range(
    bundle: &TemporalDebugBundle,
    resolved: &ResolvedRange,
) -> krometrail_core::Result<()> {
    if bundle.range != *resolved {
        return Err(live_error(
            krometrail_core::ErrorCode::EvidenceInvalidated,
            "temporal bundle returned a different resolved interval",
        ));
    }
    Ok(())
}

async fn bundle_identities(
    runtime: &QualificationRuntime,
    bundle: &TemporalDebugBundle,
    resolved: &ResolvedRange,
) -> krometrail_core::Result<Vec<ArtifactIdentityObservation>> {
    let BundleArtifactEvidence::Available(result) = &bundle.artifacts else {
        return Err(live_error(
            krometrail_core::ErrorCode::EvidenceInvalidated,
            "temporal bundle did not return artifact evidence",
        ));
    };
    artifact_identities(runtime, LatencyOperation::TemporalBundle, result, resolved).await
}

async fn artifact_identities(
    runtime: &QualificationRuntime,
    _operation: LatencyOperation,
    result: &krometrail_core::ArtifactGenerationResult,
    resolved: &ResolvedRange,
) -> krometrail_core::Result<Vec<ArtifactIdentityObservation>> {
    if result.range != *resolved {
        return Err(live_error(
            krometrail_core::ErrorCode::EvidenceInvalidated,
            "artifact generation returned a different resolved interval",
        ));
    }
    let mut identities = Vec::new();
    for outcome in &result.outcomes {
        let ArtifactOutcome::Available { artifact, .. } = outcome else {
            return Err(live_error(
                krometrail_core::ErrorCode::EvidenceInvalidated,
                "artifact generation returned an unavailable output",
            ));
        };
        let stored = runtime
            .store
            .artifact(artifact.artifact_id)
            .await?
            .ok_or_else(|| {
                live_error(
                    krometrail_core::ErrorCode::NotFound,
                    "authority artifact disappeared before latency accounting",
                )
            })?;
        if artifact.artifact_id != *stored.manifest.artifact_id()
            || stored.manifest != artifact.manifest
            || stored.manifest.source_frame_ids() != resolved.frame_ids
            || stored.encoded_bytes.len() as u64 != artifact.encoded_byte_len
        {
            return Err(live_error(
                krometrail_core::ErrorCode::EvidenceInvalidated,
                "artifact output, manifest, or source identity changed during latency accounting",
            ));
        }
        let kind = stored.manifest.artifact_kind();
        let operation_for_kind = match kind {
            temporal_vision::ArtifactKind::Storyboard => LatencyOperation::Storyboard,
            temporal_vision::ArtifactKind::DifferenceMap => LatencyOperation::DifferenceMap,
            temporal_vision::ArtifactKind::BeforeDuringAfter
                if _operation == LatencyOperation::TemporalBundle =>
            {
                LatencyOperation::TemporalBundle
            }
            _ => {
                return Err(live_error(
                    krometrail_core::ErrorCode::Unsupported,
                    "latency profile received an artifact outside the requested production policy",
                ));
            }
        };
        identities.push(ArtifactIdentityObservation {
            operation: operation_for_kind,
            cache: artifact.cache,
            artifact_id: *stored.manifest.artifact_id(),
            cache_key: stored.cache.cache_key,
            cache_metadata: stored.cache,
            manifest: stored.manifest.clone(),
            source_frame_ids: stored.manifest.source_frame_ids().to_vec(),
            output_dimensions: stored.manifest.output_dimensions(),
        });
    }
    identities.sort_by_key(|identity| identity.artifact_id);
    Ok(identities)
}

fn identities_for_operation(
    identities: &[ArtifactIdentityObservation],
    operation: LatencyOperation,
) -> Vec<ArtifactIdentityObservation> {
    identities
        .iter()
        .filter(|identity| identity.operation == operation)
        .cloned()
        .collect()
}

/// Classify an aggregate from the exact cache dispositions returned for its artifacts.
///
/// A cache-miss regeneration is generation for this aggregate, while the identity still exposes
/// whether it was a first generation or a regeneration after invalidation. An empty set is not a
/// cache state at all and remains unavailable.
fn aggregate_cache_disposition(identities: &[ArtifactIdentityObservation]) -> CacheDisposition {
    classify_cache_dispositions(identities.iter().map(|identity| identity.cache))
}

fn classify_cache_dispositions(
    dispositions: impl IntoIterator<Item = ArtifactCacheDisposition>,
) -> CacheDisposition {
    let mut has_hit = false;
    let mut has_generation = false;
    for disposition in dispositions {
        match disposition {
            ArtifactCacheDisposition::Hit => has_hit = true,
            ArtifactCacheDisposition::Generated
            | ArtifactCacheDisposition::RegeneratedAfterInvalidation => has_generation = true,
        }
    }
    match (has_generation, has_hit) {
        (false, false) => CacheDisposition::Unavailable,
        (true, false) => CacheDisposition::Cold,
        (false, true) => CacheDisposition::Warm,
        (true, true) => CacheDisposition::Mixed,
    }
}

fn same_identity_set(
    left: &[ArtifactIdentityObservation],
    right: &[ArtifactIdentityObservation],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.operation == right.operation
                && left.artifact_id == right.artifact_id
                && left.cache_key == right.cache_key
                && left.cache_metadata == right.cache_metadata
                && left.manifest == right.manifest
                && left.source_frame_ids == right.source_frame_ids
                && left.output_dimensions == right.output_dimensions
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::live_evaluation::retention::append_scripted_frames;
    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameId,
        ImageFormat, PixelDimensions, SessionId, SessionTime, TargetId,
    };
    use std::io::Cursor;
    use uuid::Uuid;

    #[test]
    fn latency_thresholds_are_scoped_to_the_declared_profile() {
        let range =
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_999_999_999)).unwrap();
        assert!(!PerformanceProfile::TWO_SECOND_1080P.accepts(range, &[(1_920, 1_080)]));
        assert!(!PerformanceProfile::TWO_SECOND_1080P.accepts(
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2_000_000_000)).unwrap(),
            &[(800, 450)],
        ));
        assert!(PerformanceProfile::TWO_SECOND_1080P.accepts(
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2_000_000_000)).unwrap(),
            &[(1_920, 1_080)],
        ));
        assert!(!PerformanceProfile::TWO_SECOND_1080P.accepts(
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(3_000_000_000)).unwrap(),
            &[(1_920, 1_080)],
        ));
        assert_eq!(CACHED_TEMPORAL_BUNDLE_THRESHOLD_MS, 1_000);
        assert_eq!(UNCACHED_ARTIFACT_THRESHOLD_MS, 5_000);
    }

    #[test]
    fn cache_classifier_reports_all_generated_as_cold() {
        assert_eq!(
            classify_cache_dispositions([
                ArtifactCacheDisposition::Generated,
                ArtifactCacheDisposition::Generated,
            ]),
            CacheDisposition::Cold
        );
        assert_eq!(
            classify_cache_dispositions([
                ArtifactCacheDisposition::Generated,
                ArtifactCacheDisposition::RegeneratedAfterInvalidation,
            ]),
            CacheDisposition::Cold
        );
    }

    #[test]
    fn cache_classifier_reports_all_hits_as_warm() {
        assert_eq!(
            classify_cache_dispositions([
                ArtifactCacheDisposition::Hit,
                ArtifactCacheDisposition::Hit,
            ]),
            CacheDisposition::Warm
        );
    }

    #[test]
    fn cache_classifier_reports_generated_and_hit_as_mixed() {
        assert_eq!(
            classify_cache_dispositions([
                ArtifactCacheDisposition::Generated,
                ArtifactCacheDisposition::Hit,
            ]),
            CacheDisposition::Mixed
        );
        assert_eq!(
            classify_cache_dispositions([
                ArtifactCacheDisposition::RegeneratedAfterInvalidation,
                ArtifactCacheDisposition::Hit,
            ]),
            CacheDisposition::Mixed
        );
    }

    #[test]
    fn cache_classifier_reports_empty_artifact_identity_as_unavailable() {
        assert_eq!(
            classify_cache_dispositions(std::iter::empty::<ArtifactCacheDisposition>()),
            CacheDisposition::Unavailable
        );
    }

    #[tokio::test]
    async fn concrete_store_accounts_uncached_and_warm_authority_cache_dispositions() {
        let root =
            std::env::temp_dir().join(format!("krometrail-latency-1080p-{}", Uuid::new_v4()));
        let config = super::super::LiveQualificationConfig {
            output_root: root.clone(),
            retention_budget: DiskBudgetBytes::new(100_000_000).unwrap(),
            ..super::super::LiveQualificationConfig::default()
        };
        let runtime = super::super::build_qualification_runtime(
            &config,
            super::super::OptInDecision::Authorized,
        )
        .unwrap();
        let session = SessionId::from_uuid(Uuid::from_u128(0x5000));
        let target = TargetId::from_uuid(Uuid::from_u128(0x5001));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1_920,
            1_080,
            image::Rgba([0, 0, 0, 255]),
        ))
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        let frame_ids = [
            FrameId::from_uuid(Uuid::from_u128(0x5010)),
            FrameId::from_uuid(Uuid::from_u128(0x5011)),
        ];
        for (ordinal, frame_id) in frame_ids.into_iter().enumerate() {
            let session_time =
                SessionTime::from_nanos(u64::try_from(ordinal).unwrap() * 2_000_000_000);
            let metadata = CapturedFrame::new(
                frame_id,
                session,
                target,
                CaptureOrdinal::new(u64::try_from(ordinal).unwrap() + 1).unwrap(),
                None,
                krometrail_core::ObservedTime::from_nanos(session_time.as_nanos() + 1),
                session_time,
                ImageFormat::Png,
                PixelDimensions::new(1_920, 1_080).unwrap(),
                PixelDimensions::new(1_920, 1_080).unwrap(),
                DeviceScaleFactor::new(1.0).unwrap(),
                Vec::new(),
            )
            .unwrap();
            runtime
                .dependencies
                .recording
                .append_frame(EncodedFrame::new(metadata, png.clone()).unwrap())
                .await
                .unwrap();
        }
        runtime.dependencies.recording.flush(session).await.unwrap();
        let resolved = runtime
            .dependencies
            .temporal_queries
            .resolve_range(
                TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
                    scope: AnchorScope::new(Some(session), Some(target)),
                    range: SessionRange::new(
                        SessionTime::ZERO,
                        SessionTime::from_nanos(2_000_000_000),
                    )
                    .unwrap(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
        let interval = SourceInterval::new(
            "interval-scripted-1080p",
            temporal_evaluation::ScopeIdentity::new(session.to_string(), target.to_string())
                .unwrap(),
            temporal_evaluation::TimeRangeNs::new(0, 2_000_000_000).unwrap(),
            temporal_evaluation::TimeRangeNs::new(0, 2_000_000_000).unwrap(),
            1_000_000_000,
            frame_ids
                .into_iter()
                .enumerate()
                .map(|(ordinal, id)| {
                    temporal_evaluation::SourceFrameEvidence::new(
                        id.to_string(),
                        u64::try_from(ordinal).unwrap() + 1,
                        None,
                        u64::try_from(ordinal).unwrap() * 2_000_000_000 + 1,
                        u64::try_from(ordinal).unwrap() * 2_000_000_000,
                        temporal_evaluation::sha256_prefixed(&png),
                        temporal_evaluation::EvidenceAvailability::Retained,
                    )
                    .unwrap()
                })
                .collect(),
            vec![],
            temporal_evaluation::RetentionState::Retained,
        )
        .unwrap();
        let result = measure_latency(&runtime, &interval, &resolved)
            .await
            .unwrap();
        assert_eq!(result.measurements.frame_width, 1_920);
        assert_eq!(result.measurements.frame_height, 1_080);
        assert_eq!(result.measurements.source_interval_id, interval.interval_id);
        assert_eq!(result.samples.len(), 6);
        assert!(
            result.samples[..4]
                .iter()
                .enumerate()
                .all(|(index, sample)| {
                    sample.cache
                        == if index < 2 {
                            CacheDisposition::Cold
                        } else {
                            CacheDisposition::Warm
                        }
                })
        );
        assert_eq!(result.samples[4].cache, CacheDisposition::Mixed);
        assert_eq!(result.samples[5].cache, CacheDisposition::Warm);
        assert!(
            result
                .samples
                .iter()
                .all(|sample| !sample.identities.is_empty())
        );
        for identity in result.samples.iter().flat_map(|sample| &sample.identities) {
            assert_eq!(identity.artifact_id, *identity.manifest.artifact_id());
            assert_eq!(identity.cache_key, identity.cache_metadata.cache_key);
            assert_eq!(
                identity.source_frame_ids,
                identity.manifest.source_frame_ids().to_vec()
            );
            assert_eq!(
                identity.output_dimensions,
                identity.manifest.output_dimensions()
            );
            assert_eq!(identity.source_frame_ids, frame_ids);
        }
        assert!(
            result.samples[0]
                .identities
                .iter()
                .all(|identity| identity.cache == ArtifactCacheDisposition::Generated)
        );
        assert!(
            result.samples[2]
                .identities
                .iter()
                .all(|identity| identity.cache == ArtifactCacheDisposition::Hit)
        );
        assert_eq!(result.samples[4].identities.len(), 3);
        assert_eq!(
            result.samples[4]
                .identities
                .iter()
                .find(|identity| identity.operation == LatencyOperation::DifferenceMap)
                .map(|identity| identity.cache),
            Some(ArtifactCacheDisposition::Hit)
        );
        assert_eq!(
            result.samples[4]
                .identities
                .iter()
                .find(|identity| identity.operation == LatencyOperation::Storyboard)
                .map(|identity| identity.cache),
            Some(ArtifactCacheDisposition::Generated)
        );
        assert_eq!(
            result.samples[4]
                .identities
                .iter()
                .find(|identity| identity.operation == LatencyOperation::TemporalBundle)
                .map(|identity| identity.cache),
            Some(ArtifactCacheDisposition::Generated)
        );
        assert!(
            result.samples[5]
                .identities
                .iter()
                .all(|identity| identity.cache == ArtifactCacheDisposition::Hit)
        );
        assert_ne!(result.status, EvaluationStatus::Inconclusive);
        assert_ne!(result.status, EvaluationStatus::Blocked);
        let _ = runtime.cleanup();
    }

    #[tokio::test]
    async fn scripted_store_rejects_capture_fidelity_dimensions_before_thresholds() {
        let root = std::env::temp_dir().join(format!("krometrail-latency-{}", Uuid::new_v4()));
        let config = super::super::LiveQualificationConfig {
            output_root: root.clone(),
            retention_budget: DiskBudgetBytes::new(2_000_000).unwrap(),
            ..super::super::LiveQualificationConfig::default()
        };
        let runtime = super::super::build_qualification_runtime(
            &config,
            super::super::OptInDecision::Authorized,
        )
        .unwrap();
        let session = SessionId::from_uuid(Uuid::from_u128(0x4000));
        let target = TargetId::from_uuid(Uuid::from_u128(0x4001));
        let ids = append_scripted_frames(&runtime, session, target, 2)
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
        assert_eq!(resolved.frame_ids, ids);
        let interval = SourceInterval::new(
            "interval-scripted",
            temporal_evaluation::ScopeIdentity::new(session.to_string(), target.to_string())
                .unwrap(),
            temporal_evaluation::TimeRangeNs::new(1, 2).unwrap(),
            temporal_evaluation::TimeRangeNs::new(1, 2).unwrap(),
            1,
            vec![
                temporal_evaluation::SourceFrameEvidence::new(
                    ids[0].to_string(),
                    1,
                    None,
                    11,
                    1,
                    temporal_evaluation::sha256_prefixed(&[]),
                    temporal_evaluation::EvidenceAvailability::Retained,
                )
                .unwrap(),
                temporal_evaluation::SourceFrameEvidence::new(
                    ids[1].to_string(),
                    2,
                    None,
                    12,
                    2,
                    temporal_evaluation::sha256_prefixed(&[]),
                    temporal_evaluation::EvidenceAvailability::Retained,
                )
                .unwrap(),
            ],
            vec![],
            temporal_evaluation::RetentionState::Retained,
        )
        .unwrap();
        let result = measure_latency(&runtime, &interval, &resolved)
            .await
            .unwrap();
        assert_eq!(result.status, EvaluationStatus::Blocked);
        assert!(result.samples.is_empty());
        let _ = runtime.cleanup();
    }
}

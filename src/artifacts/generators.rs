use std::{
    num::{NonZeroU8, NonZeroU32, NonZeroUsize},
    sync::Arc,
};

use krometrail_core::{
    AnalysisScale, ArtifactGeneratorRequest, ArtifactId, ArtifactManifest, ErrorCode, FrameId,
    FrameSelector, KrometrailError, MAX_ANALYSIS_DOWNSCALE_FACTOR, MIN_ANALYSIS_DOWNSCALE_FACTOR,
    NonEmptyText, NormalizationRequest, OutputLimitsRequest, Result,
};
use temporal_vision::{
    ArtifactKind, ArtifactLabels, DifferenceMapLimits, DifferenceMapParameters, FilmstripTileLimit,
    IntegerScale, MeasurementParameters, MotionDecay, MotionHistoryParameters,
    NormalizationParameters, NormalizedSequence, ProcessingLimits, RegionFilmstripLabels,
    RegionFilmstripParameters, RegionFilmstripRenderLimits, RenderLimits, SharedAdjacentAnalysis,
    StoryboardParameters, StoryboardTileLimit, TimePalette, Timestamp,
};

use super::{
    epoch::EpochInput,
    scheduler::{ArtifactWorkLimits, limit_error},
};

#[derive(Clone)]
pub(crate) struct PreparedGenerator {
    pub request: ArtifactGeneratorRequest,
    pub canonical_parameters: Vec<(ArtifactKind, Arc<[u8]>)>,
}

pub(crate) struct GeneratedOutput {
    pub kind: ArtifactKind,
    pub manifest: ArtifactManifest,
    pub bytes: Arc<[u8]>,
}

pub(crate) fn prepare_generator(
    request: &ArtifactGeneratorRequest,
    anchor_was_defaulted: bool,
    epoch: &super::epoch::EpochPlan,
    limits: ArtifactWorkLimits,
) -> Result<PreparedGenerator> {
    let mut request = request.clone();
    materialize_epoch_anchor(&mut request, anchor_was_defaulted, epoch)?;
    materialize_effective_scales(&mut request, epoch, limits)?;
    validate_output_limits(&request, limits)?;
    let canonical_parameters = request
        .output_kinds()
        .iter()
        .map(|kind| {
            serde_json::to_vec(&(&request, kind, &epoch.markers, &epoch.gaps))
                .map(|bytes| (*kind, Arc::<[u8]>::from(bytes)))
                .map_err(|_| generation_error("could not encode canonical artifact parameters"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PreparedGenerator {
        request,
        canonical_parameters,
    })
}

fn materialize_epoch_anchor(
    request: &mut ArtifactGeneratorRequest,
    anchor_was_defaulted: bool,
    epoch: &super::epoch::EpochPlan,
) -> Result<()> {
    let first = epoch
        .frames
        .first()
        .ok_or_else(|| generation_error("artifact visual epoch has no source frames"))?
        .metadata()
        .session_time();
    let last = epoch
        .frames
        .last()
        .ok_or_else(|| generation_error("artifact visual epoch has no source frames"))?
        .metadata()
        .session_time();
    match request {
        ArtifactGeneratorRequest::Storyboard(request) => {
            if anchor_was_defaulted {
                // Omitted storyboard anchors use the first retained source frame, the same
                // source-safe default used by region filmstrips.
                request.anchor = first;
            } else {
                request.anchor = request.anchor.clamp(first, last);
            }
        }
        ArtifactGeneratorRequest::RegionFilmstrip(request) if anchor_was_defaulted => {
            request.anchor = request
                .locator
                .and_then(|locator| {
                    epoch
                        .frames
                        .iter()
                        .find(|frame| frame.metadata().id() == locator)
                })
                .map(|frame| frame.metadata().session_time())
                .unwrap_or(first);
        }
        ArtifactGeneratorRequest::RegionFilmstrip(_)
        | ArtifactGeneratorRequest::DifferenceMap(_)
        | ArtifactGeneratorRequest::MotionHistory(_) => {}
    }
    Ok(())
}

fn normalization_request(request: &ArtifactGeneratorRequest) -> Option<NormalizationRequest> {
    match request {
        ArtifactGeneratorRequest::Storyboard(request) => Some(request.normalization),
        ArtifactGeneratorRequest::DifferenceMap(request) => Some(request.normalization),
        ArtifactGeneratorRequest::MotionHistory(request) => Some(request.normalization),
        ArtifactGeneratorRequest::RegionFilmstrip(_) => None,
    }
}

fn output_limits(request: &ArtifactGeneratorRequest) -> OutputLimitsRequest {
    match request {
        ArtifactGeneratorRequest::Storyboard(request) => request.output,
        ArtifactGeneratorRequest::DifferenceMap(request) => request.output,
        ArtifactGeneratorRequest::RegionFilmstrip(request) => request.output,
        ArtifactGeneratorRequest::MotionHistory(request) => request.output,
    }
}

pub(crate) fn normalization_parameters(
    prepared: &PreparedGenerator,
    limits: ArtifactWorkLimits,
) -> Result<Option<NormalizationParameters>> {
    normalization_request(&prepared.request)
        .map(|request| to_normalization(request, limits))
        .transpose()
}

pub(crate) fn normalization_identity(prepared: &PreparedGenerator) -> Result<Option<Arc<[u8]>>> {
    normalization_request(&prepared.request)
        .map(|request| serde_json::to_vec(&request))
        .transpose()
        .map(|bytes| bytes.map(Arc::from))
        .map_err(|_| generation_error("could not encode normalization identity"))
}

pub(crate) fn analysis_noise_floor(prepared: &PreparedGenerator) -> Option<u16> {
    match &prepared.request {
        ArtifactGeneratorRequest::Storyboard(request) => Some(request.noise_floor),
        ArtifactGeneratorRequest::DifferenceMap(request) => Some(request.noise_floor),
        ArtifactGeneratorRequest::MotionHistory(request) => Some(request.noise_floor),
        ArtifactGeneratorRequest::RegionFilmstrip(_) => None,
    }
}

pub(crate) fn needs_change_masks(prepared: &PreparedGenerator) -> bool {
    matches!(
        &prepared.request,
        ArtifactGeneratorRequest::DifferenceMap(_) | ArtifactGeneratorRequest::MotionHistory(_)
    )
}

pub(crate) fn estimated_normalized_bytes(
    prepared: &PreparedGenerator,
    epoch: &super::epoch::EpochPlan,
) -> Result<usize> {
    let Some(normalization) = normalization_request(&prepared.request) else {
        return Ok(0);
    };
    let (width, height) = normalized_dimensions(
        normalization,
        epoch.descriptor.image.width(),
        epoch.descriptor.image.height(),
    )?;
    usize::try_from(u64::from(width) * u64::from(height))
        .ok()
        .and_then(|pixels| pixels.checked_mul(6))
        .and_then(|per_frame| per_frame.checked_mul(epoch.frames.len()))
        .ok_or_else(|| limit_error("normalized sequence byte estimate overflows"))
}

pub(crate) fn normalize(
    epoch: &EpochInput,
    prepared: &PreparedGenerator,
    limits: ArtifactWorkLimits,
) -> Result<Option<Arc<NormalizedSequence<FrameId>>>> {
    let Some(parameters) = normalization_parameters(prepared, limits)? else {
        return Ok(None);
    };
    #[cfg(test)]
    super::perf_counters::record_normalize(epoch.sequence.frames().len());
    temporal_vision::normalize_sequence(&epoch.sequence, parameters)
        .map(Arc::new)
        .map(Some)
        .map_err(vision_error)
}

pub(crate) fn generate(
    epoch: &EpochInput,
    prepared: &PreparedGenerator,
    artifact_ids: &[ArtifactId],
    normalized: Option<&NormalizedSequence<FrameId>>,
    shared: Option<&SharedAdjacentAnalysis<FrameId>>,
    limits: ArtifactWorkLimits,
) -> Result<Vec<GeneratedOutput>> {
    let outputs = match &prepared.request {
        ArtifactGeneratorRequest::Storyboard(request) => {
            let normalized = normalized
                .ok_or_else(|| generation_error("storyboard normalization is missing"))?;
            let mut storyboard_parameters = StoryboardParameters::new(
                Timestamp::from_nanos(request.anchor.as_nanos()),
                StoryboardTileLimit::new(request.tile_limit).map_err(vision_error)?,
                MeasurementParameters::new(request.noise_floor),
                ArtifactLabels::new(
                    request.labels.title.as_str(),
                    request.labels.source.as_str(),
                )
                .map_err(vision_error)?,
                render_limits(request.output, limits)?,
            );
            if epoch.sequence.source_frame_count() > epoch.sequence.frames().len()
                && let Some(source_indices) = epoch.sequence.source_indices()
            {
                storyboard_parameters =
                    storyboard_parameters.with_analysis_source_indices(source_indices.to_vec());
            }
            let generated = temporal_vision::generate_storyboard_with_analysis(
                artifact_ids[0],
                request.include_orientation.then(|| artifact_ids[1]),
                &epoch.sequence,
                normalized,
                storyboard_parameters,
                shared,
            )
            .map_err(vision_error)?;
            let mut outputs = vec![from_generated(generated.storyboard())];
            if let Some(orientation) = generated.orientation() {
                outputs.push(from_generated(orientation));
            }
            outputs
        }
        ArtifactGeneratorRequest::DifferenceMap(request) => {
            let normalized = normalized
                .ok_or_else(|| generation_error("difference-map normalization is missing"))?;
            let reference = resolve_reference(request.reference, epoch)?;
            let generated = temporal_vision::render_difference_map_with_analysis(
                artifact_ids[0],
                &epoch.sequence,
                normalized,
                DifferenceMapParameters::new(
                    reference,
                    request.frequency_mode,
                    TimePalette::Spectral,
                    request
                        .repeated_change_separation_nanos
                        .map(Timestamp::from_nanos),
                    MeasurementParameters::new(request.noise_floor),
                    request.canvas_background,
                    DifferenceMapLimits::new(
                        NonZeroUsize::new(limits.max_normalized_bytes.get()).unwrap(),
                        encoded_limit(request.output, limits)?,
                    ),
                ),
                shared,
            )
            .map_err(vision_error)?;
            vec![from_generated(&generated)]
        }
        ArtifactGeneratorRequest::RegionFilmstrip(request) => {
            let mut parameters = RegionFilmstripParameters::new(
                request.region,
                Timestamp::from_nanos(request.anchor.as_nanos()),
                FilmstripTileLimit::new(request.tile_limit).map_err(vision_error)?,
                request.background,
                request.padding,
                integer_scale(request.display_scale)?,
                RegionFilmstripLabels::new(
                    request.labels.title.as_str(),
                    request.labels.source.as_str(),
                )
                .map_err(vision_error)?,
                RegionFilmstripRenderLimits::new(
                    NonZeroU32::new(request.output.max_width()).unwrap(),
                    NonZeroU32::new(request.output.max_height()).unwrap(),
                    NonZeroUsize::new(limits.max_combined_request_bytes.get()).unwrap(),
                    encoded_limit(request.output, limits)?,
                )
                .with_max_source_frames(limits.max_source_frames),
            );
            if let Some(locator) = request.locator {
                parameters = parameters.with_locator_frame_index(
                    epoch
                        .sequence
                        .frames()
                        .iter()
                        .position(|frame| frame.id() == &locator)
                        .ok_or_else(|| {
                            generation_error("filmstrip locator frame is outside this visual epoch")
                        })?,
                );
            }
            if let Some(mask) = &request.mask {
                parameters = parameters.with_mask(mask.clone()).map_err(vision_error)?;
            }
            let generated = temporal_vision::generate_region_filmstrip(
                artifact_ids[0],
                &epoch.sequence,
                parameters,
            )
            .map_err(vision_error)?;
            vec![GeneratedOutput {
                kind: generated.manifest().artifact_kind(),
                manifest: generated.manifest().clone(),
                bytes: Arc::from(generated.image().bytes()),
            }]
        }
        ArtifactGeneratorRequest::MotionHistory(request) => {
            let normalized = normalized
                .ok_or_else(|| generation_error("motion-history normalization is missing"))?;
            let reference = resolve_reference(request.reference, epoch)?;
            let generated = temporal_vision::generate_motion_history_with_analysis(
                artifact_ids[0],
                &epoch.sequence,
                normalized,
                MotionHistoryParameters::new(
                    reference,
                    MeasurementParameters::new(request.noise_floor),
                    MotionDecay::new(
                        request.decay_peak,
                        NonZeroU8::new(request.decay_half_life_ranks).unwrap(),
                    ),
                    request.reference_strength,
                    request.accent,
                    request.outline,
                    ArtifactLabels::new(
                        request.labels.title.as_str(),
                        request.labels.source.as_str(),
                    )
                    .map_err(vision_error)?,
                    render_limits(request.output, limits)?,
                ),
                shared,
            )
            .map_err(vision_error)?;
            vec![from_generated(&generated)]
        }
    };
    validate_outputs(&outputs, &prepared.request, limits)?;
    Ok(outputs)
}

fn from_generated(
    generated: &temporal_vision::GeneratedArtifact<
        ArtifactId,
        FrameId,
        krometrail_core::ArtifactMarkerId,
        krometrail_core::GapId,
    >,
) -> GeneratedOutput {
    GeneratedOutput {
        kind: generated.manifest().artifact_kind(),
        manifest: generated.manifest().clone(),
        bytes: Arc::from(generated.image().bytes()),
    }
}

fn materialize_effective_scales(
    request: &mut ArtifactGeneratorRequest,
    epoch: &super::epoch::EpochPlan,
    limits: ArtifactWorkLimits,
) -> Result<()> {
    let reserved_output_bytes = output_reservation(request, limits)?;
    let normalization = match request {
        ArtifactGeneratorRequest::Storyboard(request) => Some(&mut request.normalization),
        ArtifactGeneratorRequest::DifferenceMap(request) => Some(&mut request.normalization),
        ArtifactGeneratorRequest::MotionHistory(request) => Some(&mut request.normalization),
        ArtifactGeneratorRequest::RegionFilmstrip(_) => None,
    };
    if let Some(normalization) = normalization {
        if normalization.scale == AnalysisScale::FitLimits {
            normalization.scale = fit_scale(*normalization, epoch, limits, reserved_output_bytes)?;
        }
        validate_normalized_limit(*normalization, epoch, limits)?;
    }
    Ok(())
}

fn fit_scale(
    request: NormalizationRequest,
    epoch: &super::epoch::EpochPlan,
    limits: ArtifactWorkLimits,
    reserved_output_bytes: usize,
) -> Result<AnalysisScale> {
    let normalized_budget = limits
        .max_combined_request_bytes
        .get()
        .checked_sub(epoch.decoded_bytes)
        .and_then(|remaining| remaining.checked_sub(reserved_output_bytes))
        .ok_or_else(|| {
            limit_error("decoded frames and bounded output exceed the combined memory limit")
        })?
        .min(limits.max_normalized_bytes.get());
    let (width, height) = crop_adjusted_dimensions(
        request,
        epoch.descriptor.image.width(),
        epoch.descriptor.image.height(),
    );
    let factors = admissible_downscale_factors(width, height);
    for factor in std::iter::once(1).chain(factors.iter().copied()) {
        let scale = if factor == 1 {
            AnalysisScale::Identity
        } else {
            AnalysisScale::Down(factor)
        };
        let effective = NormalizationRequest { scale, ..request };
        if validate_normalized_limit_with_budget(effective, epoch, limits, normalized_budget)
            .is_ok()
        {
            return Ok(scale);
        }
    }
    let fallback_factor = factors.last().copied();
    let (width, height) = fallback_factor
        .map(|factor| (width / u32::from(factor), height / u32::from(factor)))
        .unwrap_or((width, height));
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    let frame_count = u64::try_from(epoch.frames.len()).unwrap_or(u64::MAX);
    let retained = pixels.saturating_mul(6).saturating_mul(frame_count);
    Err(KrometrailError::limit_exceeded(
        ErrorCode::ResourceLimitExceeded,
        "normalized analysis",
        format!("{frame_count} frames require {retained} bytes"),
        format!("{normalized_budget} bytes"),
        None::<String>,
    )
    .with_recovery(
        NonEmptyText::new(
            "narrow the range, crop the analysis region, or pass an explicit smaller normalization.scale",
        )
        .expect("analysis limit recovery is non-empty"),
    ))
}

fn validate_normalized_limit(
    request: NormalizationRequest,
    epoch: &super::epoch::EpochPlan,
    limits: ArtifactWorkLimits,
) -> Result<()> {
    validate_normalized_limit_with_budget(request, epoch, limits, limits.max_normalized_bytes.get())
}

fn validate_normalized_limit_with_budget(
    request: NormalizationRequest,
    epoch: &super::epoch::EpochPlan,
    limits: ArtifactWorkLimits,
    normalized_budget: usize,
) -> Result<()> {
    let (width, height) = normalized_dimensions(
        request,
        epoch.descriptor.image.width(),
        epoch.descriptor.image.height(),
    )?;
    let pixels = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| limit_error("normalized pixel count exceeds this platform"))?;
    let retained = pixels
        .checked_mul(6)
        .and_then(|bytes| bytes.checked_mul(epoch.frames.len()))
        .ok_or_else(|| limit_error("normalized retained bytes overflow"))?;
    if pixels > limits.max_pixels_per_frame.get() || retained > normalized_budget {
        return Err(limit_error(
            "normalized sequence exceeds configured pixel or byte limits",
        ));
    }
    Ok(())
}

pub(crate) fn reserved_output_bytes(
    prepared: &PreparedGenerator,
    limits: ArtifactWorkLimits,
) -> Result<usize> {
    output_reservation(&prepared.request, limits)
}

fn output_reservation(
    request: &ArtifactGeneratorRequest,
    limits: ArtifactWorkLimits,
) -> Result<usize> {
    let per_output = usize::try_from(output_limits(request).max_encoded_bytes())
        .map_err(|_| limit_error("requested output byte limit exceeds this platform"))?
        .min(limits.max_output_bytes_each.get());
    per_output
        .checked_mul(request.output_kinds().len())
        .map(|bytes| bytes.min(limits.max_output_bytes_total.get()))
        .ok_or_else(|| limit_error("bounded output reservation overflows"))
}

fn normalized_dimensions(
    request: NormalizationRequest,
    source_width: u32,
    source_height: u32,
) -> Result<(u32, u32)> {
    let (width, height) = crop_adjusted_dimensions(request, source_width, source_height);
    match request.scale {
        AnalysisScale::Identity => Ok((width, height)),
        AnalysisScale::Down(factor)
            if width % u32::from(factor) == 0 && height % u32::from(factor) == 0 =>
        {
            Ok((width / u32::from(factor), height / u32::from(factor)))
        }
        AnalysisScale::Down(factor) => {
            let common = common_downscale_factors(width, height);
            let message = if common.is_empty() {
                format!(
                    "analysis downscale factor {factor} must exactly divide both dimensions {width}x{height}; no common integer downscale factor exists, so crop the analysis region"
                )
            } else {
                format!(
                    "analysis downscale factor {factor} must exactly divide both dimensions {width}x{height}; common factors representable by analysis.scale are {common:?}; requests are limited to factors {MIN_ANALYSIS_DOWNSCALE_FACTOR}..={MAX_ANALYSIS_DOWNSCALE_FACTOR}"
                )
            };
            Err(generation_error(message))
        }
        AnalysisScale::FitLimits => Err(generation_error("fit-limits scale was not materialized")),
    }
}

fn crop_adjusted_dimensions(
    request: NormalizationRequest,
    source_width: u32,
    source_height: u32,
) -> (u32, u32) {
    request.crop.map_or((source_width, source_height), |crop| {
        (crop.width(), crop.height())
    })
}

fn admissible_downscale_factors(width: u32, height: u32) -> Vec<u8> {
    common_downscale_factors(width, height)
        .into_iter()
        .filter(|factor| *factor <= 8)
        .collect()
}

fn common_downscale_factors(width: u32, height: u32) -> Vec<u8> {
    let mut left = width;
    let mut right = height;
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    (2..=u8::MAX)
        .filter(|factor| left % u32::from(*factor) == 0)
        .collect()
}

fn to_normalization(
    request: NormalizationRequest,
    limits: ArtifactWorkLimits,
) -> Result<NormalizationParameters> {
    Ok(NormalizationParameters::new(
        request.background,
        request.crop,
        integer_scale(request.scale)?,
        ProcessingLimits::new(
            limits.max_source_frames,
            limits.max_pixels_per_frame,
            limits.max_normalized_bytes,
        ),
    ))
}

fn integer_scale(scale: AnalysisScale) -> Result<IntegerScale> {
    match scale {
        AnalysisScale::Identity => Ok(IntegerScale::IDENTITY),
        AnalysisScale::Down(factor) => IntegerScale::down(
            NonZeroU8::new(factor).ok_or_else(|| generation_error("analysis scale is zero"))?,
        )
        .map_err(vision_error),
        AnalysisScale::FitLimits => Err(generation_error("fit-limits scale was not materialized")),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use std::num::NonZeroUsize;

    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, ErrorCode, ImageFormat,
        SessionId, SessionTime, TargetId, VisualEpoch,
    };
    use temporal_vision::{Rgb8, TimeRange, Timestamp};
    use uuid::Uuid;

    use super::*;

    fn epoch(width: u32, height: u32, frame_count: usize) -> super::super::epoch::EpochPlan {
        let session_id = SessionId::from_uuid(Uuid::from_u128(1));
        let target_id = TargetId::from_uuid(Uuid::from_u128(2));
        let image = krometrail_core::PixelDimensions::new(width, height).unwrap();
        let frames: Vec<_> = (0..frame_count)
            .map(|index| {
                let ordinal = u64::try_from(index + 1).unwrap();
                let metadata = CapturedFrame::new(
                    krometrail_core::FrameId::from_uuid(Uuid::from_u128(
                        100 + u128::try_from(index).unwrap(),
                    )),
                    session_id,
                    target_id,
                    CaptureOrdinal::new(ordinal).unwrap(),
                    None,
                    krometrail_core::ObservedTime::from_nanos(ordinal + 1),
                    SessionTime::from_nanos(ordinal),
                    ImageFormat::Png,
                    image,
                    image,
                    DeviceScaleFactor::new(1.0).unwrap(),
                    vec![],
                )
                .unwrap();
                EncodedFrame::new(metadata, vec![1]).unwrap()
            })
            .collect();
        let descriptor = VisualEpoch {
            index: 0,
            frame_ids: frames.iter().map(|frame| frame.metadata().id()).collect(),
            image,
            viewport: image,
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
        };
        super::super::epoch::EpochPlan {
            descriptor,
            source_fingerprints: vec![],
            cache_sources: vec![],
            frames,
            markers: vec![],
            gaps: vec![],
            decoded_bytes: 0,
            source_frame_ids: vec![],
            source_indices: vec![],
            source_range: TimeRange::new(Timestamp::from_nanos(1), Timestamp::from_nanos(2))
                .unwrap(),
        }
    }

    fn request(scale: AnalysisScale) -> NormalizationRequest {
        NormalizationRequest::new(None, Rgb8::new(0, 0, 0), scale).unwrap()
    }

    fn limits(max_normalized_bytes: usize) -> ArtifactWorkLimits {
        ArtifactWorkLimits {
            max_normalized_bytes: NonZeroUsize::new(max_normalized_bytes).unwrap(),
            ..ArtifactWorkLimits::default()
        }
    }

    #[test]
    fn fit_scale_uses_real_divisors_for_non_power_of_two_viewports() {
        let plan = epoch(1673, 1288, 42);
        let scale = fit_scale(
            request(AnalysisScale::FitLimits),
            &plan,
            limits(20_000_000),
            0,
        )
        .unwrap();
        assert_eq!(scale, AnalysisScale::Down(7));
    }

    #[test]
    fn fit_scale_keeps_smallest_admissible_factor_for_standard_viewports() {
        let plan = epoch(1920, 1080, 1);
        let scale = fit_scale(
            request(AnalysisScale::FitLimits),
            &plan,
            limits(4_000_000),
            0,
        )
        .unwrap();
        assert_eq!(scale, AnalysisScale::Down(2));
    }

    #[test]
    fn fit_scale_returns_resource_limit_error_when_no_admissible_factor_fits() {
        let plan = epoch(1673, 1289, 1);
        let error = fit_scale(
            request(AnalysisScale::FitLimits),
            &plan,
            limits(1_000_000),
            0,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ResourceLimitExceeded);
    }

    #[test]
    fn explicit_downscale_error_reports_dimensions_and_admissible_factors() {
        let error = normalized_dimensions(request(AnalysisScale::Down(2)), 1673, 1288).unwrap_err();
        let message = error.message.to_string();
        assert!(message.contains("1673x1288"));
        assert!(message.contains("[7]"));
    }

    #[test]
    fn explicit_downscale_error_reports_common_factors_beyond_fit_search_bound() {
        let error = normalized_dimensions(request(AnalysisScale::Down(2)), 47, 94).unwrap_err();
        let message = error.message.to_string();
        assert!(message.contains("[47]"));
        assert_eq!(
            message,
            "analysis downscale factor 2 must exactly divide both dimensions 47x94; common factors representable by analysis.scale are [47]; requests are limited to factors 2..=8"
        );
    }

    #[test]
    fn explicit_downscale_error_explains_that_cropping_is_required_without_common_factors() {
        let error = normalized_dimensions(request(AnalysisScale::Down(2)), 47, 48).unwrap_err();
        let message = error.message.to_string();
        assert!(message.contains("no common integer downscale factor exists"));
        assert!(message.contains("crop the analysis region"));
    }
}

fn resolve_reference(selector: FrameSelector, epoch: &EpochInput) -> Result<usize> {
    match selector {
        FrameSelector::First => Ok(0),
        FrameSelector::Last => Ok(epoch.sequence.frames().len() - 1),
        FrameSelector::Frame(id) => epoch
            .sequence
            .frames()
            .iter()
            .position(|frame| frame.id() == &id)
            .ok_or_else(|| {
                // Sampling now retains an explicitly named reference frame, so reaching
                // here means the frame genuinely belongs to another visual epoch.
                generation_error(format!(
                    "reference frame {id} is not a source frame of this visual epoch"
                ))
                .with_context(epoch.context.clone())
                .with_recovery(
                    NonEmptyText::new(
                        "select a reference frame from this epoch's source frames, or resolve a range that contains it",
                    )
                    .expect("reference recovery is non-empty"),
                )
            }),
    }
}

fn validate_output_limits(
    request: &ArtifactGeneratorRequest,
    limits: ArtifactWorkLimits,
) -> Result<()> {
    let output = output_limits(request);
    if output.max_width() > limits.max_dimension.get()
        || output.max_height() > limits.max_dimension.get()
        || output.max_encoded_bytes() > limits.max_output_bytes_each.get() as u64
    {
        return Err(limit_error(
            "requested artifact output limits exceed runtime caps",
        ));
    }
    Ok(())
}

fn validate_outputs(
    outputs: &[GeneratedOutput],
    request: &ArtifactGeneratorRequest,
    limits: ArtifactWorkLimits,
) -> Result<()> {
    let requested = output_limits(request);
    for output in outputs {
        if output.bytes.len() > requested.max_encoded_bytes() as usize
            || output.bytes.len() > limits.max_output_bytes_each.get()
            || output.manifest.output_dimensions().width() > requested.max_width()
            || output.manifest.output_dimensions().height() > requested.max_height()
        {
            return Err(limit_error(
                "generated artifact exceeds requested output limits",
            ));
        }
    }
    Ok(())
}

fn render_limits(
    output: krometrail_core::OutputLimitsRequest,
    limits: ArtifactWorkLimits,
) -> Result<RenderLimits> {
    Ok(RenderLimits::new(
        NonZeroU32::new(output.max_width()).unwrap(),
        NonZeroU32::new(output.max_height()).unwrap(),
        NonZeroUsize::new(limits.max_combined_request_bytes.get()).unwrap(),
        encoded_limit(output, limits)?,
    ))
}

fn encoded_limit(
    output: krometrail_core::OutputLimitsRequest,
    limits: ArtifactWorkLimits,
) -> Result<NonZeroUsize> {
    let requested = usize::try_from(output.max_encoded_bytes())
        .map_err(|_| limit_error("requested output byte limit exceeds this platform"))?;
    NonZeroUsize::new(requested.min(limits.max_output_bytes_each.get()))
        .ok_or_else(|| limit_error("effective output byte limit is zero"))
}

pub(crate) fn vision_error(error: temporal_vision::VisionError) -> KrometrailError {
    let code = if error.code == temporal_vision::ErrorCode::ResourceLimitExceeded {
        ErrorCode::ResourceLimitExceeded
    } else {
        ErrorCode::ArtifactGenerationFailed
    };
    let error = KrometrailError::new(
        code,
        NonEmptyText::new(format!(
            "temporal visual generation failed: {}",
            error.message
        ))
        .expect("generation errors are non-empty"),
    );
    if code == ErrorCode::ResourceLimitExceeded {
        error.with_recovery(
            NonEmptyText::new(
                "select fewer source frames, crop the analysis region, or reduce the requested output dimensions",
            )
            .expect("visual limit recovery is non-empty"),
        )
    } else {
        error
    }
}
fn generation_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message).expect("generation errors are non-empty"),
    )
}

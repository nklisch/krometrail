use std::num::NonZeroU32;

use krometrail_core::{
    ArtifactFailurePolicy, ArtifactGenerationRequest, ArtifactGeneratorRequest, CallerRegionShape,
    CurrentReferenceGeometryRequest, ErrorCode, ErrorContext, KrometrailError, NonEmptyText,
    ProgressiveEvidenceContext, ProgressiveEvidenceStore, ProgressiveRegion,
    RegionFilmstripEvidenceRequest, RegionFilmstripRequest, ResolvedProgressiveRegion, Result,
    RetryAdvice, VisualEpoch,
};
use temporal_vision::{RegionDefinition, SignedPixelRect, ViewportMapping};

pub(super) struct PreparedRegion {
    pub resolved: ResolvedProgressiveRegion,
    pub generation: ArtifactGenerationRequest,
    pub epoch: VisualEpoch,
}

pub(super) async fn prepare_region(
    store: &dyn ProgressiveEvidenceStore,
    request: RegionFilmstripEvidenceRequest,
    context: &ProgressiveEvidenceContext,
) -> Result<PreparedRegion> {
    let anchor_was_defaulted = request.anchor_was_defaulted();
    // Direct in-process callers can construct public-field values without Serde. Re-run the same
    // boundary constructor before any store or browser read.
    let request = RegionFilmstripEvidenceRequest::new(
        request.range,
        request.region,
        request.markers,
        request.anchor,
        request.tile_limit,
        request.background,
        request.padding,
        request.display_scale,
        request.labels,
        request.output,
    )?;
    // Live geometry is sampled before touching retained metadata. The browser read therefore
    // cannot overlap a recording-store gate, and the sampled rectangle remains a fixed input to
    // the later source-frame mapping rather than a historical node lookup.
    let sampled_reference_geometry = match &request.region {
        ProgressiveRegion::CurrentReference {
            session_id,
            reference,
            ..
        } => {
            let geometry_port = context.current_reference_geometry.as_ref().ok_or_else(|| {
                lifecycle_error(
                    request.range.session_id,
                    request.range.target_id,
                    "current-reference geometry requires an active browser session",
                )
            })?;
            let geometry_request = CurrentReferenceGeometryRequest::new(*session_id, *reference)?;
            let geometry = geometry_port
                .current_reference_geometry(geometry_request)
                .await?;
            if geometry.session_id != *session_id
                || geometry.target_id != request.range.target_id
                || geometry.reference != *reference
            {
                return Err(stale_geometry_error(
                    request.range.session_id,
                    request.range.target_id,
                ));
            }
            Some(geometry)
        }
        _ => None,
    };

    let frames = store
        .frame_metadata_by_id(request.range.frame_ids.clone())
        .await?;
    let epoch = request.validate_epoch(&frames)?;
    if frames
        .iter()
        .any(|frame| !request.range.resolved_range.contains(frame.session_time()))
        || frames
            .windows(2)
            .any(|pair| pair[0].session_time() > pair[1].session_time())
    {
        return Err(region_input_error(
            "region frame timing contradicts the resolved range",
        ));
    }
    let source_frame = frames
        .iter()
        .find(|frame| frame.id() == request.region.source_frame_id())
        .cloned()
        .ok_or_else(|| region_input_error("region locator frame is not retained"))?;

    let mut request = request;
    if anchor_was_defaulted {
        // Omitted filmstrip anchors use the declared source frame's session time so the
        // canonical default is inside both the resolved range and the visual source sequence.
        request.anchor = source_frame.session_time();
    }

    let mut mask = None;
    let mut viewport_mapping = None;
    let mut reference_geometry = None;
    let temporal_region = match &request.region {
        ProgressiveRegion::SourcePixels { rect, .. } => {
            RegionDefinition::FixedSourceImage { rect: *rect }
        }
        ProgressiveRegion::ViewportCss { rect, .. } => {
            let (region, mapping) = viewport_region(*rect, &source_frame)?;
            viewport_mapping = Some(mapping);
            region
        }
        ProgressiveRegion::SelectedFromSourceFrame {
            shape: CallerRegionShape::Rect { rect },
            ..
        } => RegionDefinition::FixedSourceImage { rect: *rect },
        ProgressiveRegion::SelectedFromSourceFrame {
            shape: CallerRegionShape::Mask { mask: selected },
            ..
        } => {
            let bounds = selected
                .bounds()
                .map_err(vision_region_error)?
                .ok_or_else(|| region_input_error("region mask selects no source pixels"))?;
            mask = Some(selected.clone());
            RegionDefinition::FixedSourceImage {
                rect: signed(bounds)?,
            }
        }
        ProgressiveRegion::CurrentReference { .. } => {
            let geometry = sampled_reference_geometry
                .expect("current-reference branch samples geometry before store metadata");
            let (region, mapping) = viewport_region(geometry.viewport_css_rect, &source_frame)?;
            viewport_mapping = Some(mapping);
            reference_geometry = Some(geometry);
            region
        }
    };

    let resolved = ResolvedProgressiveRegion {
        declared: request.region.clone(),
        source_frame,
        temporal_region,
        mask: mask.clone(),
        viewport_mapping,
        reference_geometry,
    };
    let generation = ArtifactGenerationRequest::new(
        request.range,
        request.markers,
        vec![ArtifactGeneratorRequest::RegionFilmstrip(
            RegionFilmstripRequest {
                region: temporal_region,
                mask,
                anchor: request.anchor,
                tile_limit: request.tile_limit,
                locator: Some(request.region.source_frame_id()),
                background: request.background,
                padding: request.padding,
                display_scale: request.display_scale,
                labels: request.labels,
                output: request.output,
            },
        )],
        ArtifactFailurePolicy::RequireAll,
    )?;
    Ok(PreparedRegion {
        resolved,
        generation,
        epoch,
    })
}

fn viewport_region(
    rect: krometrail_core::CssRect,
    source_frame: &krometrail_core::CapturedFrame,
) -> Result<(RegionDefinition, ViewportMapping)> {
    let rect = SignedPixelRect::from_outward_f64_bounds(
        rect.origin.x,
        rect.origin.y,
        rect.right(),
        rect.bottom(),
    )
    .map_err(vision_region_error)?;
    let viewport = temporal_vision::PixelDimensions::new(
        source_frame.viewport().width(),
        source_frame.viewport().height(),
    )
    .map_err(vision_region_error)?;
    let source = temporal_vision::PixelDimensions::new(
        source_frame.image().width(),
        source_frame.image().height(),
    )
    .map_err(vision_region_error)?;
    let mapping = ViewportMapping::for_source(viewport, source);
    Ok((RegionDefinition::FixedViewport { rect, mapping }, mapping))
}

fn signed(rect: temporal_vision::PixelRect) -> Result<SignedPixelRect> {
    SignedPixelRect::new(
        i64::from(rect.x()),
        i64::from(rect.y()),
        NonZeroU32::new(rect.width()).expect("validated mask bound width is non-zero"),
        NonZeroU32::new(rect.height()).expect("validated mask bound height is non-zero"),
    )
    .map_err(vision_region_error)
}

fn vision_region_error(error: temporal_vision::VisionError) -> KrometrailError {
    region_input_error(format!(
        "fixed progressive region is invalid: {}",
        error.message
    ))
}

fn region_input_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message).expect("progressive region errors are non-empty"),
    )
}

fn lifecycle_error(
    session_id: krometrail_core::SessionId,
    target_id: krometrail_core::TargetId,
    message: &'static str,
) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidLifecycleTransition,
        NonEmptyText::new(message).expect("progressive lifecycle errors are non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        interaction_id: None,
        range: None,
    })
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("request a current structured snapshot from the active browser session")
            .expect("progressive lifecycle recovery is non-empty"),
    )
}

fn stale_geometry_error(
    session_id: krometrail_core::SessionId,
    target_id: krometrail_core::TargetId,
) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::StaleReference,
        NonEmptyText::new("current-reference geometry disagrees with the resolved evidence scope")
            .expect("progressive stale-reference error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        interaction_id: None,
        range: None,
    })
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("request a new structured snapshot and retry with its reference")
            .expect("progressive stale-reference recovery is non-empty"),
    )
}

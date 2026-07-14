use std::future::Future;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    BrowserOperationResult, CoordinateSpace, CssPoint, CssRect, CssSize, DeviceScaleFactor,
    ElementLocator, EncodedScreenshot, ErrorCode, InspectPageRequest, LiveObservation,
    LiveObservationRequest, ObservationContext, ObservationPart, Result, ScreenshotMetadata,
    ScreenshotRequest, ScreenshotTarget, SnapshotPageRequest,
};
use serde_json::{Map, Value, json};

use super::{
    BoundTarget, PageControl,
    navigation::OperationCancellation,
    operation_error,
    snapshot::{ReferenceRequirement, quad_bounds},
    transport_error,
};
use crate::{
    capture::image_header,
    transport::{CdpTransport, CommandScope},
};

const MAX_SCREENSHOT_BASE64_BYTES: usize = 24 * 1024 * 1024;
const MAX_SCREENSHOT_DECODED_BYTES: usize = 16 * 1024 * 1024;

impl PageControl {
    pub(super) async fn screenshot(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: ScreenshotRequest,
        started_at: krometrail_core::SessionTime,
    ) -> Result<BrowserOperationResult> {
        let screenshot = self
            .capture_screenshot(transport, bound, request, started_at)
            .await?;
        Ok(BrowserOperationResult::TakeScreenshot(Box::new(screenshot)))
    }

    async fn capture_screenshot(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: ScreenshotRequest,
        started_at: krometrail_core::SessionTime,
    ) -> Result<EncodedScreenshot> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let layout = transport
            .send_raw(&scope, "Page.getLayoutMetrics", json!({}))
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::ScreenshotFailed, bound.target_id)
            })?;
        let layout = layout
            .get("result")
            .filter(|value| value.get("cssLayoutViewport").is_some())
            .unwrap_or(&layout);
        let viewport = protocol_rect(
            layout.get("cssLayoutViewport"),
            "layout viewport",
            bound.target_id,
            "pageX",
            "pageY",
            "clientWidth",
            "clientHeight",
        )?;
        let content = protocol_rect(
            layout.get("cssContentSize"),
            "content size",
            bound.target_id,
            "x",
            "y",
            "width",
            "height",
        )?;
        let scale_response = transport
            .send_raw(
                &scope,
                "Runtime.evaluate",
                json!({
                    "expression": "window.devicePixelRatio", "returnByValue": true,
                    "throwOnSideEffect": true, "silent": true,
                }),
            )
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::ScreenshotFailed, bound.target_id)
            })?;
        let scale = scale_response
            .pointer("/result/value")
            .or_else(|| scale_response.pointer("/result/result/value"))
            .and_then(Value::as_f64)
            .ok_or_else(|| {
                screenshot_malformed(bound.target_id, "device scale response is malformed")
            })?;
        let device_scale_factor = DeviceScaleFactor::new(scale)
            .map_err(|_| screenshot_malformed(bound.target_id, "device scale factor is invalid"))?;

        let requested_target = request.target.clone();
        let (clip, capture_beyond) = match &request.target {
            ScreenshotTarget::Viewport => (None, false),
            ScreenshotTarget::FullPage => (Some(content), true),
            ScreenshotTarget::Region {
                rect,
                space: CoordinateSpace::DocumentCss,
            } => {
                ensure_contains(
                    content,
                    *rect,
                    bound.target_id,
                    "document region lies outside page content",
                )?;
                (Some(*rect), true)
            }
            ScreenshotTarget::Region {
                rect,
                space: CoordinateSpace::ViewportCss,
            } => {
                let local_viewport = CssRect::new(CssPoint::new(0.0, 0.0)?, viewport.size)?;
                ensure_contains(
                    local_viewport,
                    *rect,
                    bound.target_id,
                    "viewport region lies outside the current viewport",
                )?;
                (
                    Some(CssRect::new(
                        CssPoint::new(
                            viewport.origin.x + rect.origin.x,
                            viewport.origin.y + rect.origin.y,
                        )?,
                        rect.size,
                    )?),
                    true,
                )
            }
            ScreenshotTarget::Element(ElementLocator::Reference(reference)) => {
                let resolved = self
                    .snapshots
                    .resolve(
                        transport,
                        bound,
                        *reference,
                        ReferenceRequirement::VisibleGeometry,
                    )
                    .await?;
                let (min_x, max_x, min_y, max_y) = quad_bounds(&resolved.document_quad);
                (
                    Some(CssRect::new(
                        CssPoint::new(min_x, min_y)?,
                        CssSize::new(max_x - min_x, max_y - min_y)?,
                    )?),
                    true,
                )
            }
            ScreenshotTarget::Element(ElementLocator::CssSelector(selector)) => {
                let resolved = self
                    .snapshots
                    .resolve_selector(
                        transport,
                        bound,
                        selector.as_str(),
                        ReferenceRequirement::VisibleGeometry,
                    )
                    .await?;
                let (min_x, max_x, min_y, max_y) = quad_bounds(&resolved.document_quad);
                (
                    Some(CssRect::new(
                        CssPoint::new(min_x, min_y)?,
                        CssSize::new(max_x - min_x, max_y - min_y)?,
                    )?),
                    true,
                )
            }
        };
        if let Some(clip) = clip {
            ensure_contains(
                content,
                clip,
                bound.target_id,
                "screenshot clip lies outside page content",
            )?;
        }
        let mut params = Map::new();
        params.insert(
            "format".into(),
            Value::String(
                match request.format {
                    krometrail_core::ImageFormat::Png => "png",
                    krometrail_core::ImageFormat::Jpeg => "jpeg",
                }
                .into(),
            ),
        );
        params.insert("captureBeyondViewport".into(), Value::Bool(capture_beyond));
        params.insert("fromSurface".into(), Value::Bool(true));
        if let Some(quality) = request.jpeg_quality {
            params.insert("quality".into(), Value::from(quality));
        }
        if let Some(rect) = clip {
            params.insert("clip".into(), json!({"x":rect.origin.x,"y":rect.origin.y,"width":rect.size.width,"height":rect.size.height,"scale":1.0}));
        }
        let response = transport
            .send_raw(&scope, "Page.captureScreenshot", Value::Object(params))
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::ScreenshotFailed, bound.target_id)
            })?;
        let data = response
            .get("data")
            .or_else(|| response.pointer("/result/data"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                screenshot_malformed(bound.target_id, "screenshot response contains no image")
            })?;
        if data.is_empty() || data.len() > MAX_SCREENSHOT_BASE64_BYTES {
            return Err(screenshot_malformed(
                bound.target_id,
                "screenshot payload is empty or exceeds the encoded limit",
            ));
        }
        let decoded_bound = data
            .len()
            .checked_mul(3)
            .and_then(|value| value.checked_div(4))
            .ok_or_else(|| {
                screenshot_malformed(bound.target_id, "screenshot size arithmetic overflow")
            })?;
        if decoded_bound > MAX_SCREENSHOT_DECODED_BYTES {
            return Err(screenshot_malformed(
                bound.target_id,
                "screenshot exceeds the decoded payload limit",
            ));
        }
        let bytes = STANDARD.decode(data).map_err(|_| {
            screenshot_malformed(bound.target_id, "screenshot payload is not valid base64")
        })?;
        if bytes.len() > MAX_SCREENSHOT_DECODED_BYTES {
            return Err(screenshot_malformed(
                bound.target_id,
                "screenshot exceeds the decoded payload limit",
            ));
        }
        let image = image_header::dimensions(request.format, &bytes).map_err(|_| {
            screenshot_malformed(
                bound.target_id,
                "screenshot image header does not match the requested format",
            )
        })?;
        let completed_at = self.session_time()?;
        let context = ObservationContext::new(
            self.session_id,
            bound.target_id,
            bound.attachment_generation,
            started_at,
            completed_at,
        )?;
        let resolved_document_rect = clip.unwrap_or(viewport);
        EncodedScreenshot::new(
            ScreenshotMetadata::new(
                context,
                requested_target,
                resolved_document_rect,
                image,
                device_scale_factor,
            )?,
            bytes,
        )
    }

    pub(super) async fn observe_live(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: LiveObservationRequest,
        started_at: krometrail_core::SessionTime,
        cancellation: Option<(&OperationCancellation, u64)>,
    ) -> Result<(
        BrowserOperationResult,
        Option<krometrail_core::KrometrailError>,
    )> {
        let mut interruption = None;

        let page = if transport.is_closed() {
            unavailable_disconnected(bound.target_id, cancellation, &mut interruption)
        } else {
            let part_started = self.session_time()?;
            let inspection = self.inspect(
                transport,
                bound,
                InspectPageRequest::new(bound.target_id),
                part_started,
            );
            match run_component(cancellation, bound.target_id, inspection).await {
                ComponentResult::Completed(Ok(BrowserOperationResult::InspectPage(page))) => {
                    ObservationPart::Available(*page)
                }
                ComponentResult::Completed(Ok(_)) => {
                    unreachable!("inspection returns its associated result")
                }
                ComponentResult::Completed(Err(error)) => ObservationPart::Unavailable(error),
                ComponentResult::Interrupted(error) => {
                    interruption = Some(error.clone());
                    ObservationPart::Unavailable(error)
                }
            }
        };

        let snapshot = if let Some(error) = &interruption {
            ObservationPart::Unavailable(error.clone())
        } else if transport.is_closed() {
            unavailable_disconnected(bound.target_id, cancellation, &mut interruption)
        } else {
            let part_started = self.session_time()?;
            let snapshot = self.snapshot(
                transport,
                bound,
                SnapshotPageRequest::new(bound.target_id),
                part_started,
            );
            match run_component(cancellation, bound.target_id, snapshot).await {
                ComponentResult::Completed(Ok(BrowserOperationResult::SnapshotPage(snapshot))) => {
                    ObservationPart::Available(*snapshot)
                }
                ComponentResult::Completed(Ok(_)) => {
                    unreachable!("snapshot returns its associated result")
                }
                ComponentResult::Completed(Err(error)) => ObservationPart::Unavailable(error),
                ComponentResult::Interrupted(error) => {
                    interruption = Some(error.clone());
                    ObservationPart::Unavailable(error)
                }
            }
        };

        let screenshot = if let Some(error) = &interruption {
            ObservationPart::Unavailable(error.clone())
        } else if transport.is_closed() {
            unavailable_disconnected(bound.target_id, cancellation, &mut interruption)
        } else {
            let part_started = self.session_time()?;
            let request = ScreenshotRequest::new(
                bound.target_id,
                ScreenshotTarget::Viewport,
                krometrail_core::ImageFormat::Png,
                None,
            )?;
            let screenshot = self.capture_screenshot(transport, bound, request, part_started);
            match run_component(cancellation, bound.target_id, screenshot).await {
                ComponentResult::Completed(Ok(screenshot)) => {
                    ObservationPart::Available(screenshot)
                }
                ComponentResult::Completed(Err(error)) => ObservationPart::Unavailable(error),
                ComponentResult::Interrupted(error) => {
                    interruption = Some(error.clone());
                    ObservationPart::Unavailable(error)
                }
            }
        };
        let completed_at = self.session_time()?;
        Ok((
            BrowserOperationResult::ObserveLive(Box::new(LiveObservation {
                context: ObservationContext::new(
                    self.session_id,
                    bound.target_id,
                    bound.attachment_generation,
                    started_at,
                    completed_at,
                )?,
                page,
                snapshot,
                screenshot,
            })),
            interruption,
        ))
    }
}

enum ComponentResult<T> {
    Completed(Result<T>),
    Interrupted(krometrail_core::KrometrailError),
}

async fn run_component<F, T>(
    cancellation: Option<(&OperationCancellation, u64)>,
    target_id: krometrail_core::TargetId,
    future: F,
) -> ComponentResult<T>
where
    F: Future<Output = Result<T>>,
{
    match cancellation {
        Some((cancel, generation)) => match cancel.race(generation, target_id, future).await {
            Ok(result) => ComponentResult::Completed(result),
            Err(error) => ComponentResult::Interrupted(error),
        },
        None => ComponentResult::Completed(future.await),
    }
}

fn unavailable_disconnected<T>(
    target_id: krometrail_core::TargetId,
    cancellation: Option<(&OperationCancellation, u64)>,
    interruption: &mut Option<krometrail_core::KrometrailError>,
) -> ObservationPart<T> {
    let error = disconnected(target_id);
    if cancellation.is_some() {
        *interruption = Some(error.clone());
    }
    ObservationPart::Unavailable(error)
}

fn protocol_rect(
    value: Option<&Value>,
    label: &str,
    target_id: krometrail_core::TargetId,
    x: &str,
    y: &str,
    width: &str,
    height: &str,
) -> Result<CssRect> {
    let value =
        value.ok_or_else(|| screenshot_malformed(target_id, format!("{label} is missing")))?;
    let number = |field: &str| {
        value
            .get(field)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| screenshot_malformed(target_id, format!("{label} {field} is invalid")))
    };
    CssRect::new(
        CssPoint::new(number(x)?, number(y)?)?,
        CssSize::new(number(width)?, number(height)?)?,
    )
}

fn ensure_contains(
    container: CssRect,
    requested: CssRect,
    target_id: krometrail_core::TargetId,
    message: &'static str,
) -> Result<()> {
    if requested.origin.x < container.origin.x
        || requested.origin.y < container.origin.y
        || requested.right() > container.right()
        || requested.bottom() > container.bottom()
    {
        return Err(operation_error(ErrorCode::InvalidInput, target_id, message));
    }
    Ok(())
}

fn screenshot_malformed(
    target_id: krometrail_core::TargetId,
    message: impl Into<String>,
) -> krometrail_core::KrometrailError {
    operation_error(ErrorCode::ScreenshotFailed, target_id, message)
}
fn disconnected(target_id: krometrail_core::TargetId) -> krometrail_core::KrometrailError {
    operation_error(
        ErrorCode::BrowserDisconnected,
        target_id,
        "browser transport disconnected during live observation",
    )
}

use std::future::Future;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    BrowserOperationResult, CoordinateSpace, CssPoint, CssRect, CssSize, DeviceScaleFactor,
    ElementLocator, EncodedScreenshot, ErrorCode, InspectPageRequest, KrometrailError,
    LiveObservation, LiveObservationRequest, NonEmptyText, ObservationContext, ObservationPart,
    Result, ScreenshotMetadata, ScreenshotRequest, ScreenshotTarget, SnapshotPageRequest,
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
const TALL_SCREENSHOT_GUIDANCE_HEIGHT: u32 = 8_192;

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
        let screenshot = EncodedScreenshot::new(
            ScreenshotMetadata::new(
                context,
                requested_target,
                resolved_document_rect,
                image,
                device_scale_factor,
            )?,
            bytes,
        )?;
        if image.height() > TALL_SCREENSHOT_GUIDANCE_HEIGHT {
            return Ok(
                screenshot.with_warning(tall_screenshot_guidance(bound.target_id, image.height()))
            );
        }
        Ok(screenshot)
    }

    pub(super) async fn observe_live(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: LiveObservationRequest,
        started_at: krometrail_core::SessionTime,
        include_document_geometry: bool,
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
                include_document_geometry,
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

fn tall_screenshot_guidance(target_id: krometrail_core::TargetId, height: u32) -> KrometrailError {
    KrometrailError::limit_exceeded(
        ErrorCode::ResourceLimitExceeded,
        "captured screenshot height",
        height,
        TALL_SCREENSHOT_GUIDANCE_HEIGHT,
        Some(TALL_SCREENSHOT_GUIDANCE_HEIGHT),
    )
    .with_context(krometrail_core::ErrorContext {
        target_id: Some(target_id),
        ..krometrail_core::ErrorContext::default()
    })
    .with_recovery(
        NonEmptyText::new(
            "request an element or region screenshot, or capture viewport images while scrolling",
        )
        .expect("tall screenshot recovery is non-empty"),
    )
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use std::sync::Arc;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use krometrail_core::{IdSource, IdValue, MonotonicClock, ObservedTime, SessionOrigin};

    use super::*;
    use crate::transport::{TransportClose, TransportError, TransportEvents, TransportSessionId};

    struct TestClock;

    impl MonotonicClock for TestClock {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(0)
        }
    }

    struct TestIds;

    impl IdSource for TestIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(uuid::Uuid::from_u128(2))
        }
    }

    struct EmptyEvents;

    impl TransportEvents for EmptyEvents {
        fn next(
            &mut self,
        ) -> crate::transport::TransportFuture<
            '_,
            std::result::Result<Option<crate::transport::NamedEvent>, TransportError>,
        > {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    struct ScreenshotTransport {
        height: u32,
    }

    impl CdpTransport for ScreenshotTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            _params: Value,
        ) -> crate::transport::TransportFuture<'_, std::result::Result<Value, TransportError>>
        {
            let response = match method {
                "Page.getLayoutMetrics" => json!({
                    "cssLayoutViewport": {"pageX": 0.0, "pageY": 0.0, "clientWidth": 800.0, "clientHeight": 600.0},
                    "cssContentSize": {"x": 0.0, "y": 0.0, "width": 800.0, "height": self.height}
                }),
                "Runtime.evaluate" => json!({"result": {"value": 1.0}}),
                "Page.captureScreenshot" => {
                    json!({"data": STANDARD.encode(png_header(800, self.height))})
                }
                _ => json!({}),
            };
            Box::pin(std::future::ready(Ok(response)))
        }

        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> crate::transport::TransportFuture<
            '_,
            std::result::Result<Box<dyn TransportEvents>, TransportError>,
        > {
            Box::pin(std::future::ready(Ok(
                Box::new(EmptyEvents) as Box<dyn TransportEvents>
            )))
        }

        fn close_reason(&self) -> Option<TransportClose> {
            None
        }

        fn is_closed(&self) -> bool {
            false
        }
    }

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13_u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn bound() -> BoundTarget {
        BoundTarget {
            target_id: krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1)),
            browser_target_key: "target".to_owned(),
            attachment_generation: 1,
            transport_session: TransportSessionId::new("session").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        }
    }

    fn control() -> PageControl {
        PageControl::new(
            Arc::new(TestClock),
            Arc::new(TestIds),
            krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(3)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
        )
    }

    async fn capture_height(height: u32) -> EncodedScreenshot {
        control()
            .capture_screenshot(
                &ScreenshotTransport { height },
                &bound(),
                ScreenshotRequest::new(
                    krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1)),
                    ScreenshotTarget::FullPage,
                    krometrail_core::ImageFormat::Png,
                    None,
                )
                .unwrap(),
                krometrail_core::SessionTime::ZERO,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn full_page_screenshot_above_height_guidance_threshold_has_one_warning() {
        let screenshot = capture_height(TALL_SCREENSHOT_GUIDANCE_HEIGHT + 1).await;
        assert_eq!(screenshot.metadata().image.height(), 8_193);
        assert_eq!(screenshot.warnings().len(), 1);
        assert!(screenshot.warnings()[0].message.as_str().contains("8193"));
        assert!(screenshot.warnings()[0].message.as_str().contains("8192"));
    }

    #[tokio::test]
    async fn full_page_screenshot_at_height_guidance_threshold_has_no_warning() {
        let screenshot = capture_height(TALL_SCREENSHOT_GUIDANCE_HEIGHT).await;
        assert_eq!(screenshot.metadata().image.height(), 8_192);
        assert!(screenshot.warnings().is_empty());
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

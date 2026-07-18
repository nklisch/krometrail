use krometrail_core::{
    CssSize, DeviceScaleFactor, EffectiveViewport, ErrorCode, PixelDimensions, Result,
    ViewportMetrics,
};
use serde_json::{Value, json};

use super::{BoundTarget, operation_error, transport_error};
use crate::capture::CaptureGeometry;
use crate::transport::{CdpTransport, CommandScope};

pub(crate) async fn apply_viewport(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    viewport: Option<ViewportMetrics>,
) -> Result<()> {
    let scope = CommandScope::Session(bound.transport_session.clone());
    match viewport {
        Some(metrics) => {
            transport
                .send_raw(
                    &scope,
                    "Emulation.setDeviceMetricsOverride",
                    json!({
                        "width": metrics.width(),
                        "height": metrics.height(),
                        "deviceScaleFactor": metrics.device_scale_factor().get(),
                        "mobile": metrics.mobile(),
                        "screenWidth": metrics.width(),
                        "screenHeight": metrics.height(),
                    }),
                )
                .await
                .map_err(|error| {
                    transport_error(error, ErrorCode::TargetFailed, bound.target_id)
                })?;
            transport
                .send_raw(
                    &scope,
                    "Emulation.setTouchEmulationEnabled",
                    touch_emulation_params(metrics.touch()),
                )
                .await
                .map_err(|error| {
                    transport_error(error, ErrorCode::TargetFailed, bound.target_id)
                })?;
            if metrics.mobile() {
                transport
                    .send_raw(
                        &scope,
                        "Emulation.setPageScaleFactor",
                        json!({"pageScaleFactor": 1}),
                    )
                    .await
                    .map_err(|error| {
                        transport_error(error, ErrorCode::TargetFailed, bound.target_id)
                    })?;
            }
        }
        None => {
            transport
                .send_raw(
                    &scope,
                    "Emulation.setTouchEmulationEnabled",
                    touch_emulation_params(false),
                )
                .await
                .map_err(|error| {
                    transport_error(error, ErrorCode::TargetFailed, bound.target_id)
                })?;
            transport
                .send_raw(&scope, "Emulation.clearDeviceMetricsOverride", json!({}))
                .await
                .map_err(|error| {
                    transport_error(error, ErrorCode::TargetFailed, bound.target_id)
                })?;
            transport
                .send_raw(&scope, "Emulation.resetPageScaleFactor", json!({}))
                .await
                .map_err(|error| {
                    transport_error(error, ErrorCode::TargetFailed, bound.target_id)
                })?;
        }
    }
    Ok(())
}

pub(crate) fn touch_emulation_params(enabled: bool) -> Value {
    if enabled {
        json!({"enabled": true, "maxTouchPoints": 1})
    } else {
        // CDP validates maxTouchPoints as 1..=16 even when emulation is disabled.
        json!({"enabled": false})
    }
}

pub(crate) async fn observe_effective_viewport(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    declared: Option<ViewportMetrics>,
) -> Result<EffectiveViewport> {
    let scope = CommandScope::Session(bound.transport_session.clone());
    let layout = transport
        .send_raw(&scope, "Page.getLayoutMetrics", json!({}))
        .await
        .map_err(|error| {
            transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
        })?;
    let runtime = transport
        .send_raw(
            &scope,
            "Runtime.evaluate",
            json!({
                "expression": "({width:innerWidth,height:innerHeight,scale:devicePixelRatio,touchPoints:navigator.maxTouchPoints})",
                "returnByValue": true,
                "throwOnSideEffect": true,
                "silent": true,
            }),
        )
        .await
        .map_err(|error| {
            transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
        })?;
    decode_effective_viewport(&layout, &runtime, declared, bound.target_id)
}

pub(crate) fn capture_geometry(effective: EffectiveViewport) -> Result<CaptureGeometry> {
    Ok(CaptureGeometry {
        viewport: PixelDimensions::new(
            integral_css_dimension(effective.css_size.width)?,
            integral_css_dimension(effective.css_size.height)?,
        )?,
        device_scale_factor: effective.device_scale_factor,
    })
}

fn integral_css_dimension(value: f64) -> Result<u32> {
    let rounded = value.round();
    if !rounded.is_finite()
        || rounded < 1.0
        || rounded > f64::from(u32::MAX)
        || (value - rounded).abs() > 0.5
    {
        return Err(krometrail_core::KrometrailError::new(
            ErrorCode::PageObservationFailed,
            krometrail_core::NonEmptyText::new(
                "browser returned unusable effective viewport geometry",
            )
            .expect("static viewport error is non-empty"),
        ));
    }
    Ok(rounded as u32)
}

fn decode_effective_viewport(
    layout: &Value,
    runtime: &Value,
    declared: Option<ViewportMetrics>,
    target_id: krometrail_core::TargetId,
) -> Result<EffectiveViewport> {
    let value = runtime
        .pointer("/result/result/value")
        .or_else(|| runtime.pointer("/result/value"))
        .or_else(|| runtime.get("value"))
        .ok_or_else(|| malformed(target_id))?;
    let layout = layout.get("result").unwrap_or(layout);
    let visual_viewport = layout
        .get("cssVisualViewport")
        .ok_or_else(|| malformed(target_id))?;
    let width = number(visual_viewport, "clientWidth")?;
    let height = number(visual_viewport, "clientHeight")?;
    let scale = number(value, "scale")?;
    let touch_points = value
        .get("touchPoints")
        .and_then(Value::as_u64)
        .ok_or_else(|| malformed(target_id))?;
    let effective = EffectiveViewport {
        css_size: CssSize::new(width, height)?,
        device_scale_factor: DeviceScaleFactor::new(scale)?,
        mobile: declared.is_some_and(ViewportMetrics::mobile),
        touch: touch_points > 0,
        override_active: declared.is_some(),
    };
    if let Some(expected) = declared {
        let matches = (effective.css_size.width - f64::from(expected.width())).abs() <= 0.5
            && (effective.css_size.height - f64::from(expected.height())).abs() <= 0.5
            && (effective.device_scale_factor.get() - expected.device_scale_factor().get()).abs()
                <= 0.01
            && effective.touch == expected.touch();
        if !matches {
            return Err(operation_error(
                ErrorCode::TargetFailed,
                target_id,
                "browser did not apply the requested viewport metrics",
            ));
        }
    } else if effective.touch {
        return Err(operation_error(
            ErrorCode::TargetFailed,
            target_id,
            "browser did not clear touch emulation",
        ));
    }
    Ok(effective)
}

fn number(value: &Value, field: &str) -> Result<f64> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            krometrail_core::KrometrailError::new(
                ErrorCode::PageObservationFailed,
                krometrail_core::NonEmptyText::new(
                    "browser returned malformed effective viewport metrics",
                )
                .unwrap(),
            )
        })
}

fn malformed(target_id: krometrail_core::TargetId) -> krometrail_core::KrometrailError {
    operation_error(
        ErrorCode::PageObservationFailed,
        target_id,
        "browser returned malformed effective viewport metrics",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn target() -> krometrail_core::TargetId {
        krometrail_core::TargetId::from_uuid(Uuid::from_u128(1))
    }

    #[test]
    fn effective_metrics_decode_nested_cdp_result() {
        let declared = ViewportMetrics::new(390, 844, 3.0, true, true).unwrap();
        let value = decode_effective_viewport(
            &json!({"result":{"cssVisualViewport":{"clientWidth":390,"clientHeight":844}}}),
            &json!({"result":{"result":{"value":{"width":390,"height":844,"scale":3.0,"touchPoints":1}}}}),
            Some(declared),
            target(),
        )
        .unwrap();
        assert_eq!(value.css_size, CssSize::new(390.0, 844.0).unwrap());
        assert!(value.mobile && value.touch && value.override_active);
    }

    #[test]
    fn mismatched_acknowledged_metrics_fail() {
        let declared = ViewportMetrics::new(390, 844, 3.0, true, true).unwrap();
        assert!(decode_effective_viewport(
            &json!({"result":{"cssVisualViewport":{"clientWidth":391,"clientHeight":844}}}),
            &json!({"result":{"result":{"value":{"width":390,"height":844,"scale":3.0,"touchPoints":1}}}}),
            Some(declared),
            target(),
        )
        .is_err());
    }

    #[test]
    fn capture_geometry_rounds_fractional_css_dimensions_within_half_a_pixel() {
        let geometry = capture_geometry(EffectiveViewport {
            css_size: CssSize::new(599.6, 500.4).unwrap(),
            device_scale_factor: DeviceScaleFactor::new(2.0).unwrap(),
            mobile: false,
            touch: false,
            override_active: false,
        })
        .unwrap();
        assert_eq!(geometry.viewport, PixelDimensions::new(600, 500).unwrap());
        assert_eq!(geometry.device_scale_factor.get(), 2.0);
    }
}

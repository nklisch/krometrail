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
                "expression": "({scale:devicePixelRatio,touchPoints:navigator.maxTouchPoints,viewportMetaPresent:document.querySelector('meta[name=\"viewport\"]')!==null})",
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
    let css_size = if effective.override_active && !effective.mobile {
        // Desktop emulation controls the layout viewport. The visual viewport can be narrower
        // when Chrome reserves scrollbar space, so preserving it here would make the recorded
        // geometry contradict the acknowledged override.
        effective.layout_css_size
    } else {
        effective.css_size
    };
    Ok(CaptureGeometry {
        viewport: PixelDimensions::new(
            integral_css_dimension(css_size.width)?,
            integral_css_dimension(css_size.height)?,
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
    let layout_viewport = layout
        .get("cssLayoutViewport")
        .ok_or_else(|| malformed(target_id))?;
    let width = number(visual_viewport, "clientWidth")?;
    let height = number(visual_viewport, "clientHeight")?;
    let layout_width = number(layout_viewport, "clientWidth")?;
    let layout_height = number(layout_viewport, "clientHeight")?;
    let scale = number(value, "scale")?;
    let touch_points = value
        .get("touchPoints")
        .and_then(Value::as_u64)
        .ok_or_else(|| malformed(target_id))?;
    let viewport_meta_present = value
        .get("viewportMetaPresent")
        .and_then(Value::as_bool)
        .ok_or_else(|| malformed(target_id))?;
    let effective = EffectiveViewport {
        css_size: CssSize::new(width, height)?,
        layout_css_size: CssSize::new(layout_width, layout_height)?,
        device_scale_factor: DeviceScaleFactor::new(scale)?,
        mobile: declared.is_some_and(ViewportMetrics::mobile),
        touch: touch_points > 0,
        override_active: declared.is_some(),
        viewport_meta_present,
    };
    if let Some(expected) = declared {
        let matches = declared_geometry_matches(expected, &effective)
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

fn declared_geometry_matches(expected: ViewportMetrics, effective: &EffectiveViewport) -> bool {
    let observed = if expected.mobile() {
        effective.css_size
    } else {
        effective.layout_css_size
    };
    (observed.width - f64::from(expected.width())).abs() <= 0.5
        && (observed.height - f64::from(expected.height())).abs() <= 0.5
}

fn number(value: &Value, field: &str) -> Result<f64> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
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
            &json!({"result":{
                "cssVisualViewport":{"clientWidth":390,"clientHeight":844},
                "cssLayoutViewport":{"clientWidth":980,"clientHeight":2120}
            }}),
            &json!({"result":{"result":{"value":{"scale":3.0,"touchPoints":1,"viewportMetaPresent":false}}}}),
            Some(declared),
            target(),
        )
        .unwrap();
        assert_eq!(value.css_size, CssSize::new(390.0, 844.0).unwrap());
        assert_eq!(value.layout_css_size, CssSize::new(980.0, 2120.0).unwrap());
        assert!(!value.viewport_meta_present);
        assert!(value.mobile && value.touch && value.override_active);
    }

    #[test]
    fn mismatched_acknowledged_metrics_fail() {
        let declared = ViewportMetrics::new(390, 844, 3.0, true, true).unwrap();
        assert!(decode_effective_viewport(
            &json!({"result":{
                "cssVisualViewport":{"clientWidth":391,"clientHeight":844},
                "cssLayoutViewport":{"clientWidth":391,"clientHeight":844}
            }}),
            &json!({"result":{"result":{"value":{"scale":3.0,"touchPoints":1,"viewportMetaPresent":true}}}}),
            Some(declared),
            target(),
        )
        .is_err());
    }

    #[test]
    fn desktop_override_accepts_scrollbar_reduced_visual_viewport_and_captures_layout() {
        let declared = ViewportMetrics::new(390, 844, 1.0, false, false).unwrap();
        let effective = decode_effective_viewport(
            &json!({"result":{
                "cssVisualViewport":{"clientWidth":375,"clientHeight":844},
                "cssLayoutViewport":{"clientWidth":390,"clientHeight":844}
            }}),
            &json!({"result":{"result":{"value":{"scale":1.0,"touchPoints":0,"viewportMetaPresent":true}}}}),
            Some(declared),
            target(),
        )
        .expect("desktop emulation acknowledges its layout viewport despite a scrollbar");

        assert_eq!(effective.css_size, CssSize::new(375.0, 844.0).unwrap());
        assert_eq!(
            effective.layout_css_size,
            CssSize::new(390.0, 844.0).unwrap()
        );
        assert_eq!(
            capture_geometry(effective).unwrap().viewport,
            PixelDimensions::new(390, 844).unwrap()
        );
    }

    #[test]
    fn desktop_override_rejects_wrong_layout_viewport_despite_scrollbar_reduction() {
        let declared = ViewportMetrics::new(390, 844, 1.0, false, false).unwrap();
        assert!(decode_effective_viewport(
            &json!({"result":{
                "cssVisualViewport":{"clientWidth":375,"clientHeight":844},
                "cssLayoutViewport":{"clientWidth":391,"clientHeight":844}
            }}),
            &json!({"result":{"result":{"value":{"scale":1.0,"touchPoints":0,"viewportMetaPresent":true}}}}),
            Some(declared),
            target(),
        )
        .is_err());
    }

    #[test]
    fn clear_rejects_remaining_touch_emulation() {
        assert!(decode_effective_viewport(
            &json!({"result":{
                "cssVisualViewport":{"clientWidth":1440,"clientHeight":900},
                "cssLayoutViewport":{"clientWidth":1440,"clientHeight":900}
            }}),
            &json!({"result":{"result":{"value":{"scale":1.0,"touchPoints":1,"viewportMetaPresent":true}}}}),
            None,
            target(),
        )
        .is_err());
    }

    #[test]
    fn capture_geometry_rounds_fractional_css_dimensions_within_half_a_pixel() {
        let geometry = capture_geometry(EffectiveViewport {
            css_size: CssSize::new(599.6, 500.4).unwrap(),
            layout_css_size: CssSize::new(599.6, 500.4).unwrap(),
            device_scale_factor: DeviceScaleFactor::new(2.0).unwrap(),
            mobile: false,
            touch: false,
            override_active: false,
            viewport_meta_present: true,
        })
        .unwrap();
        assert_eq!(geometry.viewport, PixelDimensions::new(600, 500).unwrap());
        assert_eq!(geometry.device_scale_factor.get(), 2.0);
    }
}

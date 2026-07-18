use schemars::Schema;
use serde::{Deserialize, Serialize};

use crate::{
    CssSize, DeviceScaleFactor, NonEmptyText, ObservationPart, PageOperationResult, PageSelection,
    Result,
    error::invalid,
    validation::{delegate_json_schema, deserialize_validated},
};

pub const MAX_VIEWPORT_DIMENSION: u32 = 10_000;
pub const MAX_VIEWPORT_DEVICE_SCALE: f64 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ViewportMetrics {
    width: u32,
    height: u32,
    device_scale_factor: DeviceScaleFactor,
    mobile: bool,
    touch: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ViewportMetricsWire {
    #[schemars(range(min = 1, max = 10_000))]
    width: u32,
    #[schemars(range(min = 1, max = 10_000))]
    height: u32,
    #[schemars(range(max = 8.0), transform = require_positive_number)]
    device_scale_factor: f64,
    mobile: bool,
    touch: bool,
}

fn require_positive_number(schema: &mut Schema) {
    schema.insert("exclusiveMinimum".to_owned(), 0.0.into());
}

impl ViewportMetrics {
    pub fn new(
        width: u32,
        height: u32,
        device_scale_factor: f64,
        mobile: bool,
        touch: bool,
    ) -> Result<Self> {
        if !(1..=MAX_VIEWPORT_DIMENSION).contains(&width)
            || !(1..=MAX_VIEWPORT_DIMENSION).contains(&height)
        {
            return Err(invalid(
                "viewport width and height must be between 1 and 10000 CSS pixels",
            ));
        }
        if device_scale_factor > MAX_VIEWPORT_DEVICE_SCALE {
            return Err(invalid("viewport device scale factor must not exceed 8"));
        }
        Ok(Self {
            width,
            height,
            device_scale_factor: DeviceScaleFactor::new(device_scale_factor)?,
            mobile,
            touch,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn device_scale_factor(self) -> DeviceScaleFactor {
        self.device_scale_factor
    }

    pub const fn mobile(self) -> bool {
        self.mobile
    }

    pub const fn touch(self) -> bool {
        self.touch
    }
}

impl<'de> Deserialize<'de> for ViewportMetrics {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: ViewportMetricsWire| {
            Self::new(
                wire.width,
                wire.height,
                wire.device_scale_factor,
                wire.mobile,
                wire.touch,
            )
        })
    }
}

delegate_json_schema!(ViewportMetrics => ViewportMetricsWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportPreset {
    ResponsiveSmall,
    ResponsiveTablet,
    ResponsiveDesktop,
    MobilePhone,
    MobileTablet,
}

impl ViewportPreset {
    pub fn materialize(self) -> ViewportMetrics {
        let (width, height, device_scale_factor, mobile, touch) = match self {
            Self::ResponsiveSmall => (390, 844, 1.0, false, false),
            Self::ResponsiveTablet => (768, 1_024, 1.0, false, false),
            Self::ResponsiveDesktop => (1_440, 900, 1.0, false, false),
            Self::MobilePhone => (390, 844, 3.0, true, true),
            Self::MobileTablet => (768, 1_024, 2.0, true, true),
        };
        ViewportMetrics::new(width, height, device_scale_factor, mobile, touch)
            .expect("built-in viewport preset must contain valid metrics")
    }

    pub const fn intent(self) -> ViewportIntent {
        match self {
            Self::ResponsiveSmall | Self::ResponsiveTablet | Self::ResponsiveDesktop => {
                ViewportIntent::ResponsiveCss
            }
            Self::MobilePhone | Self::MobileTablet => ViewportIntent::MobileDevice,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportIntent {
    BrowserDefault,
    Custom,
    ResponsiveCss,
    MobileDevice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ViewportOverride {
    Override { metrics: ViewportMetrics },
    Preset { preset: ViewportPreset },
    Clear,
}

#[derive(schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename = "ViewportOverride")]
#[allow(dead_code)]
enum ViewportOverrideWire {
    Override { metrics: ViewportMetrics },
    Preset { preset: ViewportPreset },
    Clear,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ViewportOverrideDecode {
    Override(ViewportOverrideMetricsWire),
    Preset(ViewportOverridePresetWire),
    Clear(ViewportOverrideClearWire),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewportOverrideMetricsWire {
    mode: OverrideMode,
    metrics: ViewportMetrics,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum OverrideMode {
    Override,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewportOverridePresetWire {
    mode: PresetMode,
    preset: ViewportPreset,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PresetMode {
    Preset,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewportOverrideClearWire {
    mode: ClearMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClearMode {
    Clear,
}

impl<'de> Deserialize<'de> for ViewportOverride {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Ok(match ViewportOverrideDecode::deserialize(deserializer)? {
            ViewportOverrideDecode::Override(wire) => {
                let OverrideMode::Override = wire.mode;
                Self::Override {
                    metrics: wire.metrics,
                }
            }
            ViewportOverrideDecode::Preset(wire) => {
                let PresetMode::Preset = wire.mode;
                Self::Preset {
                    preset: wire.preset,
                }
            }
            ViewportOverrideDecode::Clear(wire) => {
                let ClearMode::Clear = wire.mode;
                Self::Clear
            }
        })
    }
}

delegate_json_schema!(ViewportOverride => ViewportOverrideWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ViewportMaterialization {
    pub intent: ViewportIntent,
    pub preset: Option<ViewportPreset>,
    pub metrics: Option<ViewportMetrics>,
    pub user_agent_emulated: bool,
}

impl ViewportOverride {
    pub fn materialize(self) -> ViewportMaterialization {
        match self {
            Self::Override { metrics } => ViewportMaterialization {
                intent: ViewportIntent::Custom,
                preset: None,
                metrics: Some(metrics),
                user_agent_emulated: false,
            },
            Self::Preset { preset } => ViewportMaterialization {
                intent: preset.intent(),
                preset: Some(preset),
                metrics: Some(preset.materialize()),
                user_agent_emulated: false,
            },
            Self::Clear => ViewportMaterialization {
                intent: ViewportIntent::BrowserDefault,
                preset: None,
                metrics: None,
                user_agent_emulated: false,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetViewportRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub viewport: ViewportOverride,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EffectiveViewport {
    pub css_size: CssSize,
    pub layout_css_size: CssSize,
    pub device_scale_factor: DeviceScaleFactor,
    pub mobile: bool,
    pub touch: bool,
    pub override_active: bool,
    pub viewport_meta_present: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportGuidanceCode {
    LayoutViewportMismatch,
    LikelyMissingViewportMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ViewportGuidance {
    pub code: ViewportGuidanceCode,
    pub message: NonEmptyText,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportOperationResult {
    pub operation: PageOperationResult,
    pub effective: ObservationPart<EffectiveViewport>,
    pub materialization: ViewportMaterialization,
    pub guidance: Vec<ViewportGuidance>,
}

pub fn viewport_guidance(
    materialization: ViewportMaterialization,
    effective: &EffectiveViewport,
) -> Vec<ViewportGuidance> {
    if materialization.metrics.is_none() {
        return Vec::new();
    }
    let visual = effective.css_size;
    let layout = effective.layout_css_size;
    let mismatched = dimension_mismatched(layout.width, visual.width)
        || dimension_mismatched(layout.height, visual.height);
    if !mismatched {
        return Vec::new();
    }

    let likely_missing_metadata = materialization.intent == ViewportIntent::MobileDevice
        && !effective.viewport_meta_present
        && layout.width >= visual.width * 1.5;
    let (code, explanation) = if likely_missing_metadata {
        (
            ViewportGuidanceCode::LikelyMissingViewportMetadata,
            "The page likely lacks viewport metadata; add it for mobile-device layout behavior or use a responsive preset for CSS-breakpoint testing.",
        )
    } else {
        (
            ViewportGuidanceCode::LayoutViewportMismatch,
            "The page layout viewport differs from the acknowledged visual viewport; inspect page responsive-layout behavior.",
        )
    };
    let message = format!(
        "Viewport override was acknowledged at {}×{} CSS px, while the observed layout viewport is {}×{} CSS px. {explanation}",
        visual.width, visual.height, layout.width, layout.height
    );
    vec![ViewportGuidance {
        code,
        message: NonEmptyText::new(message).expect("viewport guidance is non-empty"),
    }]
}

fn dimension_mismatched(layout: f64, visual: f64) -> bool {
    (layout - visual).abs() > (visual * 0.05).max(8.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_reject_invalid_dimensions_and_scale() {
        assert!(ViewportMetrics::new(0, 1, 1.0, false, false).is_err());
        assert!(ViewportMetrics::new(1, MAX_VIEWPORT_DIMENSION + 1, 1.0, false, false).is_err());
        assert!(ViewportMetrics::new(1, 1, f64::NAN, false, false).is_err());
        assert!(ViewportMetrics::new(1, 1, MAX_VIEWPORT_DEVICE_SCALE + 0.1, false, false).is_err());
    }

    #[test]
    fn request_defaults_to_selected_page() {
        let request: SetViewportRequest = serde_json::from_str(
            r#"{"viewport":{"mode":"override","metrics":{"width":390,"height":844,"device_scale_factor":3.0,"mobile":true,"touch":true}}}"#,
        )
        .unwrap();
        assert_eq!(request.target, PageSelection::Selected);
    }

    #[test]
    fn stable_override_and_clear_wire_shapes_round_trip_exactly() {
        for json in [
            r#"{"mode":"override","metrics":{"width":390,"height":844,"device_scale_factor":3.0,"mobile":true,"touch":true}}"#,
            r#"{"mode":"clear"}"#,
        ] {
            let viewport: ViewportOverride = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&viewport).unwrap(), json);
        }
    }

    #[test]
    fn presets_materialize_to_exact_metrics_and_intent() {
        let cases = [
            (
                ViewportPreset::ResponsiveSmall,
                390,
                844,
                1.0,
                false,
                false,
                ViewportIntent::ResponsiveCss,
            ),
            (
                ViewportPreset::ResponsiveTablet,
                768,
                1_024,
                1.0,
                false,
                false,
                ViewportIntent::ResponsiveCss,
            ),
            (
                ViewportPreset::ResponsiveDesktop,
                1_440,
                900,
                1.0,
                false,
                false,
                ViewportIntent::ResponsiveCss,
            ),
            (
                ViewportPreset::MobilePhone,
                390,
                844,
                3.0,
                true,
                true,
                ViewportIntent::MobileDevice,
            ),
            (
                ViewportPreset::MobileTablet,
                768,
                1_024,
                2.0,
                true,
                true,
                ViewportIntent::MobileDevice,
            ),
        ];
        for (preset, width, height, scale, mobile, touch, intent) in cases {
            let materialization = ViewportOverride::Preset { preset }.materialize();
            let metrics = materialization.metrics.unwrap();
            assert_eq!(materialization.intent, intent);
            assert_eq!(materialization.preset, Some(preset));
            assert_eq!(metrics.width(), width);
            assert_eq!(metrics.height(), height);
            assert_eq!(metrics.device_scale_factor().get(), scale);
            assert_eq!(metrics.mobile(), mobile);
            assert_eq!(metrics.touch(), touch);
            assert!(!materialization.user_agent_emulated);
        }
    }

    #[test]
    fn custom_and_clear_materialization_preserve_truthful_provenance() {
        let metrics = ViewportMetrics::new(512, 768, 1.25, false, true).unwrap();
        assert_eq!(
            ViewportOverride::Override { metrics }.materialize(),
            ViewportMaterialization {
                intent: ViewportIntent::Custom,
                preset: None,
                metrics: Some(metrics),
                user_agent_emulated: false,
            }
        );
        assert_eq!(
            ViewportOverride::Clear.materialize(),
            ViewportMaterialization {
                intent: ViewportIntent::BrowserDefault,
                preset: None,
                metrics: None,
                user_agent_emulated: false,
            }
        );
    }

    #[test]
    fn preset_wire_shape_is_additive_and_rejects_mixed_fields() {
        let json = r#"{"mode":"preset","preset":"responsive_small"}"#;
        let viewport: ViewportOverride = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&viewport).unwrap(), json);
        assert!(serde_json::from_str::<ViewportOverride>(
            r#"{"mode":"preset","preset":"responsive_small","metrics":{"width":390,"height":844,"device_scale_factor":1.0,"mobile":false,"touch":false}}"#
        ).is_err());
        assert!(
            serde_json::from_str::<ViewportOverride>(
                r#"{"mode":"clear","preset":"responsive_small"}"#
            )
            .is_err()
        );
    }

    fn effective(layout_width: f64, viewport_meta_present: bool) -> EffectiveViewport {
        EffectiveViewport {
            css_size: CssSize::new(100.0, 100.0).unwrap(),
            layout_css_size: CssSize::new(layout_width, 100.0).unwrap(),
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
            mobile: false,
            touch: false,
            override_active: true,
            viewport_meta_present,
        }
    }

    #[test]
    fn layout_mismatch_threshold_is_strictly_above_eight_pixels_or_five_percent() {
        let custom = ViewportOverride::Override {
            metrics: ViewportMetrics::new(100, 100, 1.0, false, false).unwrap(),
        }
        .materialize();
        assert!(viewport_guidance(custom, &effective(107.9, true)).is_empty());
        assert!(viewport_guidance(custom, &effective(108.0, true)).is_empty());
        assert_eq!(
            viewport_guidance(custom, &effective(108.1, true))[0].code,
            ViewportGuidanceCode::LayoutViewportMismatch
        );

        let large_visual = EffectiveViewport {
            css_size: CssSize::new(1_000.0, 100.0).unwrap(),
            layout_css_size: CssSize::new(1_050.0, 100.0).unwrap(),
            ..effective(100.0, true)
        };
        assert!(viewport_guidance(custom, &large_visual).is_empty());
        assert_eq!(
            viewport_guidance(
                custom,
                &EffectiveViewport {
                    layout_css_size: CssSize::new(1_050.1, 100.0).unwrap(),
                    ..large_visual
                }
            )[0]
            .code,
            ViewportGuidanceCode::LayoutViewportMismatch
        );
    }

    #[test]
    fn missing_metadata_guidance_requires_mobile_no_meta_and_width_ratio() {
        let mobile = ViewportOverride::Preset {
            preset: ViewportPreset::MobilePhone,
        }
        .materialize();
        let responsive = ViewportOverride::Preset {
            preset: ViewportPreset::ResponsiveSmall,
        }
        .materialize();
        assert_eq!(
            viewport_guidance(mobile, &effective(150.0, false))[0].code,
            ViewportGuidanceCode::LikelyMissingViewportMetadata
        );
        assert_eq!(
            viewport_guidance(mobile, &effective(150.0, true))[0].code,
            ViewportGuidanceCode::LayoutViewportMismatch
        );
        assert_eq!(
            viewport_guidance(responsive, &effective(150.0, false))[0].code,
            ViewportGuidanceCode::LayoutViewportMismatch
        );
        assert_eq!(
            viewport_guidance(mobile, &effective(149.9, false))[0].code,
            ViewportGuidanceCode::LayoutViewportMismatch
        );
    }

    #[test]
    fn clear_never_describes_divergent_browser_default_geometry_as_acknowledged_override() {
        let browser_default = EffectiveViewport {
            override_active: false,
            ..effective(150.0, false)
        };
        assert!(
            viewport_guidance(ViewportOverride::Clear.materialize(), &browser_default).is_empty()
        );
    }

    #[test]
    fn clear_has_no_unused_metrics_payload() {
        assert!(
            serde_json::from_str::<SetViewportRequest>(
                r#"{"viewport":{"mode":"clear","metrics":{"width":1}}}"#
            )
            .is_err()
        );
    }
}

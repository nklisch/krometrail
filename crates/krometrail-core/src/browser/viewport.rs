use schemars::Schema;
use serde::{Deserialize, Serialize};

use crate::{
    CssSize, DeviceScaleFactor, ObservationPart, PageOperationResult, PageSelection, Result,
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
#[serde(tag = "mode", content = "metrics", rename_all = "snake_case")]
pub enum ViewportOverride {
    Override(ViewportMetrics),
    Clear,
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
    pub device_scale_factor: DeviceScaleFactor,
    pub mobile: bool,
    pub touch: bool,
    pub override_active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportOperationResult {
    pub operation: PageOperationResult,
    pub effective: ObservationPart<EffectiveViewport>,
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
    fn clear_has_no_unused_metrics_payload() {
        assert!(
            serde_json::from_str::<SetViewportRequest>(
                r#"{"viewport":{"mode":"clear","metrics":{"width":1}}}"#
            )
            .is_err()
        );
    }
}

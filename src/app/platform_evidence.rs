//! Shared test-only platform evidence runner.
//!
//! This module is deliberately a thin lane adapter. It owns lane selection and the two-gate
//! boundary, then delegates the complete run to `live_evaluation::run_live_qualification` so
//! browser, fixture, lock, store, artifact, cleanup, and manifest authorities cannot fork.
#![allow(dead_code)]

use std::path::PathBuf;

use krometrail_core::{DiskBudgetBytes, ErrorCode, KrometrailError, NonEmptyText, Result};
use temporal_evaluation::{
    PlatformLaneDeclaration, PlatformLaneId, PlatformProfile, validate_platform_lane,
};

use super::live_evaluation::{
    LiveQualificationConfig, OptInDecision, run_live_qualification,
    run_live_qualification_with_decision,
};

/// Test-only configuration for one canonical platform lane. Browser product, profile, viewport,
/// requested scale, optionality, and claim role all come from the temporal-evaluation registry.
#[derive(Clone, Debug)]
pub struct PlatformLaneConfig {
    pub lane: PlatformLaneId,
    pub output_root: PathBuf,
    pub run_id: String,
    pub retention_budget: DiskBudgetBytes,
}

impl PlatformLaneConfig {
    pub fn new(lane: PlatformLaneId) -> Self {
        let output_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/temporal-evaluation/live");
        Self {
            lane,
            output_root,
            run_id: format!("{}-{}", lane.as_str(), std::process::id()),
            retention_budget: DiskBudgetBytes::default(),
        }
    }

    pub fn declaration(&self) -> PlatformLaneDeclaration {
        PlatformLaneDeclaration::canonical(self.lane)
    }

    pub fn profile(&self) -> PlatformProfile {
        self.lane.definition().profile
    }

    fn validate(&self) -> Result<()> {
        let definition = self.lane.definition();
        definition.validate().map_err(contract_error)?;
        let host_platform = if cfg!(target_os = "linux") {
            temporal_evaluation::Platform::Linux
        } else if cfg!(target_os = "macos") {
            temporal_evaluation::Platform::Macos
        } else {
            temporal_evaluation::Platform::Other
        };
        if definition.platform != host_platform {
            return Err(invalid("platform lane does not match the current host"));
        }
        if self.run_id.is_empty()
            || self.run_id == "."
            || self.run_id == ".."
            || self.run_id.contains("..")
            || self.run_id.contains('/')
            || self.run_id.contains('\\')
            || !self
                .run_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(invalid("platform run id is not a safe output component"));
        }
        let ignored_boundary =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/temporal-evaluation/live");
        if !self.output_root.is_absolute() || !self.output_root.starts_with(ignored_boundary) {
            return Err(invalid(
                "platform output root must remain below the ignored qualification boundary",
            ));
        }
        Ok(())
    }

    fn live_config(&self) -> LiveQualificationConfig {
        let definition = self.lane.definition();
        LiveQualificationConfig {
            output_root: self.output_root.clone(),
            browser_product: match definition.browser_product {
                temporal_evaluation::BrowserProduct::Chrome => {
                    krometrail_core::BrowserProduct::Chrome
                }
                temporal_evaluation::BrowserProduct::Chromium => {
                    krometrail_core::BrowserProduct::Chromium
                }
                temporal_evaluation::BrowserProduct::OtherChromium => {
                    krometrail_core::BrowserProduct::OtherChromium
                }
            },
            run_id: self.run_id.clone(),
            optional_browser: !definition.required,
            retention_budget: self.retention_budget,
            platform: Some(self.declaration()),
        }
    }
}

/// Run the selected lane using the process's explicit operator gates.
pub async fn run_platform_lane(
    config: PlatformLaneConfig,
) -> Result<temporal_evaluation::RunManifest> {
    if OptInDecision::from_environment() != OptInDecision::Authorized {
        return Err(invalid(
            "platform evidence requires both explicit opt-in gates",
        ));
    }
    config.validate()?;
    let manifest = run_live_qualification(config.live_config()).await?;
    validate_platform_lane(config.lane, &manifest).map_err(contract_error)?;
    Ok(manifest)
}

/// Injected decision seam used by deterministic tests. It returns before validation, discovery,
/// fixture-server startup, profile creation, store opening, or output preparation when either
/// authorization gate is absent.
pub async fn run_platform_lane_with_decision(
    config: PlatformLaneConfig,
    decision: OptInDecision,
) -> Result<temporal_evaluation::RunManifest> {
    if decision != OptInDecision::Authorized {
        return Err(invalid(
            "platform evidence requires both explicit opt-in gates",
        ));
    }
    config.validate()?;
    let manifest = run_live_qualification_with_decision(config.live_config(), decision).await?;
    validate_platform_lane(config.lane, &manifest).map_err(contract_error)?;
    Ok(manifest)
}

fn invalid(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidLifecycleTransition,
        NonEmptyText::new(message).expect("static platform runner error"),
    )
}

fn contract_error(error: temporal_evaluation::ContractError) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(error.to_string()).expect("contract errors are non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_config_derives_all_runtime_identity_from_the_registry() {
        let config = PlatformLaneConfig::new(PlatformLaneId::MacosChromeHighDpi);
        let definition = PlatformLaneId::MacosChromeHighDpi.definition();
        assert_eq!(config.declaration(), definition.declaration());
        assert_eq!(config.profile(), PlatformProfile::HighDpi);
        let live = config.live_config();
        assert_eq!(live.platform, Some(config.declaration()));
        assert_eq!(
            live.browser_product,
            krometrail_core::BrowserProduct::Chrome
        );
        assert_eq!(live.optional_browser, !definition.required);
        assert_eq!(live.viewport().device_scale_factor_milli, 2_000);
        assert_eq!(live.wrapper_variant().as_str(), "high_dpi");
    }

    #[tokio::test]
    async fn disabled_platform_runner_has_no_output_side_effect() {
        let root = std::env::temp_dir().join(format!(
            "krometrail-platform-disabled-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = PlatformLaneConfig::new(PlatformLaneId::LinuxStableChromeReferenceHost);
        config.output_root = root.clone();
        let error = run_platform_lane_with_decision(config, OptInDecision::Disabled)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidLifecycleTransition);
        assert!(!root.exists());
    }
}

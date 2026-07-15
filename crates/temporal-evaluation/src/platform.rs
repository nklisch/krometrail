use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    BrowserAvailability, BrowserProduct, ContractError, Platform, Result, VIEWPORT_HEIGHT,
    VIEWPORT_WIDTH, Viewport,
};

/// The manifest profile used by every platform evidence lane.
pub const PLATFORM_EVIDENCE_PROFILE: &str = "platform-evidence-v1";

/// Non-claims shared by every platform lane. Exact measurements remain in the referenced
/// `RunManifest`; this short list prevents a lane from being mistaken for a cross-platform or
/// model-evaluation result.
pub const PLATFORM_NON_CLAIMS: [&str; 8] = [
    "No model call or model data was used.",
    "No remote or paid service was used.",
    "This run makes no model-effectiveness conclusion.",
    "This run makes no temporal-advantage uplift conclusion.",
    "This run makes no production-scale stability claim.",
    "This run qualifies only the declared platform lane and observed capture configuration.",
    "This run makes no cross-platform conclusion unless every required platform lane passes.",
    "A wrapper flag or requested scale is not DPI evidence; only observed capture metadata counts.",
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformLaneId {
    LinuxStableChromeReferenceHost,
    MacosChromeDefaultDpi,
    MacosChromeHighDpi,
    LinuxChromiumOptional,
}

impl PlatformLaneId {
    /// Canonical publication and execution order. Do not sort lanes by host or product.
    pub const ALL: [Self; 4] = [
        Self::LinuxStableChromeReferenceHost,
        Self::MacosChromeDefaultDpi,
        Self::MacosChromeHighDpi,
        Self::LinuxChromiumOptional,
    ];

    pub const REQUIRED: [Self; 3] = [
        Self::LinuxStableChromeReferenceHost,
        Self::MacosChromeDefaultDpi,
        Self::MacosChromeHighDpi,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxStableChromeReferenceHost => "linux_stable_chrome_reference_host",
            Self::MacosChromeDefaultDpi => "macos_chrome_default_dpi",
            Self::MacosChromeHighDpi => "macos_chrome_high_dpi",
            Self::LinuxChromiumOptional => "linux_chromium_optional",
        }
    }

    pub const fn is_required(self) -> bool {
        matches!(
            self,
            Self::LinuxStableChromeReferenceHost
                | Self::MacosChromeDefaultDpi
                | Self::MacosChromeHighDpi
        )
    }

    pub const fn definition(self) -> PlatformLaneDefinition {
        let (platform, browser_product, profile, requested, minimum, maximum, reference_host, role) =
            match self {
                Self::LinuxStableChromeReferenceHost => (
                    Platform::Linux,
                    BrowserProduct::Chrome,
                    PlatformProfile::DefaultDpi,
                    1_000,
                    1_000,
                    Some(1_000),
                    true,
                    PlatformClaimRole::ReferenceHost,
                ),
                Self::MacosChromeDefaultDpi => (
                    Platform::Macos,
                    BrowserProduct::Chrome,
                    PlatformProfile::DefaultDpi,
                    1_000,
                    1_000,
                    Some(1_000),
                    false,
                    PlatformClaimRole::RequiredCrossPlatform,
                ),
                Self::MacosChromeHighDpi => (
                    Platform::Macos,
                    BrowserProduct::Chrome,
                    PlatformProfile::HighDpi,
                    2_000,
                    1_500,
                    None,
                    false,
                    PlatformClaimRole::RequiredCrossPlatform,
                ),
                Self::LinuxChromiumOptional => (
                    Platform::Linux,
                    BrowserProduct::Chromium,
                    PlatformProfile::DefaultDpi,
                    1_000,
                    1_000,
                    Some(1_000),
                    false,
                    PlatformClaimRole::OptionalSupplemental,
                ),
            };
        PlatformLaneDefinition {
            lane: self,
            required: self.is_required(),
            platform,
            browser_product,
            profile,
            viewport: Viewport {
                width: VIEWPORT_WIDTH,
                height: VIEWPORT_HEIGHT,
            },
            requested_device_scale_factor: requested,
            minimum_observed_device_scale_factor: minimum,
            maximum_observed_device_scale_factor: maximum,
            reference_host,
            claim_role: role,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformProfile {
    DefaultDpi,
    HighDpi,
}

impl PlatformProfile {
    pub const ALL: [Self; 2] = [Self::DefaultDpi, Self::HighDpi];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DefaultDpi => "default_dpi",
            Self::HighDpi => "high_dpi",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformClaimRole {
    ReferenceHost,
    RequiredCrossPlatform,
    OptionalSupplemental,
}

impl PlatformClaimRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReferenceHost => "reference_host",
            Self::RequiredCrossPlatform => "required_cross_platform",
            Self::OptionalSupplemental => "optional_supplemental",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformLaneDefinition {
    pub lane: PlatformLaneId,
    pub required: bool,
    pub platform: Platform,
    pub browser_product: BrowserProduct,
    pub profile: PlatformProfile,
    pub viewport: Viewport,
    /// Requested scale is configuration identity only. Validation uses observed capture metadata.
    pub requested_device_scale_factor: u16,
    pub minimum_observed_device_scale_factor: u16,
    pub maximum_observed_device_scale_factor: Option<u16>,
    pub reference_host: bool,
    pub claim_role: PlatformClaimRole,
}

impl PlatformLaneDefinition {
    pub fn declaration(&self) -> PlatformLaneDeclaration {
        PlatformLaneDeclaration {
            lane: self.lane,
            profile: self.profile,
            platform: self.platform,
            browser_product: self.browser_product,
            viewport: self.viewport,
            declared_device_scale_factor: self.requested_device_scale_factor,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self != &self.lane.definition() {
            return Err(ContractError::new(
                "platform lane definition does not match the canonical registry",
            ));
        }
        if self.required != PlatformLaneId::REQUIRED.contains(&self.lane)
            || (self.reference_host != matches!(self.claim_role, PlatformClaimRole::ReferenceHost))
            || self.minimum_observed_device_scale_factor == 0
            || self
                .maximum_observed_device_scale_factor
                .is_some_and(|maximum| maximum < self.minimum_observed_device_scale_factor)
        {
            return Err(ContractError::new(
                "platform lane definition has inconsistent status or scale bounds",
            ));
        }
        Ok(())
    }
}

/// The declaration embedded in a platform run manifest. It contains requested identity only;
/// observed scale and browser identity remain in the existing qualification measurements.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformLaneDeclaration {
    pub lane: PlatformLaneId,
    pub profile: PlatformProfile,
    pub platform: Platform,
    pub browser_product: BrowserProduct,
    pub viewport: Viewport,
    pub declared_device_scale_factor: u16,
}

/// Short alias for callers that describe the field as a platform declaration.
pub type PlatformDeclaration = PlatformLaneDeclaration;

impl PlatformLaneDeclaration {
    pub fn canonical(lane: PlatformLaneId) -> Self {
        lane.definition().declaration()
    }

    pub fn validate(&self) -> Result<()> {
        if self != &Self::canonical(self.lane) {
            return Err(ContractError::new(
                "platform lane declaration does not match the canonical registry",
            ));
        }
        Ok(())
    }
}

/// Validate one existing live manifest against the one lane registry. This is intentionally a
/// read-only check: it never reads files, launches a browser, or treats wrapper intent as proof.
pub fn validate_platform_lane(lane: PlatformLaneId, manifest: &crate::RunManifest) -> Result<()> {
    manifest.validate()?;
    let definition = lane.definition();
    definition.validate()?;
    let declaration = manifest.platform.as_ref().ok_or_else(|| {
        ContractError::new("platform evidence manifest must declare its lane and profile")
    })?;
    if declaration != &definition.declaration() {
        return Err(ContractError::new(
            "platform manifest declaration does not match its lane",
        ));
    }
    if manifest.run.threshold_profile != PLATFORM_EVIDENCE_PROFILE
        || manifest.run.optional_configuration == definition.required
        || manifest.run.viewport != definition.viewport
        || manifest.run.device_scale_factor != definition.requested_device_scale_factor
        || manifest.environment.platform != definition.platform
    {
        return Err(ContractError::new(
            "platform manifest configuration does not match its lane",
        ));
    }

    match &manifest.browser {
        BrowserAvailability::Observed { product, .. }
        | BrowserAvailability::Skipped { product, .. }
            if *product != definition.browser_product =>
        {
            return Err(ContractError::new(
                "platform manifest browser product does not match its lane",
            ));
        }
        BrowserAvailability::Observed { .. }
        | BrowserAvailability::Skipped { .. }
        | BrowserAvailability::NotRequired
        | BrowserAvailability::Unavailable { .. }
        | BrowserAvailability::Blocked { .. } => {}
    }

    let qualification = manifest.qualification.as_ref().ok_or_else(|| {
        ContractError::new("platform evidence manifest must contain live qualification")
    })?;
    if qualification.profile != PLATFORM_EVIDENCE_PROFILE {
        return Err(ContractError::new(
            "platform manifest qualification profile is not registered",
        ));
    }
    let observed_viewport = qualification.capture.observed_viewport;
    let observed_scale = qualification.capture.observed_device_scale_factor;
    let has_observation =
        observed_viewport.width != 0 && observed_viewport.height != 0 && observed_scale != 0;
    if !has_observation {
        if matches!(
            manifest.status,
            crate::EvaluationStatus::Blocked
                | crate::EvaluationStatus::Inconclusive
                | crate::EvaluationStatus::Skipped
        ) {
            return Ok(());
        }
        return Err(ContractError::new(
            "platform manifest lacks decisive observed viewport and scale metadata",
        ));
    }
    if observed_viewport != definition.viewport
        || observed_scale < definition.minimum_observed_device_scale_factor
        || definition
            .maximum_observed_device_scale_factor
            .is_some_and(|maximum| observed_scale > maximum)
    {
        return Err(ContractError::new(
            "platform manifest observed viewport or scale is outside its lane",
        ));
    }
    if manifest.non_claims
        != PLATFORM_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_owned())
            .collect::<Vec<_>>()
    {
        return Err(ContractError::new(
            "platform manifest non-claims do not match the canonical platform contract",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_exact_canonical_order_and_required_subset() {
        assert_eq!(PlatformLaneId::ALL.len(), 4);
        assert_eq!(PlatformLaneId::REQUIRED.len(), 3);
        assert_eq!(
            PlatformLaneId::ALL,
            [
                PlatformLaneId::LinuxStableChromeReferenceHost,
                PlatformLaneId::MacosChromeDefaultDpi,
                PlatformLaneId::MacosChromeHighDpi,
                PlatformLaneId::LinuxChromiumOptional,
            ]
        );
        assert!(
            PlatformLaneId::ALL[..3]
                .iter()
                .all(|lane| PlatformLaneId::REQUIRED.contains(lane))
        );
        assert!(!PlatformLaneId::REQUIRED.contains(&PlatformLaneId::LinuxChromiumOptional));
    }

    #[test]
    fn definitions_are_registry_owned_and_high_dpi_is_observation_only() {
        let high = PlatformLaneId::MacosChromeHighDpi.definition();
        assert_eq!(high.requested_device_scale_factor, 2_000);
        assert_eq!(high.minimum_observed_device_scale_factor, 1_500);
        assert_eq!(high.maximum_observed_device_scale_factor, None);
        assert_eq!(high.profile, PlatformProfile::HighDpi);
        assert!(!high.reference_host);
        assert_eq!(
            PlatformLaneId::LinuxStableChromeReferenceHost
                .definition()
                .claim_role,
            PlatformClaimRole::ReferenceHost
        );
    }
}

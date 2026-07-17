use serde::{Deserialize, Serialize};

use crate::{
    error::{KrometrailError, NonEmptyText, Result, invalid},
    lifecycle::TargetLifecycle,
    validation::deserialize_validated,
};

use super::{BrowserVersion, PageTarget};
use crate::recording::{CaptureGap, TargetCaptureStatus};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserSessionState {
    Connecting,
    Ready,
    Reconnecting,
    Stopping,
    Ended,
}

impl BrowserSessionState {
    pub const ALL: &'static [Self] = &[
        Self::Connecting,
        Self::Ready,
        Self::Reconnecting,
        Self::Stopping,
        Self::Ended,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Reconnecting => "reconnecting",
            Self::Stopping => "stopping",
            Self::Ended => "ended",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserOwnership {
    Managed,
    Attached,
}

impl BrowserOwnership {
    pub const ALL: &'static [Self] = &[Self::Managed, Self::Attached];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Attached => "attached",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetVisibility {
    Unknown,
    Visible,
    Hidden,
}

impl TargetVisibility {
    pub const ALL: &'static [Self] = &[Self::Unknown, Self::Visible, Self::Hidden];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Visible => "visible",
            Self::Hidden => "hidden",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserStopOutcome {
    ManagedBrowserClosed,
    ManagedBrowserClosedDegraded,
    Detached,
}

impl BrowserStopOutcome {
    pub const ALL: &'static [Self] = &[
        Self::ManagedBrowserClosed,
        Self::ManagedBrowserClosedDegraded,
        Self::Detached,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManagedBrowserClosed => "managed_browser_closed",
            Self::ManagedBrowserClosedDegraded => "managed_browser_closed_degraded",
            Self::Detached => "detached",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererCapability {
    BrowserIdentity,
    TargetDiscovery,
    FlatTargetSessions,
    Page,
    Runtime,
    Accessibility,
    Input,
    Screencast,
}

impl RendererCapability {
    pub const ALL: &'static [Self] = &[
        Self::BrowserIdentity,
        Self::TargetDiscovery,
        Self::FlatTargetSessions,
        Self::Page,
        Self::Runtime,
        Self::Accessibility,
        Self::Input,
        Self::Screencast,
    ];

    pub const REQUIRED: &'static [Self] = Self::ALL;

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BrowserIdentity => "browser_identity",
            Self::TargetDiscovery => "target_discovery",
            Self::FlatTargetSessions => "flat_target_sessions",
            Self::Page => "page",
            Self::Runtime => "runtime",
            Self::Accessibility => "accessibility",
            Self::Input => "input",
            Self::Screencast => "screencast",
        }
    }

    pub const fn is_required(self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySupport {
    pub capability: RendererCapability,
    pub available: bool,
    pub required: bool,
    pub detail: Option<NonEmptyText>,
}

impl CapabilitySupport {
    pub fn new(
        capability: RendererCapability,
        available: bool,
        required: bool,
        detail: Option<NonEmptyText>,
    ) -> Result<Self> {
        if available && detail.is_some() {
            // Details are useful for both outcomes, so this is intentionally allowed.
        }
        Ok(Self {
            capability,
            available,
            required,
            detail,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserCompatibility {
    pub version: BrowserVersion,
    pub capabilities: Vec<CapabilitySupport>,
}

#[derive(Deserialize)]
struct BrowserCompatibilityWire {
    version: BrowserVersion,
    capabilities: Vec<CapabilitySupport>,
}

impl BrowserCompatibility {
    pub fn new(version: BrowserVersion, capabilities: Vec<CapabilitySupport>) -> Result<Self> {
        let compatibility = Self {
            version,
            capabilities,
        };
        compatibility.validate()?;
        Ok(compatibility)
    }

    pub fn support(&self, capability: RendererCapability) -> Option<&CapabilitySupport> {
        self.capabilities
            .iter()
            .find(|support| support.capability == capability)
    }

    pub fn validate(&self) -> Result<()> {
        for (index, support) in self.capabilities.iter().enumerate() {
            if self.capabilities[..index]
                .iter()
                .any(|prior| prior.capability == support.capability)
            {
                return Err(invalid(format!(
                    "renderer capability {:?} was reported more than once",
                    support.capability
                )));
            }
        }
        for required in REQUIRED_RENDERER_CAPABILITIES {
            if !self
                .capabilities
                .iter()
                .any(|support| support.capability == *required)
            {
                return Err(invalid(format!(
                    "renderer capability {:?} was not reported",
                    required
                )));
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for BrowserCompatibility {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserCompatibilityWire| {
            Self::new(wire.version, wire.capabilities)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SupervisedTarget {
    pub target: PageTarget,
    pub lifecycle: TargetLifecycle,
    pub visibility: TargetVisibility,
    pub attachment_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BrowserSessionEvent {
    SessionStateChanged {
        state: BrowserSessionState,
    },
    SessionFailed {
        error: KrometrailError,
    },
    TargetDiscovered {
        target: SupervisedTarget,
    },
    TargetChanged {
        target: SupervisedTarget,
    },
    TargetClosed {
        target_id: crate::ids::TargetId,
    },
    SelectedTargetChanged {
        previous: Option<crate::ids::TargetId>,
        selected: Option<crate::ids::TargetId>,
    },
    TargetFailed {
        target_id: crate::ids::TargetId,
        error: KrometrailError,
    },
    CaptureStateChanged {
        status: TargetCaptureStatus,
    },
    CaptureGapDeclared {
        gap: CaptureGap,
    },
}

pub const REQUIRED_RENDERER_CAPABILITIES: &[RendererCapability] = RendererCapability::REQUIRED;

pub fn renderer_capability_is_required(capability: RendererCapability) -> bool {
    capability.is_required()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserProduct, BrowserProductVersion, ProfileIdentity, ProfileRef, ids::TargetId,
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn browser_version() -> BrowserVersion {
        BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("128").unwrap(),
            "revision",
            "1.3",
            "Chrome/128",
            "12",
        )
        .unwrap()
    }

    fn support(capability: RendererCapability) -> CapabilitySupport {
        CapabilitySupport::new(capability, true, capability.is_required(), None).unwrap()
    }

    #[test]
    fn renderer_capability_registry_is_complete_and_unique() {
        assert_eq!(RendererCapability::ALL.len(), 8);
        assert_eq!(RendererCapability::ALL, RendererCapability::REQUIRED);
        for (index, capability) in RendererCapability::ALL.iter().enumerate() {
            assert!(
                RendererCapability::ALL[..index]
                    .iter()
                    .all(|prior| prior != capability)
            );
            assert!(renderer_capability_is_required(*capability));
        }
    }

    #[test]
    fn compatibility_rejects_duplicate_capabilities_and_round_trips() {
        let duplicate = vec![
            support(RendererCapability::Page),
            support(RendererCapability::Page),
        ];
        assert!(BrowserCompatibility::new(browser_version(), duplicate).is_err());
        assert!(
            BrowserCompatibility::new(browser_version(), vec![support(RendererCapability::Page)])
                .is_err()
        );

        let value = BrowserCompatibility::new(
            browser_version(),
            RendererCapability::ALL
                .iter()
                .copied()
                .map(support)
                .collect(),
        )
        .unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<BrowserCompatibility>(&json).unwrap(),
            value
        );
    }

    #[test]
    fn session_event_stream_values_are_structured_and_stable() {
        let target = SupervisedTarget {
            target: PageTarget::new(
                TargetId::from_uuid(UUID.parse().unwrap()),
                "target-key",
                "https://example.test",
                "Example",
            )
            .unwrap(),
            lifecycle: TargetLifecycle::Discovered,
            visibility: TargetVisibility::Unknown,
            attachment_generation: 0,
        };
        let event = BrowserSessionEvent::TargetDiscovered { target };
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(encoded.contains("target_discovered"));
        assert_eq!(
            serde_json::from_str::<BrowserSessionEvent>(&encoded).unwrap(),
            event
        );
        let _ = ProfileRef::managed(ProfileIdentity::new("managed").unwrap());
    }
}

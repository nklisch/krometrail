//! Capability registry: identifiers, defaults, dependencies, and subsystems live here once.

use serde::{Deserialize, Serialize};

use crate::{
    ErrorCode,
    error::{KrometrailError, Result, invalid},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityDefault {
    Enabled,
    Disabled,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingSubsystem {
    VisualCapture,
    BrowserEvents,
    PageState,
    FrameworkState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDefinition {
    pub id: CapabilityId,
    pub default: CapabilityDefault,
    pub dependencies: &'static [CapabilityId],
    pub recording_subsystems: &'static [RecordingSubsystem],
}

macro_rules! define_capability_registry {
	($(
		$variant:ident {
			default: $default:ident,
			dependencies: [$($dependency:expr),* $(,)?],
			subsystems: [$($subsystem:expr),* $(,)?] $(,)?
		}
	),+ $(,)?) => {
		#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
		#[serde(rename_all = "kebab-case")]
		pub enum CapabilityId {
			$($variant),+
		}

		impl CapabilityId {
			pub const ALL: [Self; define_capability_registry!(@count $($variant),+)] = [$(Self::$variant),+];
		}

		pub static CAPABILITY_REGISTRY: &[CapabilityDefinition] = &[
			$(CapabilityDefinition {
				id: CapabilityId::$variant,
				default: CapabilityDefault::$default,
				dependencies: &[$($dependency),*],
				recording_subsystems: &[$($subsystem),*],
			}),+
		];
	};
	(@count $head:ident $(, $tail:ident)*) => { 1usize $(+ define_capability_registry!(@count $tail))* };
	(@count $only:ident) => { 1usize };
}

define_capability_registry!(
    Control {
        default: Enabled,
        dependencies: [],
        subsystems: [],
    },
    TemporalVision {
        default: Enabled,
        dependencies: [],
        subsystems: [RecordingSubsystem::VisualCapture],
    },
    BrowserEvents {
        default: Enabled,
        dependencies: [],
        subsystems: [RecordingSubsystem::BrowserEvents],
    },
    PageState {
        default: Unavailable,
        dependencies: [CapabilityId::Control],
        subsystems: [RecordingSubsystem::PageState],
    },
    FrameworkState {
        default: Unavailable,
        dependencies: [CapabilityId::PageState],
        subsystems: [RecordingSubsystem::FrameworkState],
    },
);

pub fn capability(id: CapabilityId) -> &'static CapabilityDefinition {
    CAPABILITY_REGISTRY
        .iter()
        .find(|definition| definition.id == id)
        .expect("capability registry must contain every CapabilityId")
}

pub fn validate_capability_selection(enabled: &[CapabilityId]) -> Result<()> {
    for (index, id) in enabled.iter().enumerate() {
        if enabled[..index].contains(id) {
            return Err(invalid(format!(
                "capability {id:?} was selected more than once"
            )));
        }
        let definition = capability(*id);
        if definition.default == CapabilityDefault::Unavailable {
            return Err(KrometrailError::new(
                ErrorCode::Unsupported,
                format!("capability {id:?} is unavailable"),
            ));
        }
        for dependency in definition.dependencies {
            if !enabled.contains(dependency) {
                return Err(invalid(format!(
                    "capability {id:?} requires {dependency:?}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_and_all_are_one_complete_variant_set() {
        assert_eq!(CapabilityId::ALL.len(), CAPABILITY_REGISTRY.len());
        for id in CapabilityId::ALL {
            assert_eq!(capability(id).id, id);
        }
    }

    #[test]
    fn defaults_and_unavailable_extensions_match_spec() {
        assert_eq!(
            capability(CapabilityId::Control).default,
            CapabilityDefault::Enabled
        );
        assert_eq!(
            capability(CapabilityId::TemporalVision).default,
            CapabilityDefault::Enabled
        );
        assert_eq!(
            capability(CapabilityId::BrowserEvents).default,
            CapabilityDefault::Enabled
        );
        assert_eq!(
            capability(CapabilityId::PageState).default,
            CapabilityDefault::Unavailable
        );
        assert_eq!(
            capability(CapabilityId::FrameworkState).default,
            CapabilityDefault::Unavailable
        );
    }

    #[test]
    fn rejects_unavailable_missing_dependencies_and_duplicates() {
        assert!(validate_capability_selection(&[CapabilityId::PageState]).is_err());
        assert!(
            validate_capability_selection(&[CapabilityId::Control, CapabilityId::PageState])
                .is_err()
        );
        assert!(
            validate_capability_selection(&[CapabilityId::Control, CapabilityId::Control]).is_err()
        );
        assert!(
            validate_capability_selection(&[CapabilityId::Control, CapabilityId::TemporalVision])
                .is_ok()
        );
    }
}

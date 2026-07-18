//! Capability registry: identifiers, defaults, dependencies, and subsystems live here once.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    ErrorCode,
    error::{KrometrailError, NonEmptyText, Result, invalid},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityDefault {
    Enabled,
    Disabled,
    RuntimeQualified,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityState {
    Enabled,
    Disabled,
    Unavailable,
}

/// One immutable, registry-ordered capability decision for a process surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySnapshot {
    states: Arc<[CapabilityState]>,
    enabled: Arc<[CapabilityId]>,
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
    TemporalVideo {
        default: RuntimeQualified,
        dependencies: [CapabilityId::TemporalVision],
        subsystems: [],
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
                NonEmptyText::new(format!("capability {id:?} is unavailable"))
                    .expect("capability error message is non-empty"),
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

impl CapabilitySnapshot {
    /// Resolve registry defaults against the capabilities proven available at startup.
    pub fn resolve_defaults(runtime_qualified: &[CapabilityId]) -> Result<Self> {
        Self::resolve(None, runtime_qualified)
    }

    /// Resolve an explicit operator selection against startup qualification.
    pub fn resolve_explicit(
        selected: Vec<CapabilityId>,
        runtime_qualified: &[CapabilityId],
    ) -> Result<Self> {
        Self::resolve(Some(selected), runtime_qualified)
    }

    fn resolve(
        selected: Option<Vec<CapabilityId>>,
        runtime_qualified: &[CapabilityId],
    ) -> Result<Self> {
        validate_runtime_qualification(runtime_qualified)?;
        if let Some(selected) = selected.as_deref() {
            validate_capability_selection(selected)?;
        }

        let explicitly_selected = selected.is_some();
        let requested = selected.unwrap_or_else(|| {
            CAPABILITY_REGISTRY
                .iter()
                .filter(|definition| {
                    matches!(
                        definition.default,
                        CapabilityDefault::Enabled | CapabilityDefault::RuntimeQualified
                    )
                })
                .map(|definition| definition.id)
                .collect()
        });
        let mut states = Vec::with_capacity(CAPABILITY_REGISTRY.len());
        let mut enabled = Vec::with_capacity(CAPABILITY_REGISTRY.len());
        for definition in CAPABILITY_REGISTRY {
            let requested = requested.contains(&definition.id);
            let state = if !requested {
                match definition.default {
                    CapabilityDefault::Unavailable => CapabilityState::Unavailable,
                    _ => CapabilityState::Disabled,
                }
            } else if definition.default == CapabilityDefault::Unavailable {
                return Err(unavailable_error(definition.id));
            } else if definition.default == CapabilityDefault::RuntimeQualified
                && !runtime_qualified.contains(&definition.id)
            {
                if explicitly_selected {
                    return Err(unavailable_error(definition.id));
                }
                CapabilityState::Unavailable
            } else {
                for dependency in definition.dependencies {
                    if !enabled.contains(dependency) {
                        return Err(invalid(format!(
                            "capability {:?} requires {dependency:?}",
                            definition.id
                        )));
                    }
                }
                enabled.push(definition.id);
                CapabilityState::Enabled
            };
            states.push(state);
        }
        Ok(Self {
            states: states.into(),
            enabled: enabled.into(),
        })
    }

    pub fn state(&self, id: CapabilityId) -> CapabilityState {
        let index = CAPABILITY_REGISTRY
            .iter()
            .position(|definition| definition.id == id)
            .expect("capability registry contains every id");
        self.states[index]
    }

    pub fn is_enabled(&self, id: CapabilityId) -> bool {
        self.state(id) == CapabilityState::Enabled
    }

    pub fn enabled_capabilities(&self) -> &[CapabilityId] {
        &self.enabled
    }
}

fn validate_runtime_qualification(runtime_qualified: &[CapabilityId]) -> Result<()> {
    for (index, id) in runtime_qualified.iter().enumerate() {
        if runtime_qualified[..index].contains(id)
            || capability(*id).default != CapabilityDefault::RuntimeQualified
        {
            return Err(invalid(format!(
                "capability {id:?} has an invalid runtime qualification"
            )));
        }
    }
    Ok(())
}

fn unavailable_error(id: CapabilityId) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Unsupported,
        NonEmptyText::new(format!("capability {id:?} is unavailable"))
            .expect("capability error message is non-empty"),
    )
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
            capability(CapabilityId::TemporalVideo).default,
            CapabilityDefault::RuntimeQualified
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

    #[test]
    fn startup_snapshot_is_registry_ordered_and_runtime_qualified_once() {
        let unavailable = CapabilitySnapshot::resolve_defaults(&[]).unwrap();
        assert_eq!(
            unavailable.state(CapabilityId::TemporalVideo),
            CapabilityState::Unavailable
        );
        assert!(!unavailable.is_enabled(CapabilityId::TemporalVideo));

        let qualified =
            CapabilitySnapshot::resolve_defaults(&[CapabilityId::TemporalVideo]).unwrap();
        assert!(qualified.is_enabled(CapabilityId::TemporalVideo));
        assert_eq!(
            qualified.enabled_capabilities(),
            &[
                CapabilityId::Control,
                CapabilityId::TemporalVision,
                CapabilityId::TemporalVideo,
                CapabilityId::BrowserEvents,
            ]
        );
    }

    #[test]
    fn explicit_selection_can_disable_video_but_cannot_claim_unavailable_video() {
        let disabled = CapabilitySnapshot::resolve_explicit(
            vec![CapabilityId::Control, CapabilityId::TemporalVision],
            &[CapabilityId::TemporalVideo],
        )
        .unwrap();
        assert_eq!(
            disabled.state(CapabilityId::TemporalVideo),
            CapabilityState::Disabled
        );
        assert!(
            CapabilitySnapshot::resolve_explicit(
                vec![CapabilityId::TemporalVision, CapabilityId::TemporalVideo],
                &[],
            )
            .is_err()
        );
        assert!(
            CapabilitySnapshot::resolve_explicit(
                vec![CapabilityId::TemporalVideo],
                &[CapabilityId::TemporalVideo],
            )
            .is_err()
        );
        assert!(CapabilitySnapshot::resolve_defaults(&[CapabilityId::Control]).is_err());
    }
}

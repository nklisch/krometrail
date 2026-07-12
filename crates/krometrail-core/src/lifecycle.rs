//! Exhaustive lifecycle transition tables.

use serde::{Deserialize, Serialize};

use crate::error::{Result, invalid_transition};

macro_rules! define_lifecycle {
    (
        $vis:vis enum $name:ident {
            $( $variant:ident => $stable_name:literal : [$($next:ident),* $(,)?] ),+ $(,)?
        }
    ) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
        $vis enum $name {
            $( #[serde(rename = $stable_name)] $variant ),+
        }

        impl $name {
            /// The enum, iterable state set, stable names, and transition rows are one
            /// declaration so adding a state necessarily updates the exhaustive registry.
            pub const ALL: &'static [Self] = &[
                $(Self::$variant),+
            ];

            pub const TRANSITIONS: &'static [(Self, &'static [Self])] = &[
                $(
                    (Self::$variant, &[$(Self::$next),*]),
                )+
            ];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $stable_name),+
                }
            }

            pub fn allows_transition(self, next: Self) -> bool {
                Self::TRANSITIONS
                    .iter()
                    .any(|(from, allowed)| *from == self && allowed.contains(&next))
            }

            pub fn transition(self, next: Self) -> Result<Self> {
                if self.allows_transition(next) {
                    Ok(next)
                } else {
                    Err(invalid_transition(format!(
                        "cannot transition {} from {} to {}",
                        stringify!($name),
                        self.as_str(),
                        next.as_str()
                    )))
                }
            }
        }
    };
}

define_lifecycle! {
    pub enum SessionLifecycle {
        Starting => "starting": [Recording, Reconnecting, Stopping],
        Recording => "recording": [Reconnecting, Stopping],
        Reconnecting => "reconnecting": [Recording, Stopping],
        Stopping => "stopping": [Ended],
        Ended => "ended": [],
    }
}

define_lifecycle! {
    pub enum TargetLifecycle {
        Discovered => "discovered": [Attached, Closed, Failed],
        Attached => "attached": [Recording, Hidden, Closed, Failed],
        Recording => "recording": [Hidden, Closed, Failed],
        Hidden => "hidden": [Recording, Closed, Failed],
        Closed => "closed": [],
        Failed => "failed": [],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn assert_session_registry_is_closed() {
        assert_eq!(
            SessionLifecycle::TRANSITIONS.len(),
            SessionLifecycle::ALL.len()
        );
        for (from, allowed) in SessionLifecycle::TRANSITIONS {
            assert!(SessionLifecycle::ALL.contains(from));
            for next in *allowed {
                assert!(SessionLifecycle::ALL.contains(next));
                assert!(from.allows_transition(*next));
            }
        }
    }

    fn assert_target_registry_is_closed() {
        assert_eq!(
            TargetLifecycle::TRANSITIONS.len(),
            TargetLifecycle::ALL.len()
        );
        for (from, allowed) in TargetLifecycle::TRANSITIONS {
            assert!(TargetLifecycle::ALL.contains(from));
            for next in *allowed {
                assert!(TargetLifecycle::ALL.contains(next));
                assert!(from.allows_transition(*next));
            }
        }
    }

    #[test]
    fn every_session_lifecycle_pair_is_classified() {
        assert_session_registry_is_closed();

        for from in SessionLifecycle::ALL {
            for next in SessionLifecycle::ALL {
                let result = from.transition(*next);
                assert_eq!(
                    result.is_ok(),
                    from.allows_transition(*next),
                    "unclassified session pair {} -> {}",
                    from.as_str(),
                    next.as_str()
                );
                if from.allows_transition(*next) {
                    assert_eq!(result.unwrap(), *next);
                } else {
                    assert_eq!(
                        result.unwrap_err().code,
                        ErrorCode::InvalidLifecycleTransition
                    );
                }
            }
        }
    }

    #[test]
    fn every_target_lifecycle_pair_is_classified() {
        assert_target_registry_is_closed();

        for from in TargetLifecycle::ALL {
            for next in TargetLifecycle::ALL {
                let result = from.transition(*next);
                assert_eq!(
                    result.is_ok(),
                    from.allows_transition(*next),
                    "unclassified target pair {} -> {}",
                    from.as_str(),
                    next.as_str()
                );
                if from.allows_transition(*next) {
                    assert_eq!(result.unwrap(), *next);
                } else {
                    assert_eq!(
                        result.unwrap_err().code,
                        ErrorCode::InvalidLifecycleTransition
                    );
                }
            }
        }
    }
}

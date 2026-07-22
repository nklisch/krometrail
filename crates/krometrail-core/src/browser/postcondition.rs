//! Bounded pre/post interaction postcondition facts.
//!
//! Every field is an observation; absence always means "not observed", never a
//! claimed failure. Facts follow the [`super::SanitizedParameters`] privacy
//! discipline: booleans, bounded enums, and lengths only — URLs, raw values,
//! and page content never enter these types.

use serde::{Deserialize, Serialize};

use crate::{Result, error::invalid, validation::deserialize_validated};

/// Bounded node-state facts captured by an actionability probe.
/// All fields are observations; `None` means the probe could not read the fact.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeStateFacts {
    pub connected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
    pub pressed: Option<bool>,
    pub value_length: Option<u32>,
}

/// One observed boolean pre/post fact. `changed` is `None` when either side is
/// unobserved; construction and wire decoding enforce that consistency.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FlagObservation {
    pub before: Option<bool>,
    pub after: Option<bool>,
    pub changed: Option<bool>,
}

#[derive(Deserialize)]
struct FlagObservationWire {
    before: Option<bool>,
    after: Option<bool>,
    changed: Option<bool>,
}

impl FlagObservation {
    /// Derives `changed` from the observed sides; either unobserved side keeps
    /// the delta unobserved.
    pub fn observed(before: Option<bool>, after: Option<bool>) -> Self {
        Self {
            before,
            after,
            changed: before.zip(after).map(|(before, after)| before != after),
        }
    }

    pub const fn unobserved() -> Self {
        Self {
            before: None,
            after: None,
            changed: None,
        }
    }

    fn validated(before: Option<bool>, after: Option<bool>, changed: Option<bool>) -> Result<Self> {
        let value = Self::observed(before, after);
        if value.changed != changed {
            return Err(invalid(
                "flag observation changed fact does not match its observed sides",
            ));
        }
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for FlagObservation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: FlagObservationWire| {
            Self::validated(wire.before, wire.after, wire.changed)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetNodeOutcome {
    /// No post-action target evaluation was performed: the action had no
    /// element target (e.g. page-scoped press_keys) or the observation point
    /// was blocked before the target could be probed.
    NotEvaluated,
    /// The pre-resolved backing node was still connected post-action.
    Present,
    /// The pre-resolved backing node was gone, disconnected, or no longer
    /// resolvable post-action.
    DetachedOrReplaced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TargetPostcondition {
    pub node: TargetNodeOutcome,
    pub checked: FlagObservation,
    pub expanded: FlagObservation,
    pub selected: FlagObservation,
    pub pressed: FlagObservation,
    pub value_length_changed: Option<bool>,
}

impl TargetPostcondition {
    const fn unobserved(node: TargetNodeOutcome) -> Self {
        Self {
            node,
            checked: FlagObservation::unobserved(),
            expanded: FlagObservation::unobserved(),
            selected: FlagObservation::unobserved(),
            pressed: FlagObservation::unobserved(),
            value_length_changed: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PagePostcondition {
    /// `None` when the pre or post URL read degraded. URLs themselves never
    /// enter this type; the control layer compares strings and passes only
    /// the boolean inward.
    pub url_changed: Option<bool>,
    /// A page lifecycle signal arrived between dispatch and observation.
    pub navigation_lifecycle_observed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionPostcondition {
    pub page: PagePostcondition,
    pub target: TargetPostcondition,
}

impl InteractionPostcondition {
    /// Pure assembly from pre/post probes; every degradation maps to
    /// not-observed, never to an error.
    ///
    /// A missing post probe with a present pre probe means the backing node
    /// no longer resolved (or its probe degraded); that maps to
    /// `DetachedOrReplaced` with unobserved after-facts. A disconnected post
    /// probe also keeps its readable state out of the after-facts: a detached
    /// node's state no longer describes the current document.
    pub fn from_facts(
        pre: Option<&NodeStateFacts>,
        post: Option<&NodeStateFacts>,
        url_changed: Option<bool>,
        navigation_lifecycle_observed: bool,
    ) -> Self {
        let target = match pre {
            None => TargetPostcondition::unobserved(TargetNodeOutcome::NotEvaluated),
            Some(pre) => {
                let (node, post) = match post {
                    Some(post) if post.connected => (TargetNodeOutcome::Present, Some(post)),
                    Some(_) | None => (TargetNodeOutcome::DetachedOrReplaced, None),
                };
                TargetPostcondition {
                    node,
                    checked: FlagObservation::observed(
                        pre.checked,
                        post.and_then(|post| post.checked),
                    ),
                    expanded: FlagObservation::observed(
                        pre.expanded,
                        post.and_then(|post| post.expanded),
                    ),
                    selected: FlagObservation::observed(
                        pre.selected,
                        post.and_then(|post| post.selected),
                    ),
                    pressed: FlagObservation::observed(
                        pre.pressed,
                        post.and_then(|post| post.pressed),
                    ),
                    value_length_changed: pre
                        .value_length
                        .zip(post.and_then(|post| post.value_length))
                        .map(|(before, after)| before != after),
                }
            }
        };
        Self {
            page: PagePostcondition {
                url_changed,
                navigation_lifecycle_observed,
            },
            target,
        }
    }

    /// All facts unobserved (pre-dispatch-only records, clipboard/download
    /// records).
    pub const fn unobserved() -> Self {
        Self {
            page: PagePostcondition {
                url_changed: None,
                navigation_lifecycle_observed: false,
            },
            target: TargetPostcondition::unobserved(TargetNodeOutcome::NotEvaluated),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facts(checked: Option<bool>, value_length: Option<u32>) -> NodeStateFacts {
        NodeStateFacts {
            connected: true,
            checked,
            expanded: Some(false),
            selected: None,
            pressed: None,
            value_length,
        }
    }

    #[test]
    fn flag_observation_wire_decoding_rejects_inconsistent_changed() {
        let consistent: FlagObservation =
            serde_json::from_value(json!({"before": false, "after": true, "changed": true}))
                .unwrap();
        assert_eq!(
            consistent,
            FlagObservation::observed(Some(false), Some(true))
        );
        for inconsistent in [
            json!({"before": false, "after": true, "changed": false}),
            json!({"before": false, "after": true, "changed": null}),
            json!({"before": null, "after": true, "changed": true}),
            json!({"before": false, "after": null, "changed": false}),
        ] {
            assert!(
                serde_json::from_value::<FlagObservation>(inconsistent.clone()).is_err(),
                "{inconsistent}"
            );
        }
        let unobserved: FlagObservation =
            serde_json::from_value(json!({"before": null, "after": null, "changed": null}))
                .unwrap();
        assert_eq!(unobserved, FlagObservation::unobserved());
    }

    #[test]
    fn from_facts_reports_deltas_when_both_probes_observe_the_target() {
        let pre = facts(Some(false), Some(0));
        let post = facts(Some(true), Some(8));
        let value =
            InteractionPostcondition::from_facts(Some(&pre), Some(&post), Some(false), false);
        assert_eq!(value.target.node, TargetNodeOutcome::Present);
        assert_eq!(value.target.checked.changed, Some(true));
        assert_eq!(value.target.expanded.changed, Some(false));
        assert_eq!(value.target.selected, FlagObservation::unobserved());
        assert_eq!(value.target.value_length_changed, Some(true));
        assert_eq!(value.page.url_changed, Some(false));
        assert!(!value.page.navigation_lifecycle_observed);
    }

    #[test]
    fn from_facts_missing_post_probe_degrades_deltas_and_keeps_the_node_outcome() {
        let pre = facts(Some(true), Some(4));
        let value = InteractionPostcondition::from_facts(Some(&pre), None, None, true);
        assert_eq!(value.target.node, TargetNodeOutcome::DetachedOrReplaced);
        assert_eq!(value.target.checked.before, Some(true));
        assert_eq!(value.target.checked.after, None);
        assert_eq!(value.target.checked.changed, None);
        assert_eq!(value.target.value_length_changed, None);
        assert_eq!(value.page.url_changed, None);
        assert!(value.page.navigation_lifecycle_observed);
    }

    #[test]
    fn from_facts_disconnected_post_probe_keeps_detached_state_unobserved() {
        let pre = facts(Some(false), Some(0));
        let post = NodeStateFacts {
            connected: false,
            checked: Some(true),
            ..NodeStateFacts::default()
        };
        let value = InteractionPostcondition::from_facts(Some(&pre), Some(&post), None, false);
        assert_eq!(value.target.node, TargetNodeOutcome::DetachedOrReplaced);
        assert_eq!(value.target.checked.after, None);
        assert_eq!(value.target.checked.changed, None);
    }

    #[test]
    fn from_facts_without_a_pre_probe_never_evaluates_the_target() {
        for post in [None, Some(facts(Some(true), Some(1)))] {
            let value = InteractionPostcondition::from_facts(None, post.as_ref(), Some(true), true);
            assert_eq!(value.target.node, TargetNodeOutcome::NotEvaluated);
            assert_eq!(value.target.checked, FlagObservation::unobserved());
            assert_eq!(value.target.value_length_changed, None);
            assert_eq!(value.page.url_changed, Some(true));
            assert!(value.page.navigation_lifecycle_observed);
        }
    }

    #[test]
    fn unobserved_postcondition_round_trips_through_serde() {
        let value = InteractionPostcondition::unobserved();
        let decoded: InteractionPostcondition =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(decoded.page.url_changed, None);
        assert_eq!(decoded.target.node, TargetNodeOutcome::NotEvaluated);
    }
}

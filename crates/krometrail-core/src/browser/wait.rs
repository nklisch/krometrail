use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    NonEmptyText, Result, SessionTime,
    error::invalid,
    validation::{delegate_json_schema, deserialize_validated},
};

use super::{DocumentReadiness, ElementLocator, ObservationContext, PageSelection};

pub const MAX_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);
pub const MIN_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);
pub const MAX_WAIT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MAX_WAIT_TEXT_BYTES: usize = 16 * 1024;
const MAX_WAIT_EXPRESSION_BYTES: usize = 16 * 1024;
const MAX_WAIT_SELECTOR_BYTES: usize = 4 * 1024;
const MAX_WAIT_URL_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitTextMatch {
    Contains,
    Exact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitPresence {
    Present,
    Absent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ElementState {
    Attached,
    Visible,
    Hidden,
    Enabled,
    Disabled,
    Editable,
    Checked,
    Unchecked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UrlMatch {
    Exact,
    Prefix,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaitCondition {
    Elapsed {
        duration: Duration,
    },
    Text {
        locator: Option<ElementLocator>,
        text: NonEmptyText,
        match_mode: WaitTextMatch,
        presence: WaitPresence,
        case_sensitive: bool,
    },
    Element {
        locator: ElementLocator,
        state: ElementState,
    },
    Navigation {
        readiness: DocumentReadiness,
        url: Option<(UrlMatch, NonEmptyText)>,
    },
    Page {
        expression: NonEmptyText,
    },
    NetworkQuiet {
        quiet_for: Duration,
    },
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "condition",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum WaitConditionWire {
    Elapsed {
        duration: u64,
    },
    Text {
        /// Limits text matching to this element. When omitted, the scope is the full document body text; use a locator for exact element text.
        locator: Option<ElementLocator>,
        text: NonEmptyText,
        /// `exact` compares the complete text in that scope. For an unscoped substring, use `contains`.
        match_mode: WaitTextMatch,
        presence: WaitPresence,
        case_sensitive: bool,
    },
    Element {
        locator: ElementLocator,
        state: ElementState,
    },
    Navigation {
        readiness: DocumentReadiness,
        url: Option<(UrlMatch, NonEmptyText)>,
    },
    Page {
        expression: NonEmptyText,
    },
    NetworkQuiet {
        quiet_for: u64,
    },
}

delegate_json_schema!(WaitCondition => WaitConditionWire);

impl WaitCondition {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Elapsed { duration } => {
                validate_millisecond_duration(*duration, "elapsed duration")?;
            }
            Self::Text { locator, text, .. } => {
                validate_text(text, MAX_WAIT_TEXT_BYTES, "wait text")?;
                if let Some(locator) = locator {
                    validate_locator(locator)?;
                }
            }
            Self::Element { locator, .. } => validate_locator(locator)?,
            Self::Navigation { url, .. } => {
                if let Some((_, url)) = url {
                    validate_text(url, MAX_WAIT_URL_BYTES, "wait URL predicate")?;
                }
            }
            Self::Page { expression } => {
                validate_text(expression, MAX_WAIT_EXPRESSION_BYTES, "wait expression")?;
            }
            Self::NetworkQuiet { quiet_for } => {
                validate_millisecond_duration(*quiet_for, "network quiet duration")?;
            }
        }
        Ok(())
    }
}

impl Serialize for WaitCondition {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Elapsed { duration } => WaitConditionWire::Elapsed {
                duration: duration_millis(*duration).map_err(serde::ser::Error::custom)?,
            },
            Self::Text {
                locator,
                text,
                match_mode,
                presence,
                case_sensitive,
            } => WaitConditionWire::Text {
                locator: locator.clone(),
                text: text.clone(),
                match_mode: *match_mode,
                presence: *presence,
                case_sensitive: *case_sensitive,
            },
            Self::Element { locator, state } => WaitConditionWire::Element {
                locator: locator.clone(),
                state: *state,
            },
            Self::Navigation { readiness, url } => WaitConditionWire::Navigation {
                readiness: *readiness,
                url: url.clone(),
            },
            Self::Page { expression } => WaitConditionWire::Page {
                expression: expression.clone(),
            },
            Self::NetworkQuiet { quiet_for } => WaitConditionWire::NetworkQuiet {
                quiet_for: duration_millis(*quiet_for).map_err(serde::ser::Error::custom)?,
            },
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WaitCondition {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: WaitConditionWire| {
            let value = match wire {
                WaitConditionWire::Elapsed { duration } => Self::Elapsed {
                    duration: duration_from_millis(duration, "elapsed duration")?,
                },
                WaitConditionWire::Text {
                    locator,
                    text,
                    match_mode,
                    presence,
                    case_sensitive,
                } => Self::Text {
                    locator,
                    text,
                    match_mode,
                    presence,
                    case_sensitive,
                },
                WaitConditionWire::Element { locator, state } => Self::Element { locator, state },
                WaitConditionWire::Navigation { readiness, url } => {
                    Self::Navigation { readiness, url }
                }
                WaitConditionWire::Page { expression } => Self::Page { expression },
                WaitConditionWire::NetworkQuiet { quiet_for } => Self::NetworkQuiet {
                    quiet_for: duration_from_millis(quiet_for, "network quiet duration")?,
                },
            };
            value.validate()?;
            Ok(value)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WaitRequest {
    pub target: PageSelection,
    pub condition: WaitCondition,
    #[serde(serialize_with = "serialize_duration")]
    pub timeout: Duration,
    #[serde(serialize_with = "serialize_duration")]
    pub poll_interval: Duration,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct WaitRequestWire {
    #[serde(default)]
    target: PageSelection,
    condition: WaitCondition,
    timeout: u64,
    poll_interval: u64,
}

impl WaitRequest {
    pub fn new(
        target: PageSelection,
        condition: WaitCondition,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<Self> {
        condition.validate()?;
        validate_operation_timeout(timeout)?;
        validate_millisecond_duration(poll_interval, "wait poll interval")?;
        if !(MIN_WAIT_POLL_INTERVAL..=MAX_WAIT_POLL_INTERVAL).contains(&poll_interval) {
            return Err(invalid(
                "wait poll interval must be between 10 ms and 5 seconds",
            ));
        }
        match condition {
            WaitCondition::Elapsed { duration } if duration > timeout => {
                return Err(invalid("elapsed duration must not exceed wait timeout"));
            }
            WaitCondition::NetworkQuiet { quiet_for } if quiet_for > timeout => {
                return Err(invalid(
                    "network quiet duration must not exceed wait timeout",
                ));
            }
            _ => {}
        }
        Ok(Self {
            target,
            condition,
            timeout,
            poll_interval,
        })
    }
}

delegate_json_schema!(WaitRequest => WaitRequestWire);

impl<'de> Deserialize<'de> for WaitRequest {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: WaitRequestWire| {
            Self::new(
                wire.target,
                wire.condition,
                duration_from_millis(wire.timeout, "wait timeout")?,
                duration_from_millis(wire.poll_interval, "wait poll interval")?,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WaitOutcome {
    Satisfied { matched_at: SessionTime },
    TimedOut { last_probe_at: SessionTime },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "probe", rename_all = "snake_case")]
pub enum WaitProbe {
    Elapsed {
        matched: bool,
        elapsed_ms: u64,
    },
    Text {
        matched: bool,
        observed_length: Option<u32>,
    },
    Element {
        matched: bool,
        attached: bool,
        visible: Option<bool>,
        enabled: Option<bool>,
        editable: Option<bool>,
        checked: Option<bool>,
    },
    Navigation {
        matched: bool,
        readiness: DocumentReadiness,
        url_matched: Option<bool>,
    },
    Page {
        matched: bool,
    },
    NetworkQuiet {
        matched: bool,
        in_flight: u32,
        quiet_for_elapsed_ms: u64,
        tracks_from_subscription: bool,
        excludes_long_lived_connections: bool,
    },
}

impl WaitProbe {
    pub const fn matched(&self) -> bool {
        match self {
            Self::Elapsed { matched, .. }
            | Self::Text { matched, .. }
            | Self::Element { matched, .. }
            | Self::Navigation { matched, .. }
            | Self::Page { matched }
            | Self::NetworkQuiet { matched, .. } => *matched,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WaitResult {
    pub context: ObservationContext,
    pub condition: WaitCondition,
    pub outcome: WaitOutcome,
    pub last_probe: Option<WaitProbe>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitResultWire {
    context: ObservationContext,
    condition: WaitCondition,
    outcome: WaitOutcome,
    last_probe: Option<WaitProbe>,
}

impl WaitResult {
    pub fn new(
        context: ObservationContext,
        condition: WaitCondition,
        outcome: WaitOutcome,
        last_probe: Option<WaitProbe>,
    ) -> Result<Self> {
        condition.validate()?;
        let outcome_time = match outcome {
            WaitOutcome::Satisfied { matched_at } => matched_at,
            WaitOutcome::TimedOut { last_probe_at } => last_probe_at,
        };
        if outcome_time < context.started_at || outcome_time > context.completed_at {
            return Err(invalid(
                "wait outcome time must be inside its observation context",
            ));
        }
        if matches!(outcome, WaitOutcome::Satisfied { .. })
            && last_probe.as_ref().is_some_and(|probe| !probe.matched())
        {
            return Err(invalid(
                "satisfied wait must not retain a false final probe",
            ));
        }
        Ok(Self {
            context,
            condition,
            outcome,
            last_probe,
        })
    }
}

impl<'de> Deserialize<'de> for WaitResult {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: WaitResultWire| {
            Self::new(wire.context, wire.condition, wire.outcome, wire.last_probe)
        })
    }
}

pub(crate) fn validate_operation_timeout(timeout: Duration) -> Result<()> {
    validate_millisecond_duration(timeout, "operation timeout")?;
    if timeout > MAX_OPERATION_TIMEOUT {
        return Err(invalid("operation timeout must not exceed 120 seconds"));
    }
    Ok(())
}

fn validate_locator(locator: &ElementLocator) -> Result<()> {
    if let ElementLocator::CssSelector(selector) = locator {
        validate_text(selector, MAX_WAIT_SELECTOR_BYTES, "CSS selector")?;
    }
    Ok(())
}

fn validate_text(value: &NonEmptyText, max: usize, label: &str) -> Result<()> {
    if value.as_str().trim().is_empty() || value.as_str().len() > max {
        return Err(invalid(format!(
            "{label} is empty or exceeds its byte limit"
        )));
    }
    Ok(())
}

fn validate_millisecond_duration(value: Duration, label: &str) -> Result<()> {
    if value.is_zero() {
        return Err(invalid(format!("{label} must be non-zero")));
    }
    if value.subsec_nanos() % 1_000_000 != 0 {
        return Err(invalid(format!("{label} must use whole milliseconds")));
    }
    duration_millis(value).map(|_| ())
}

fn duration_from_millis(value: u64, label: &str) -> Result<Duration> {
    if value == 0 {
        return Err(invalid(format!("{label} must be non-zero")));
    }
    Ok(Duration::from_millis(value))
}

fn duration_millis(value: Duration) -> Result<u64> {
    u64::try_from(value.as_millis()).map_err(|_| invalid("duration exceeds millisecond range"))
}

fn serialize_duration<S: serde::Serializer>(
    value: &Duration,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    duration_millis(*value)
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SnapshotGeneration, SnapshotNodeId, TargetId};
    use uuid::Uuid;

    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(1))
    }

    fn request(condition: WaitCondition) -> WaitRequest {
        WaitRequest::new(
            PageSelection::Target(target()),
            condition,
            Duration::from_secs(2),
            Duration::from_millis(25),
        )
        .unwrap()
    }

    #[test]
    fn every_condition_round_trips_with_integer_milliseconds() {
        let reference = crate::NodeReference {
            target_id: target(),
            generation: SnapshotGeneration::new(1).unwrap(),
            node_id: SnapshotNodeId::new(2).unwrap(),
        };
        let conditions = vec![
            WaitCondition::Elapsed {
                duration: Duration::from_millis(50),
            },
            WaitCondition::Text {
                locator: None,
                text: NonEmptyText::new("ready").unwrap(),
                match_mode: WaitTextMatch::Contains,
                presence: WaitPresence::Present,
                case_sensitive: true,
            },
            WaitCondition::Element {
                locator: ElementLocator::Reference(reference),
                state: ElementState::Visible,
            },
            WaitCondition::Navigation {
                readiness: DocumentReadiness::Complete,
                url: Some((
                    UrlMatch::Prefix,
                    NonEmptyText::new("https://example.test").unwrap(),
                )),
            },
            WaitCondition::Page {
                expression: NonEmptyText::new("globalThis.ready === true").unwrap(),
            },
            WaitCondition::NetworkQuiet {
                quiet_for: Duration::from_millis(100),
            },
        ];
        for condition in conditions {
            let value = request(condition);
            let json = serde_json::to_value(&value).unwrap();
            assert!(json["timeout"].is_u64());
            assert!(json["poll_interval"].is_u64());
            assert_eq!(serde_json::from_value::<WaitRequest>(json).unwrap(), value);
        }
    }

    #[test]
    fn constructors_and_wire_reject_invalid_durations_and_bounded_text() {
        assert!(
            WaitRequest::new(
                PageSelection::Selected,
                WaitCondition::Elapsed {
                    duration: Duration::from_millis(11)
                },
                Duration::from_millis(10),
                Duration::from_millis(10),
            )
            .is_err()
        );
        assert!(
            WaitRequest::new(
                PageSelection::Selected,
                WaitCondition::NetworkQuiet {
                    quiet_for: Duration::from_millis(11)
                },
                Duration::from_millis(10),
                Duration::from_millis(10),
            )
            .is_err()
        );
        assert!(
            WaitRequest::new(
                PageSelection::Selected,
                WaitCondition::Page {
                    expression: NonEmptyText::new("x").unwrap()
                },
                Duration::from_secs(121),
                Duration::from_millis(10),
            )
            .is_err()
        );
        assert!(
            WaitRequest::new(
                PageSelection::Selected,
                WaitCondition::Page {
                    expression: NonEmptyText::new("x").unwrap()
                },
                Duration::from_secs(1),
                Duration::from_millis(9),
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<WaitRequest>(serde_json::json!({
                "target":{"selection":"selected"},
                "condition":{"condition":"elapsed","value":{"duration":0}},
                "timeout": 10.5,
                "poll_interval": 10
            }))
            .is_err()
        );
        let oversized = "x".repeat(MAX_WAIT_EXPRESSION_BYTES + 1);
        assert!(
            WaitRequest::new(
                PageSelection::Selected,
                WaitCondition::Page {
                    expression: NonEmptyText::new(oversized).unwrap()
                },
                Duration::from_secs(1),
                Duration::from_millis(10),
            )
            .is_err()
        );
    }
}

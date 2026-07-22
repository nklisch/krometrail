//! Bounded pre/post interaction postcondition facts.
//!
//! Every field is an observation; absence always means "not observed", never a
//! claimed failure. Facts follow the [`super::SanitizedParameters`] privacy
//! discipline: booleans, bounded enums, lengths, and opaque ordinals only —
//! URLs, raw values, and page content never enter these types.

use serde::{Deserialize, Serialize};

use crate::{
    DownloadId, DownloadSequence, DownloadState, PageSequence, Result, TargetId, error::invalid,
    validation::deserialize_validated,
};

/// Canonical cap for per-interaction side-channel fact lists.
pub const MAX_SIDE_CHANNEL_FACTS: usize = 4;

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
    /// The post-action probe was attempted but could not observe the backing
    /// node — transport failure, timeout, cancellation, or a malformed probe
    /// payload. Not a claim that the node detached.
    Unobserved,
    /// The pre-resolved backing node was still connected post-action.
    Present,
    /// The post-action probe ran and reported the pre-resolved backing node
    /// disconnected from the document.
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectationTargetRole {
    Link,
    Checkbox,
    Radio,
    Other,
}

impl ExpectationTargetRole {
    pub fn from_accessibility_role(role: &str) -> Self {
        if role.eq_ignore_ascii_case("link") {
            Self::Link
        } else if role.eq_ignore_ascii_case("checkbox") {
            Self::Checkbox
        } else if role.eq_ignore_ascii_case("radio") {
            Self::Radio
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectationTarget {
    Role(ExpectationTargetRole),
    AnyObservedRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectationChannel {
    Navigation,
    NewPage,
    Download,
    Checked,
    Expanded,
    ValueLength,
    Selected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionExpectation {
    pub target: ExpectationTarget,
    pub required_channels: &'static [ExpectationChannel],
    pub note: ExpectationNote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectationNote {
    NavigationOutcomeUnobserved,
    CheckedStateUnchanged,
    ExpandedStateUnchanged,
    ValueLengthUnchanged,
    SelectedStateUnchanged,
}

impl ExpectationNote {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NavigationOutcomeUnobserved => {
                "No navigation, new page, or download was observed by the observation point."
            }
            Self::CheckedStateUnchanged => {
                "The target's checked state was unchanged by the observation point."
            }
            Self::ExpandedStateUnchanged => {
                "The target's expanded state was unchanged by the observation point."
            }
            Self::ValueLengthUnchanged => {
                "The target's value length was unchanged by the observation point."
            }
            Self::SelectedStateUnchanged => {
                "The target's selected state was unchanged by the observation point."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpectationEvaluation {
    Held,
    DidNotHold(ExpectationNote),
    NotEvaluated,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservationSource {
    MainFrameNavigationSignal,
    UrlDelta,
    TargetStateProbe,
    PageCursorReconciliation,
    DownloadCursorAuthority,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChannelObservation {
    Changed { observed_through: ObservationSource },
    Unchanged { observed_through: ObservationSource },
    Unavailable,
    NotApplicable,
}

const ALL_EXPECTATION_CHANNELS: &[ExpectationChannel] = &[
    ExpectationChannel::Navigation,
    ExpectationChannel::NewPage,
    ExpectationChannel::Download,
    ExpectationChannel::Checked,
    ExpectationChannel::Expanded,
    ExpectationChannel::ValueLength,
    ExpectationChannel::Selected,
];

fn target_channel_observation(
    node: TargetNodeOutcome,
    changed: Option<bool>,
) -> ChannelObservation {
    if node != TargetNodeOutcome::Present {
        return ChannelObservation::Unavailable;
    }
    match changed {
        Some(true) => ChannelObservation::Changed {
            observed_through: ObservationSource::TargetStateProbe,
        },
        Some(false) => ChannelObservation::Unchanged {
            observed_through: ObservationSource::TargetStateProbe,
        },
        None => ChannelObservation::Unavailable,
    }
}

fn channel_observation(
    channel: ExpectationChannel,
    facts: &InteractionPostcondition,
) -> ChannelObservation {
    match channel {
        ExpectationChannel::Navigation => {
            if facts.page.main_frame_navigation_observed == Some(true)
                || facts.page.url_changed == Some(true)
            {
                ChannelObservation::Changed {
                    observed_through: if facts.page.main_frame_navigation_observed == Some(true) {
                        ObservationSource::MainFrameNavigationSignal
                    } else {
                        ObservationSource::UrlDelta
                    },
                }
            } else if facts.page.main_frame_navigation_observed == Some(false) {
                ChannelObservation::Unchanged {
                    observed_through: ObservationSource::MainFrameNavigationSignal,
                }
            } else {
                ChannelObservation::Unavailable
            }
        }
        ExpectationChannel::NewPage
            if facts.signals.window_open_attempts.is_some_and(|n| n > 0) =>
        {
            ChannelObservation::Unavailable
        }
        ExpectationChannel::NewPage => match facts.new_pages.as_ref() {
            Some(value) if !value.pages.is_empty() || value.omitted > 0 => {
                ChannelObservation::Changed {
                    observed_through: ObservationSource::PageCursorReconciliation,
                }
            }
            Some(_) => ChannelObservation::Unchanged {
                observed_through: ObservationSource::PageCursorReconciliation,
            },
            None => ChannelObservation::Unavailable,
        },
        ExpectationChannel::Download if facts.signals.download_requests.is_some_and(|n| n > 0) => {
            ChannelObservation::Unavailable
        }
        ExpectationChannel::Download => match facts.downloads.as_ref() {
            Some(value) if !value.downloads.is_empty() || value.omitted > 0 => {
                ChannelObservation::Changed {
                    observed_through: ObservationSource::DownloadCursorAuthority,
                }
            }
            Some(_) => ChannelObservation::Unchanged {
                observed_through: ObservationSource::DownloadCursorAuthority,
            },
            None => ChannelObservation::Unavailable,
        },
        ExpectationChannel::Checked => {
            target_channel_observation(facts.target.node, facts.target.checked.changed)
        }
        ExpectationChannel::Expanded => {
            target_channel_observation(facts.target.node, facts.target.expanded.changed)
        }
        ExpectationChannel::ValueLength => {
            target_channel_observation(facts.target.node, facts.target.value_length_changed)
        }
        ExpectationChannel::Selected => {
            target_channel_observation(facts.target.node, facts.target.selected.changed)
        }
    }
}

fn normalized_channel_observation(
    channel: ExpectationChannel,
    required: bool,
    facts: &InteractionPostcondition,
) -> ChannelObservation {
    if required {
        channel_observation(channel, facts)
    } else {
        ChannelObservation::NotApplicable
    }
}

fn target_matches(target: ExpectationTarget, role: Option<ExpectationTargetRole>) -> bool {
    match target {
        ExpectationTarget::Role(expected) => role == Some(expected),
        ExpectationTarget::AnyObservedRole => role.is_some(),
    }
}

pub(crate) fn evaluate_expectations(
    expectations: &[InteractionExpectation],
    target_role: Option<ExpectationTargetRole>,
    facts: &InteractionPostcondition,
) -> ExpectationEvaluation {
    let Some(expectation) = expectations
        .iter()
        .find(|expectation| target_matches(expectation.target, target_role))
    else {
        return if expectations.is_empty() || target_role.is_none() {
            if expectations.is_empty() {
                ExpectationEvaluation::NotApplicable
            } else {
                ExpectationEvaluation::NotEvaluated
            }
        } else {
            ExpectationEvaluation::NotApplicable
        };
    };

    if expectation.required_channels.is_empty() {
        return ExpectationEvaluation::NotApplicable;
    }

    let mut unavailable = false;
    for channel in ALL_EXPECTATION_CHANNELS {
        let required = expectation.required_channels.contains(channel);
        let observation = normalized_channel_observation(*channel, required, facts);
        if !required {
            continue;
        }
        match observation {
            ChannelObservation::Changed { observed_through }
            | ChannelObservation::Unchanged { observed_through } => {
                let _ = observed_through;
            }
            ChannelObservation::Unavailable | ChannelObservation::NotApplicable => {
                unavailable = true;
            }
        }
        if matches!(observation, ChannelObservation::Changed { .. }) {
            return ExpectationEvaluation::Held;
        }
    }
    if unavailable {
        ExpectationEvaluation::NotEvaluated
    } else {
        ExpectationEvaluation::DidNotHold(expectation.note)
    }
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
    /// A committed main-frame navigation (`Page.frameNavigated`) or main-frame
    /// same-document navigation (`Page.navigatedWithinDocument`) arrived
    /// between dispatch and observation. `None` = signal source unavailable.
    /// Catches same-URL reloads and committed-and-returned navigations that
    /// `url_changed` misses; `url_changed` stays a separate fact.
    pub main_frame_navigation_observed: Option<bool>,
}

/// One page adopted by the target supervisor after the pre-action cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NewPageFact {
    pub target_id: TargetId,
    pub sequence: PageSequence,
    /// The new page's opener is the acting interaction target.
    pub opener_matched: bool,
}

/// New-page inventory delta. Absent (`None` on the parent) when post-dispatch
/// reconciliation was unavailable — never a claim that nothing opened.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NewPagePostcondition {
    /// Pre-action page cursor: chain `wait_for_page(after: cursor_before)` to
    /// observe a page that had not been adopted by the observation point.
    pub cursor_before: PageSequence,
    /// Capped at [`MAX_SIDE_CHANNEL_FACTS`]; an empty list with the cursor
    /// present is the honest "no new page observed" fact.
    pub pages: Vec<NewPageFact>,
    /// Exact count of observed pages beyond the cap.
    pub omitted: u32,
}

#[derive(Deserialize)]
struct NewPagePostconditionWire {
    cursor_before: PageSequence,
    pages: Vec<NewPageFact>,
    omitted: u32,
}

impl NewPagePostcondition {
    /// Caps the list and records the exact omission count.
    pub fn from_observed(cursor_before: PageSequence, mut pages: Vec<NewPageFact>) -> Self {
        let omitted = pages.len().saturating_sub(MAX_SIDE_CHANNEL_FACTS);
        pages.truncate(MAX_SIDE_CHANNEL_FACTS);
        Self {
            cursor_before,
            pages,
            omitted: u32::try_from(omitted).unwrap_or(u32::MAX),
        }
    }
}

impl<'de> Deserialize<'de> for NewPagePostcondition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: NewPagePostconditionWire| {
            if wire.pages.len() > MAX_SIDE_CHANNEL_FACTS {
                return Err(invalid("new-page facts exceed the side-channel cap"));
            }
            Ok(Self {
                cursor_before: wire.cursor_before,
                pages: wire.pages,
                omitted: wire.omitted,
            })
        })
    }
}

/// One download whose begin was recorded after the pre-action cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DownloadFact {
    pub download_id: DownloadId,
    pub sequence: DownloadSequence,
    /// State at the observation point, not a terminal claim.
    pub state: DownloadState,
}

/// Download inventory delta keyed on begin ordering. Absent (`None` on the
/// parent) when the download authority is unavailable or the session does not
/// manage downloads — never a claim that nothing was downloaded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DownloadPostcondition {
    /// Pre-action download cursor: chain `wait_for_download(after:
    /// cursor_before)` to observe a download that had not begun by the
    /// observation point.
    pub cursor_before: DownloadSequence,
    /// Capped at [`MAX_SIDE_CHANNEL_FACTS`]; an empty list with the cursor
    /// present is the honest "no download observed" fact.
    pub downloads: Vec<DownloadFact>,
    /// Exact count of observed downloads beyond the cap.
    pub omitted: u32,
}

#[derive(Deserialize)]
struct DownloadPostconditionWire {
    cursor_before: DownloadSequence,
    downloads: Vec<DownloadFact>,
    omitted: u32,
}

impl DownloadPostcondition {
    /// Caps the list and records the exact omission count.
    pub fn from_observed(
        cursor_before: DownloadSequence,
        mut downloads: Vec<DownloadFact>,
    ) -> Self {
        let omitted = downloads.len().saturating_sub(MAX_SIDE_CHANNEL_FACTS);
        downloads.truncate(MAX_SIDE_CHANNEL_FACTS);
        Self {
            cursor_before,
            downloads,
            omitted: u32::try_from(omitted).unwrap_or(u32::MAX),
        }
    }
}

impl<'de> Deserialize<'de> for DownloadPostcondition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: DownloadPostconditionWire| {
            if wire.downloads.len() > MAX_SIDE_CHANNEL_FACTS {
                return Err(invalid("download facts exceed the side-channel cap"));
            }
            Ok(Self {
                cursor_before: wire.cursor_before,
                downloads: wire.downloads,
                omitted: wire.omitted,
            })
        })
    }
}

/// Session-scoped attempt signals drained at the observation point.
/// `None` = signal source unavailable; counts are lower bounds under
/// broadcast lag and never claim an attempt was blocked or succeeded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SideChannelSignals {
    /// `Page.windowOpen` occurrences (open attempts; no blocked/succeeded
    /// claim).
    pub window_open_attempts: Option<u32>,
    /// `Page.frameRequestedNavigation` with disposition `download`
    /// (download-attempt candidates).
    pub download_requests: Option<u32>,
}

impl SideChannelSignals {
    pub const fn unobserved() -> Self {
        Self {
            window_open_attempts: None,
            download_requests: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionPostcondition {
    pub page: PagePostcondition,
    pub target: TargetPostcondition,
    pub signals: SideChannelSignals,
    pub new_pages: Option<NewPagePostcondition>,
    pub downloads: Option<DownloadPostcondition>,
    /// `Some(true)` only on WriteClipboard records whose in-page bridge
    /// confirmed the write. `None` everywhere else.
    pub clipboard_write_confirmed: Option<bool>,
}

impl InteractionPostcondition {
    /// Pure assembly from pre/post probes; every degradation maps to
    /// not-observed, never to an error.
    ///
    /// A missing post probe with a present pre probe means the probe was
    /// attempted but could not observe the backing node; that maps to
    /// `Unobserved` with unobserved after-facts. A disconnected post probe
    /// maps to `DetachedOrReplaced` and keeps its readable state out of the
    /// after-facts: a detached node's state no longer describes the current
    /// document. Side-channel deltas start absent; the session layer attaches
    /// them after target reconciliation.
    pub fn from_facts(
        pre: Option<&NodeStateFacts>,
        post: Option<&NodeStateFacts>,
        url_changed: Option<bool>,
        navigation_lifecycle_observed: bool,
        main_frame_navigation_observed: Option<bool>,
        signals: SideChannelSignals,
    ) -> Self {
        let target = match pre {
            None => TargetPostcondition::unobserved(TargetNodeOutcome::NotEvaluated),
            Some(pre) => {
                let (node, post) = match post {
                    None => (TargetNodeOutcome::Unobserved, None),
                    Some(post) if post.connected => (TargetNodeOutcome::Present, Some(post)),
                    Some(_) => (TargetNodeOutcome::DetachedOrReplaced, None),
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
                main_frame_navigation_observed,
            },
            target,
            signals,
            new_pages: None,
            downloads: None,
            clipboard_write_confirmed: None,
        }
    }

    /// All facts unobserved (pre-dispatch-only records, download records).
    pub const fn unobserved() -> Self {
        Self {
            page: PagePostcondition {
                url_changed: None,
                navigation_lifecycle_observed: false,
                main_frame_navigation_observed: None,
            },
            target: TargetPostcondition::unobserved(TargetNodeOutcome::NotEvaluated),
            signals: SideChannelSignals::unobserved(),
            new_pages: None,
            downloads: None,
            clipboard_write_confirmed: None,
        }
    }

    /// WriteClipboard record block: [`Self::unobserved`] plus the confirmed
    /// in-page write fact.
    pub const fn clipboard_confirmed() -> Self {
        let mut value = Self::unobserved();
        value.clipboard_write_confirmed = Some(true);
        value
    }

    /// Session-layer attachment after post-dispatch target reconciliation.
    pub fn attach_new_pages(&mut self, facts: NewPagePostcondition) {
        self.new_pages = Some(facts);
    }

    /// Session-layer attachment after the download-authority delta read.
    pub fn attach_downloads(&mut self, facts: DownloadPostcondition) {
        self.downloads = Some(facts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

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

    fn page_fact(sequence: u64) -> NewPageFact {
        NewPageFact {
            target_id: TargetId::from_uuid(Uuid::from_u128(u128::from(sequence))),
            sequence: PageSequence::new(sequence).unwrap(),
            opener_matched: true,
        }
    }

    fn download_fact(sequence: u64) -> DownloadFact {
        DownloadFact {
            download_id: DownloadId::from_uuid(Uuid::from_u128(u128::from(sequence))),
            sequence: DownloadSequence::new(sequence).unwrap(),
            state: DownloadState::InProgress,
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
        let value = InteractionPostcondition::from_facts(
            Some(&pre),
            Some(&post),
            Some(false),
            false,
            Some(false),
            SideChannelSignals::unobserved(),
        );
        assert_eq!(value.target.node, TargetNodeOutcome::Present);
        assert_eq!(value.target.checked.changed, Some(true));
        assert_eq!(value.target.expanded.changed, Some(false));
        assert_eq!(value.target.selected, FlagObservation::unobserved());
        assert_eq!(value.target.value_length_changed, Some(true));
        assert_eq!(value.page.url_changed, Some(false));
        assert!(!value.page.navigation_lifecycle_observed);
        assert_eq!(value.page.main_frame_navigation_observed, Some(false));
        assert_eq!(value.new_pages, None);
        assert_eq!(value.downloads, None);
        assert_eq!(value.clipboard_write_confirmed, None);
    }

    #[test]
    fn from_facts_missing_post_probe_is_unobserved_not_a_detachment_claim() {
        let pre = facts(Some(true), Some(4));
        let value = InteractionPostcondition::from_facts(
            Some(&pre),
            None,
            None,
            true,
            None,
            SideChannelSignals::unobserved(),
        );
        assert_eq!(value.target.node, TargetNodeOutcome::Unobserved);
        assert_eq!(value.target.checked.before, Some(true));
        assert_eq!(value.target.checked.after, None);
        assert_eq!(value.target.checked.changed, None);
        assert_eq!(value.target.value_length_changed, None);
        assert_eq!(value.page.url_changed, None);
        assert!(value.page.navigation_lifecycle_observed);
        assert_eq!(value.page.main_frame_navigation_observed, None);
    }

    #[test]
    fn from_facts_disconnected_post_probe_is_the_only_detachment_claim() {
        let pre = facts(Some(false), Some(0));
        let post = NodeStateFacts {
            connected: false,
            checked: Some(true),
            ..NodeStateFacts::default()
        };
        let value = InteractionPostcondition::from_facts(
            Some(&pre),
            Some(&post),
            None,
            false,
            None,
            SideChannelSignals::unobserved(),
        );
        assert_eq!(value.target.node, TargetNodeOutcome::DetachedOrReplaced);
        assert_eq!(value.target.checked.after, None);
        assert_eq!(value.target.checked.changed, None);
    }

    #[test]
    fn from_facts_without_a_pre_probe_never_evaluates_the_target() {
        for post in [None, Some(facts(Some(true), Some(1)))] {
            let value = InteractionPostcondition::from_facts(
                None,
                post.as_ref(),
                Some(true),
                true,
                Some(true),
                SideChannelSignals::unobserved(),
            );
            assert_eq!(value.target.node, TargetNodeOutcome::NotEvaluated);
            assert_eq!(value.target.checked, FlagObservation::unobserved());
            assert_eq!(value.target.value_length_changed, None);
            assert_eq!(value.page.url_changed, Some(true));
            assert!(value.page.navigation_lifecycle_observed);
            assert_eq!(value.page.main_frame_navigation_observed, Some(true));
        }
    }

    #[test]
    fn side_channel_constructors_cap_lists_with_exact_omission_counts() {
        let cursor = PageSequence::new(3).unwrap();
        let observed = (1..=6).map(page_fact).collect::<Vec<_>>();
        let capped = NewPagePostcondition::from_observed(cursor, observed.clone());
        assert_eq!(capped.cursor_before, cursor);
        assert_eq!(capped.pages, observed[..MAX_SIDE_CHANNEL_FACTS]);
        assert_eq!(capped.omitted, 2);

        let exact = NewPagePostcondition::from_observed(cursor, observed[..2].to_vec());
        assert_eq!(exact.pages.len(), 2);
        assert_eq!(exact.omitted, 0);

        let empty = NewPagePostcondition::from_observed(cursor, Vec::new());
        assert!(empty.pages.is_empty());
        assert_eq!(empty.omitted, 0);

        let cursor = DownloadSequence::new(2).unwrap();
        let observed = (1..=5).map(download_fact).collect::<Vec<_>>();
        let capped = DownloadPostcondition::from_observed(cursor, observed.clone());
        assert_eq!(capped.downloads, observed[..MAX_SIDE_CHANNEL_FACTS]);
        assert_eq!(capped.omitted, 1);
    }

    #[test]
    fn wire_decoding_rejects_over_cap_side_channel_lists() {
        let over_cap_pages = json!({
            "cursor_before": 1,
            "pages": (1..=(MAX_SIDE_CHANNEL_FACTS as u64 + 1))
                .map(|sequence| serde_json::to_value(page_fact(sequence)).unwrap())
                .collect::<Vec<_>>(),
            "omitted": 0,
        });
        assert!(serde_json::from_value::<NewPagePostcondition>(over_cap_pages).is_err());

        let over_cap_downloads = json!({
            "cursor_before": 1,
            "downloads": (1..=(MAX_SIDE_CHANNEL_FACTS as u64 + 1))
                .map(|sequence| serde_json::to_value(download_fact(sequence)).unwrap())
                .collect::<Vec<_>>(),
            "omitted": 0,
        });
        assert!(serde_json::from_value::<DownloadPostcondition>(over_cap_downloads).is_err());

        // A capped list with its exact omission count round-trips.
        let capped = NewPagePostcondition::from_observed(
            PageSequence::new(1).unwrap(),
            (1..=6).map(page_fact).collect(),
        );
        let decoded: NewPagePostcondition =
            serde_json::from_value(serde_json::to_value(&capped).unwrap()).unwrap();
        assert_eq!(decoded, capped);
    }

    #[test]
    fn populated_and_unobserved_blocks_round_trip_through_serde() {
        let mut populated = InteractionPostcondition::from_facts(
            Some(&facts(Some(false), Some(0))),
            Some(&facts(Some(true), Some(4))),
            Some(false),
            true,
            Some(true),
            SideChannelSignals {
                window_open_attempts: Some(2),
                download_requests: Some(1),
            },
        );
        populated.attach_new_pages(NewPagePostcondition::from_observed(
            PageSequence::new(2).unwrap(),
            vec![page_fact(3)],
        ));
        populated.attach_downloads(DownloadPostcondition::from_observed(
            DownloadSequence::new(1).unwrap(),
            vec![download_fact(2)],
        ));
        let decoded: InteractionPostcondition =
            serde_json::from_str(&serde_json::to_string(&populated).unwrap()).unwrap();
        assert_eq!(decoded, populated);

        let unobserved = InteractionPostcondition::unobserved();
        let decoded: InteractionPostcondition =
            serde_json::from_str(&serde_json::to_string(&unobserved).unwrap()).unwrap();
        assert_eq!(decoded, unobserved);
        assert_eq!(decoded.page.url_changed, None);
        assert_eq!(decoded.page.main_frame_navigation_observed, None);
        assert_eq!(decoded.target.node, TargetNodeOutcome::NotEvaluated);
        assert_eq!(decoded.signals, SideChannelSignals::unobserved());

        let clipboard = InteractionPostcondition::clipboard_confirmed();
        let decoded: InteractionPostcondition =
            serde_json::from_str(&serde_json::to_string(&clipboard).unwrap()).unwrap();
        assert_eq!(decoded.clipboard_write_confirmed, Some(true));
        assert_eq!(decoded.target.node, TargetNodeOutcome::NotEvaluated);
    }

    fn link_expectation() -> InteractionExpectation {
        InteractionExpectation {
            target: ExpectationTarget::Role(ExpectationTargetRole::Link),
            required_channels: &[
                ExpectationChannel::Navigation,
                ExpectationChannel::NewPage,
                ExpectationChannel::Download,
            ],
            note: ExpectationNote::NavigationOutcomeUnobserved,
        }
    }

    fn link_facts(
        navigation: Option<bool>,
        new_page: Option<bool>,
        download: Option<bool>,
    ) -> InteractionPostcondition {
        let mut facts = InteractionPostcondition::from_facts(
            None,
            None,
            None,
            false,
            navigation,
            SideChannelSignals::unobserved(),
        );
        if let Some(changed) = new_page {
            facts.attach_new_pages(NewPagePostcondition::from_observed(
                PageSequence::new(1).unwrap(),
                changed.then(|| page_fact(2)).into_iter().collect(),
            ));
        }
        if let Some(changed) = download {
            facts.attach_downloads(DownloadPostcondition::from_observed(
                DownloadSequence::new(1).unwrap(),
                changed.then(|| download_fact(2)).into_iter().collect(),
            ));
        }
        facts
    }

    #[test]
    fn expectation_truth_table_requires_complete_unchanged_channels() {
        let expectation = link_expectation();
        for navigation in [None, Some(false), Some(true)] {
            for new_page in [None, Some(false), Some(true)] {
                for download in [None, Some(false), Some(true)] {
                    let evaluation = evaluate_expectations(
                        &[expectation],
                        Some(ExpectationTargetRole::Link),
                        &link_facts(navigation, new_page, download),
                    );
                    let expected = if navigation == Some(true)
                        || new_page == Some(true)
                        || download == Some(true)
                    {
                        ExpectationEvaluation::Held
                    } else if navigation == Some(false)
                        && new_page == Some(false)
                        && download == Some(false)
                    {
                        ExpectationEvaluation::DidNotHold(
                            ExpectationNote::NavigationOutcomeUnobserved,
                        )
                    } else {
                        ExpectationEvaluation::NotEvaluated
                    };
                    assert_eq!(
                        evaluation, expected,
                        "{navigation:?} {new_page:?} {download:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn observed_side_channel_attempts_demote_empty_inventories() {
        let expectation = link_expectation();

        let mut window_attempt = link_facts(Some(false), Some(false), Some(false));
        window_attempt.signals.window_open_attempts = Some(1);
        assert_eq!(
            evaluate_expectations(
                &[expectation],
                Some(ExpectationTargetRole::Link),
                &window_attempt,
            ),
            ExpectationEvaluation::NotEvaluated
        );

        let mut window_attempt_with_navigation = link_facts(Some(true), Some(false), Some(false));
        window_attempt_with_navigation.signals.window_open_attempts = Some(1);
        assert_eq!(
            evaluate_expectations(
                &[expectation],
                Some(ExpectationTargetRole::Link),
                &window_attempt_with_navigation,
            ),
            ExpectationEvaluation::Held
        );

        let mut download_attempt = link_facts(Some(false), Some(false), Some(false));
        download_attempt.signals.download_requests = Some(1);
        assert_eq!(
            evaluate_expectations(
                &[expectation],
                Some(ExpectationTargetRole::Link),
                &download_attempt,
            ),
            ExpectationEvaluation::NotEvaluated
        );

        assert_eq!(
            evaluate_expectations(
                &[expectation],
                Some(ExpectationTargetRole::Link),
                &link_facts(Some(false), Some(false), Some(false)),
            ),
            ExpectationEvaluation::DidNotHold(ExpectationNote::NavigationOutcomeUnobserved)
        );
    }

    #[test]
    fn expectation_navigation_uses_committed_signal_and_url_positive_fallback() {
        let expectation = link_expectation();
        let positive_signal = evaluate_expectations(
            &[expectation],
            Some(ExpectationTargetRole::Link),
            &link_facts(Some(true), Some(false), Some(false)),
        );
        assert_eq!(positive_signal, ExpectationEvaluation::Held);

        let mut url_fallback = link_facts(None, Some(false), Some(false));
        url_fallback.page.url_changed = Some(true);
        assert_eq!(
            evaluate_expectations(
                &[expectation],
                Some(ExpectationTargetRole::Link),
                &url_fallback,
            ),
            ExpectationEvaluation::Held
        );

        let mut url_only_unchanged = link_facts(None, Some(false), Some(false));
        url_only_unchanged.page.url_changed = Some(false);
        assert_eq!(
            evaluate_expectations(
                &[expectation],
                Some(ExpectationTargetRole::Link),
                &url_only_unchanged,
            ),
            ExpectationEvaluation::NotEvaluated
        );
    }

    #[test]
    fn target_expectations_gate_on_a_present_node_and_role() {
        let checked = InteractionExpectation {
            target: ExpectationTarget::Role(ExpectationTargetRole::Checkbox),
            required_channels: &[ExpectationChannel::Checked],
            note: ExpectationNote::CheckedStateUnchanged,
        };
        let unchanged = InteractionPostcondition::from_facts(
            Some(&facts(Some(false), Some(2))),
            Some(&facts(Some(false), Some(2))),
            Some(false),
            false,
            Some(false),
            SideChannelSignals::unobserved(),
        );
        assert_eq!(
            evaluate_expectations(
                &[checked],
                Some(ExpectationTargetRole::Checkbox),
                &unchanged,
            ),
            ExpectationEvaluation::DidNotHold(ExpectationNote::CheckedStateUnchanged)
        );
        assert_eq!(
            evaluate_expectations(&[checked], None, &unchanged),
            ExpectationEvaluation::NotEvaluated
        );

        let detached = InteractionPostcondition::from_facts(
            Some(&facts(Some(false), Some(2))),
            Some(&NodeStateFacts {
                connected: false,
                checked: Some(false),
                value_length: Some(2),
                ..NodeStateFacts::default()
            }),
            Some(false),
            false,
            Some(false),
            SideChannelSignals::unobserved(),
        );
        assert_eq!(
            evaluate_expectations(&[checked], Some(ExpectationTargetRole::Checkbox), &detached,),
            ExpectationEvaluation::NotEvaluated
        );
    }

    #[test]
    fn first_matching_expectation_does_not_fall_through() {
        let click_expectations = [
            link_expectation(),
            InteractionExpectation {
                target: ExpectationTarget::AnyObservedRole,
                required_channels: &[ExpectationChannel::Expanded],
                note: ExpectationNote::ExpandedStateUnchanged,
            },
        ];
        let mut facts = link_facts(None, Some(false), Some(false));
        facts.target = TargetPostcondition {
            node: TargetNodeOutcome::Present,
            expanded: FlagObservation::observed(Some(false), Some(false)),
            ..TargetPostcondition::unobserved(TargetNodeOutcome::Present)
        };
        assert_eq!(
            evaluate_expectations(
                &click_expectations,
                Some(ExpectationTargetRole::Link),
                &facts,
            ),
            ExpectationEvaluation::NotEvaluated
        );
    }

    #[test]
    fn single_channel_rules_and_unavailable_roles_are_conservative() {
        let fill = InteractionExpectation {
            target: ExpectationTarget::AnyObservedRole,
            required_channels: &[ExpectationChannel::ValueLength],
            note: ExpectationNote::ValueLengthUnchanged,
        };
        let value_unchanged = InteractionPostcondition::from_facts(
            Some(&facts(None, Some(4))),
            Some(&facts(None, Some(4))),
            None,
            false,
            None,
            SideChannelSignals::unobserved(),
        );
        assert_eq!(
            evaluate_expectations(
                &[fill],
                Some(ExpectationTargetRole::Other),
                &value_unchanged,
            ),
            ExpectationEvaluation::DidNotHold(ExpectationNote::ValueLengthUnchanged)
        );
        assert_eq!(
            evaluate_expectations(&[fill], None, &value_unchanged),
            ExpectationEvaluation::NotEvaluated
        );

        let selected = InteractionExpectation {
            target: ExpectationTarget::AnyObservedRole,
            required_channels: &[ExpectationChannel::Selected],
            note: ExpectationNote::SelectedStateUnchanged,
        };
        assert_eq!(
            evaluate_expectations(
                &[selected],
                Some(ExpectationTargetRole::Other),
                &value_unchanged
            ),
            ExpectationEvaluation::NotEvaluated
        );
        assert_eq!(
            normalized_channel_observation(ExpectationChannel::Checked, false, &value_unchanged),
            ChannelObservation::NotApplicable
        );
    }

    #[test]
    fn expectation_roles_and_closed_messages_are_stable() {
        assert_eq!(
            ExpectationTargetRole::from_accessibility_role("CHECKBOX"),
            ExpectationTargetRole::Checkbox
        );
        assert_eq!(
            ExpectationTargetRole::from_accessibility_role("button"),
            ExpectationTargetRole::Other
        );
        assert_eq!(
            ExpectationNote::NavigationOutcomeUnobserved.message(),
            "No navigation, new page, or download was observed by the observation point."
        );
        assert_eq!(
            ExpectationNote::CheckedStateUnchanged.message(),
            "The target's checked state was unchanged by the observation point."
        );
    }
}

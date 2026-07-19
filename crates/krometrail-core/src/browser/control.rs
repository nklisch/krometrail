use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    ErrorContext, EveryNthFrame, InteractionId, KrometrailError, NonEmptyText, Result,
    RetentionStatus, SessionId, SessionTime, TargetCaptureStatus, TargetId, error::invalid,
    validation::deserialize_validated,
};

use super::{
    BROWSER_OPERATION_REGISTRY, BrowserCompatibility, BrowserOperationKind, BrowserOwnership,
    BrowserSessionState, LiveObservation, ObservationPart, OperationMutability, ProfileRef,
    SupervisedTarget,
};

pub const DEFAULT_MANAGED_PROFILE_NAME: &str = "default";

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "selection", content = "target_id", rename_all = "snake_case")]
pub enum PageSelection {
    #[default]
    Selected,
    Target(TargetId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageStatus {
    pub target: SupervisedTarget,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserStatus {
    pub session_id: SessionId,
    pub state: BrowserSessionState,
    pub ownership: BrowserOwnership,
    pub profile: ProfileRef,
    pub compatibility: BrowserCompatibility,
    pub selected_target_id: Option<TargetId>,
    pub pages: Vec<PageStatus>,
    pub capture: Vec<TargetCaptureStatus>,
    pub retention: RetentionStatus,
    pub every_nth_frame: EveryNthFrame,
}

#[derive(Deserialize)]
struct BrowserStatusWire {
    session_id: SessionId,
    state: BrowserSessionState,
    ownership: BrowserOwnership,
    profile: ProfileRef,
    compatibility: BrowserCompatibility,
    selected_target_id: Option<TargetId>,
    pages: Vec<PageStatus>,
    capture: Vec<TargetCaptureStatus>,
    retention: RetentionStatus,
    every_nth_frame: EveryNthFrame,
}

impl BrowserStatus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        state: BrowserSessionState,
        ownership: BrowserOwnership,
        profile: ProfileRef,
        compatibility: BrowserCompatibility,
        selected_target_id: Option<TargetId>,
        pages: Vec<PageStatus>,
        capture: Vec<TargetCaptureStatus>,
        retention: RetentionStatus,
        every_nth_frame: EveryNthFrame,
    ) -> Result<Self> {
        let mut ids = HashSet::new();
        let selected = pages
            .iter()
            .filter(|page| page.selected)
            .collect::<Vec<_>>();
        if pages
            .iter()
            .any(|page| !ids.insert(page.target.target.id()))
        {
            return Err(invalid("browser status pages must have unique target ids"));
        }
        match (selected_target_id, selected.as_slice()) {
            (None, []) => {}
            (Some(id), [page]) if page.target.target.id() == id => {}
            _ => return Err(invalid("browser status selection does not match its pages")),
        }
        if state == BrowserSessionState::Ended && selected_target_id.is_some() {
            return Err(invalid("ended browser status cannot select an active page"));
        }
        Ok(Self {
            session_id,
            state,
            ownership,
            profile,
            compatibility,
            selected_target_id,
            pages,
            capture,
            retention,
            every_nth_frame,
        })
    }

    pub const fn every_nth_frame(&self) -> EveryNthFrame {
        self.every_nth_frame
    }
}

impl<'de> Deserialize<'de> for BrowserStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: BrowserStatusWire| {
            Self::new(
                w.session_id,
                w.state,
                w.ownership,
                w.profile,
                w.compatibility,
                w.selected_target_id,
                w.pages,
                w.capture,
                w.retention,
                w.every_nth_frame,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ListPagesRequest {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatePageRequest {
    pub initial_url: Option<NonEmptyText>,
}

impl CreatePageRequest {
    pub fn new(initial_url: Option<impl Into<String>>) -> Result<Self> {
        Ok(Self {
            initial_url: initial_url
                .map(|url| NonEmptyText::new(url.into()))
                .transpose()
                .map_err(|_| invalid("initial page URL must not be empty"))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SelectPageRequest {
    pub target_id: TargetId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClosePageRequest {
    #[serde(default)]
    pub target: PageSelection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NavigatePageRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub url: NonEmptyText,
}

impl NavigatePageRequest {
    pub fn new(target: PageSelection, url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            target,
            url: NonEmptyText::new(url.into())
                .map_err(|_| invalid("navigation URL must not be empty"))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReloadPageRequest {
    #[serde(default)]
    pub target: PageSelection,
    #[serde(default)]
    pub bypass_cache: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GoBackRequest {
    #[serde(default)]
    pub target: PageSelection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GoForwardRequest {
    #[serde(default)]
    pub target: PageSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct InteractionTiming {
    pub started_at: SessionTime,
    pub dispatched_at: SessionTime,
    pub completed_at: SessionTime,
    pub observed_at: Option<SessionTime>,
}

#[derive(Deserialize)]
struct InteractionTimingWire {
    started_at: SessionTime,
    dispatched_at: SessionTime,
    completed_at: SessionTime,
    observed_at: Option<SessionTime>,
}

impl InteractionTiming {
    pub fn new(
        started_at: SessionTime,
        dispatched_at: SessionTime,
        completed_at: SessionTime,
        observed_at: Option<SessionTime>,
    ) -> Result<Self> {
        if started_at > dispatched_at
            || dispatched_at > completed_at
            || observed_at.is_some_and(|observed| observed < completed_at)
        {
            return Err(invalid("interaction times must be monotonically ordered"));
        }
        Ok(Self {
            started_at,
            dispatched_at,
            completed_at,
            observed_at,
        })
    }
}

impl<'de> Deserialize<'de> for InteractionTiming {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: InteractionTimingWire| {
            Self::new(w.started_at, w.dispatched_at, w.completed_at, w.observed_at)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct InteractionAnchor {
    pub interaction_id: InteractionId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub operation: BrowserOperationKind,
    pub timing: InteractionTiming,
}

#[derive(Deserialize)]
struct InteractionAnchorWire {
    interaction_id: InteractionId,
    session_id: SessionId,
    target_id: TargetId,
    operation: BrowserOperationKind,
    timing: InteractionTiming,
}

impl InteractionAnchor {
    pub fn new(
        interaction_id: InteractionId,
        session_id: SessionId,
        target_id: TargetId,
        operation: BrowserOperationKind,
        timing: InteractionTiming,
    ) -> Result<Self> {
        let definition = BROWSER_OPERATION_REGISTRY
            .iter()
            .find(|definition| definition.kind == operation)
            .ok_or_else(|| invalid("interaction operation is not registered"))?;
        if definition.mutability != OperationMutability::StateChanging {
            return Err(invalid("interaction operation must be state-changing"));
        }
        Ok(Self {
            interaction_id,
            session_id,
            target_id,
            operation,
            timing,
        })
    }
}

impl<'de> Deserialize<'de> for InteractionAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: InteractionAnchorWire| {
            Self::new(
                w.interaction_id,
                w.session_id,
                w.target_id,
                w.operation,
                w.timing,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum PageChange {
    Created {
        target_id: TargetId,
    },
    Selected {
        previous: Option<TargetId>,
        selected: TargetId,
    },
    Closed {
        closed: TargetId,
        selected: Option<TargetId>,
    },
    Navigated,
    Reloaded,
    WentBack,
    WentForward,
    ViewportConfigured {
        override_active: bool,
    },
    ClipboardWritten,
}

impl PageChange {
    fn mutated_target(&self, anchor: TargetId) -> TargetId {
        match *self {
            Self::Created { target_id } => target_id,
            Self::Selected { selected, .. } => selected,
            Self::Closed { closed, .. } => closed,
            Self::Navigated
            | Self::Reloaded
            | Self::WentBack
            | Self::WentForward
            | Self::ViewportConfigured { .. }
            | Self::ClipboardWritten => anchor,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum PageOperationOutcome {
    Succeeded(PageChange),
    Failed(KrometrailError),
}

impl PageOperationOutcome {
    pub fn failed(mut error: KrometrailError, interaction: &InteractionAnchor) -> Self {
        error.context = ErrorContext {
            session_id: Some(interaction.session_id),
            target_id: Some(interaction.target_id),
            interaction_id: Some(interaction.interaction_id),
            range: error.context.range,
        };
        Self::Failed(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageOperationResult {
    pub interaction: InteractionAnchor,
    pub outcome: PageOperationOutcome,
    pub observation: ObservationPart<LiveObservation>,
}

impl PageOperationResult {
    pub fn new(
        interaction: InteractionAnchor,
        outcome: PageOperationOutcome,
        observation: ObservationPart<LiveObservation>,
    ) -> Result<Self> {
        if let PageOperationOutcome::Succeeded(change) = &outcome {
            if change.mutated_target(interaction.target_id) != interaction.target_id {
                return Err(invalid(
                    "page change target does not match interaction target",
                ));
            }
        }
        Ok(Self {
            interaction,
            outcome,
            observation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserProduct, BrowserProductVersion, BrowserVersion, CapabilitySupport,
        RendererCapability,
    };
    use uuid::Uuid;

    fn id(n: u128) -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(n))
    }
    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(10))
    }
    fn interaction() -> InteractionId {
        InteractionId::from_uuid(Uuid::from_u128(11))
    }
    fn compatibility() -> BrowserCompatibility {
        BrowserCompatibility::new(
            BrowserVersion::new(
                BrowserProduct::Chrome,
                BrowserProductVersion::new("1").unwrap(),
                "r",
                "1.3",
                "ua",
                "js",
            )
            .unwrap(),
            RendererCapability::ALL
                .iter()
                .map(|capability| CapabilitySupport::new(*capability, true, true, None).unwrap())
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn profile_and_page_inputs_reject_malformed_wire_values() {
        assert!(CreatePageRequest::new(Some(" ")).is_err());
        assert!(NavigatePageRequest::new(PageSelection::Selected, "").is_err());
        assert!(
            serde_json::from_str::<NavigatePageRequest>(
                r#"{"target":{"selection":"selected"},"url":" "}"#
            )
            .is_err()
        );
    }

    #[test]
    fn interaction_contract_rejects_read_only_and_unordered_values() {
        assert!(
            InteractionTiming::new(
                SessionTime::from_nanos(2),
                SessionTime::from_nanos(1),
                SessionTime::from_nanos(3),
                None
            )
            .is_err()
        );
        let timing = InteractionTiming::new(
            SessionTime::ZERO,
            SessionTime::from_nanos(1),
            SessionTime::from_nanos(2),
            None,
        )
        .unwrap();
        assert!(
            InteractionAnchor::new(
                interaction(),
                session(),
                id(1),
                BrowserOperationKind::InspectPage,
                timing
            )
            .is_err()
        );
        let anchor = InteractionAnchor::new(
            interaction(),
            session(),
            id(1),
            BrowserOperationKind::NavigatePage,
            timing,
        )
        .unwrap();
        assert!(
            PageOperationResult::new(
                anchor,
                PageOperationOutcome::Succeeded(PageChange::Created { target_id: id(2) }),
                ObservationPart::Unavailable(invalid("none"))
            )
            .is_err()
        );
    }

    #[test]
    fn status_selection_is_coherent_at_constructor_and_wire_boundary() {
        let status = BrowserStatus::new(
            session(),
            BrowserSessionState::Ready,
            BrowserOwnership::Attached,
            ProfileRef::External,
            compatibility(),
            None,
            vec![],
            vec![],
            RetentionStatus::empty(crate::DiskBudgetBytes::default()),
            EveryNthFrame::new(17).unwrap(),
        )
        .unwrap();
        assert_eq!(status.every_nth_frame().get(), 17);
        assert_eq!(
            serde_json::from_str::<BrowserStatus>(&serde_json::to_string(&status).unwrap())
                .unwrap(),
            status
        );
        let mut value = serde_json::to_value(status).unwrap();
        value["selected_target_id"] = serde_json::json!(id(1));
        assert!(serde_json::from_value::<BrowserStatus>(value).is_err());
    }
}

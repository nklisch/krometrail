use serde::{Deserialize, Serialize};

use crate::CapabilityId;

use super::{
    AcceptedLocator, ActionCategory, ActionDefinition, ActionabilityRequirement, BatchRequest,
    BatchResult, ClickRequest, ClosePageRequest, CompletionKind, CreatePageRequest, DragRequest,
    EncodedScreenshot, EvaluationResult, FillRequest, GoBackRequest, GoForwardRequest,
    HandleDialogRequest, HoverRequest, InspectPageRequest, InteractionResult, ListPagesRequest,
    LiveObservation, LiveObservationRequest, NavigatePageRequest, PageOperationResult,
    PageSelection, PageSnapshot, PageState, PageStatus, PressKeysRequest,
    ReadOnlyEvaluationRequest, ReloadPageRequest, ScreenshotRequest, ScrollRequest,
    SelectOptionRequest, SelectPageRequest, SnapshotPageRequest, UploadFilesRequest, WaitRequest,
    WaitResult,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationMutability {
    ReadOnly,
    StateChanging,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationEvidence {
    RequestedOnly,
    LiveObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserOperationScopeKind {
    Browser,
    Page,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserOperationScope {
    Browser,
    Page(PageSelection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserOperationDefinition {
    pub kind: BrowserOperationKind,
    pub stable_name: &'static str,
    pub description: &'static str,
    pub capability: CapabilityId,
    pub mutability: OperationMutability,
    pub evidence: OperationEvidence,
    pub scope: BrowserOperationScopeKind,
    pub batchable: bool,
    pub action: Option<&'static ActionDefinition>,
}

trait PageScopedRequest {
    fn page_selection(&self) -> PageSelection;
}

macro_rules! selected_field {
    ($type:ty, $field:ident) => {
        impl PageScopedRequest for $type {
            fn page_selection(&self) -> PageSelection {
                self.$field
            }
        }
    };
}
selected_field!(InspectPageRequest, target);
selected_field!(SnapshotPageRequest, target);
selected_field!(LiveObservationRequest, target);
selected_field!(ReadOnlyEvaluationRequest, target);
selected_field!(ScreenshotRequest, page);
selected_field!(ClosePageRequest, target);
selected_field!(NavigatePageRequest, target);
selected_field!(ReloadPageRequest, target);
selected_field!(GoBackRequest, target);
selected_field!(GoForwardRequest, target);
selected_field!(ClickRequest, target);
selected_field!(FillRequest, target);
selected_field!(PressKeysRequest, target);
selected_field!(SelectOptionRequest, target);
selected_field!(HoverRequest, target);
selected_field!(DragRequest, target);
selected_field!(ScrollRequest, target);
selected_field!(UploadFilesRequest, target);
selected_field!(HandleDialogRequest, target);
selected_field!(WaitRequest, target);
selected_field!(BatchRequest, target);

macro_rules! operation_scope {
    (Browser, $request:ident) => {{
        let _ = $request;
        BrowserOperationScope::Browser
    }};
    (Page, $request:ident) => {
        BrowserOperationScope::Page($request.page_selection())
    };
}

macro_rules! define_browser_operations {
    ($(
        $variant:ident($request:ty) => $result:ty {
            stable_name: $stable_name:literal,
            description: $description:literal,
            mutability: $mutability:ident,
            evidence: $evidence:ident,
            scope: $scope:ident,
            batchable: $batchable:literal,
            action: $action:expr,
        }
    ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
        pub enum BrowserOperationKind {
            $(#[serde(rename = $stable_name)] $variant),+
        }

        impl BrowserOperationKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn stable_name(self) -> &'static str {
                match self { $(Self::$variant => $stable_name),+ }
            }
            pub fn input_schema(self) -> schemars::Schema {
                match self { $(Self::$variant => schemars::schema_for!($request)),+ }
            }
            pub fn is_interaction(self) -> bool {
                BROWSER_OPERATION_REGISTRY
                    .iter()
                    .find(|definition| definition.kind == self)
                    .is_some_and(|definition| definition.action.is_some())
            }
        }

        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(tag = "operation", content = "request")]
        pub enum BrowserOperationRequest {
            $(#[serde(rename = $stable_name)] $variant($request)),+
        }

        impl BrowserOperationRequest {
            pub const fn kind(&self) -> BrowserOperationKind {
                match self { $(Self::$variant(_) => BrowserOperationKind::$variant),+ }
            }
            pub const fn stable_name(&self) -> &'static str { self.kind().stable_name() }
            pub fn scope(&self) -> BrowserOperationScope {
                match self { $(Self::$variant(request) => operation_scope!($scope, request)),+ }
            }
        }

        #[derive(Clone, Debug, PartialEq)]
        pub enum BrowserOperationResult { $($variant(Box<$result>)),+ }

        impl BrowserOperationResult {
            pub const fn kind(&self) -> BrowserOperationKind {
                match self { $(Self::$variant(_) => BrowserOperationKind::$variant),+ }
            }
        }

        pub static BROWSER_OPERATION_REGISTRY: &[BrowserOperationDefinition] = &[
            $(BrowserOperationDefinition {
                kind: BrowserOperationKind::$variant,
                stable_name: $stable_name,
                description: $description,
                capability: CapabilityId::Control,
                mutability: OperationMutability::$mutability,
                evidence: OperationEvidence::$evidence,
                scope: BrowserOperationScopeKind::$scope,
                batchable: $batchable,
                action: $action,
            }),+
        ];
    };
}

const ACTION_CLICK: ActionDefinition = ActionDefinition {
    category: ActionCategory::Pointer,
    actionability: ActionabilityRequirement::Actionable,
    locator: AcceptedLocator::ElementOrCoordinate,
    completion: CompletionKind::Settled,
    display_name: "Click",
};
const ACTION_FILL: ActionDefinition = ActionDefinition {
    category: ActionCategory::Form,
    actionability: ActionabilityRequirement::Editable,
    locator: AcceptedLocator::Element,
    completion: CompletionKind::Settled,
    display_name: "Fill",
};
const ACTION_PRESS_KEYS: ActionDefinition = ActionDefinition {
    category: ActionCategory::Keyboard,
    actionability: ActionabilityRequirement::Actionable,
    locator: AcceptedLocator::OptionalElement,
    completion: CompletionKind::Settled,
    display_name: "Press keys",
};
const ACTION_SELECT: ActionDefinition = ActionDefinition {
    category: ActionCategory::Form,
    actionability: ActionabilityRequirement::Selectable,
    locator: AcceptedLocator::Element,
    completion: CompletionKind::Settled,
    display_name: "Select option",
};
const ACTION_HOVER: ActionDefinition = ActionDefinition {
    category: ActionCategory::Pointer,
    actionability: ActionabilityRequirement::VisibleGeometry,
    locator: AcceptedLocator::ElementOrCoordinate,
    completion: CompletionKind::Settled,
    display_name: "Hover",
};
const ACTION_DRAG: ActionDefinition = ActionDefinition {
    category: ActionCategory::DragDrop,
    actionability: ActionabilityRequirement::Actionable,
    locator: AcceptedLocator::ElementOrCoordinate,
    completion: CompletionKind::Settled,
    display_name: "Drag",
};
const ACTION_SCROLL: ActionDefinition = ActionDefinition {
    category: ActionCategory::Scroll,
    actionability: ActionabilityRequirement::Actionable,
    locator: AcceptedLocator::OptionalElement,
    completion: CompletionKind::InputAcknowledged,
    display_name: "Scroll",
};
const ACTION_UPLOAD: ActionDefinition = ActionDefinition {
    category: ActionCategory::FileDialog,
    actionability: ActionabilityRequirement::FileInput,
    locator: AcceptedLocator::Element,
    completion: CompletionKind::InputAcknowledged,
    display_name: "Upload files",
};
const ACTION_DIALOG: ActionDefinition = ActionDefinition {
    category: ActionCategory::Dialog,
    actionability: ActionabilityRequirement::None,
    locator: AcceptedLocator::None,
    // Chrome acknowledges the dialog command before the page's blocked callback necessarily
    // resumes. A renderer task checkpoint is required before the live observation is trustworthy.
    completion: CompletionKind::Settled,
    display_name: "Handle dialog",
};

define_browser_operations! {
    InspectPage(InspectPageRequest) => PageState {
        stable_name: "inspect_page", description: "Inspect the current page URL, title, viewport, and navigation state.", mutability: ReadOnly, evidence: RequestedOnly, scope: Page, batchable: true, action: None,
    },
    SnapshotPage(SnapshotPageRequest) => PageSnapshot {
        stable_name: "snapshot_page", description: "Capture a structured accessibility snapshot with actionable references.", mutability: ReadOnly, evidence: RequestedOnly, scope: Page, batchable: true, action: None,
    },
    TakeScreenshot(ScreenshotRequest) => EncodedScreenshot {
        stable_name: "take_screenshot", description: "Capture the requested viewport, page, element, or region image.", mutability: ReadOnly, evidence: RequestedOnly, scope: Page, batchable: true, action: None,
    },
    EvaluatePage(ReadOnlyEvaluationRequest) => EvaluationResult {
        stable_name: "evaluate_page", description: "Evaluate a bounded read-only JavaScript expression in the page.", mutability: ReadOnly, evidence: RequestedOnly, scope: Page, batchable: true, action: None,
    },
    ObserveLive(LiveObservationRequest) => LiveObservation {
        stable_name: "observe_live", description: "Capture current page state, snapshot, and screenshot together.", mutability: ReadOnly, evidence: LiveObservation, scope: Page, batchable: false, action: None,
    },
    ListPages(ListPagesRequest) => Vec<PageStatus> {
        stable_name: "list_pages", description: "List supervised browser pages and the current selection.", mutability: ReadOnly, evidence: RequestedOnly, scope: Browser, batchable: false, action: None,
    },
    CreatePage(CreatePageRequest) => PageOperationResult {
        stable_name: "create_page", description: "Create and select a new browser page.", mutability: StateChanging, evidence: LiveObservation, scope: Browser, batchable: false, action: None,
    },
    SelectPage(SelectPageRequest) => PageOperationResult {
        stable_name: "select_page", description: "Select a supervised browser page.", mutability: StateChanging, evidence: LiveObservation, scope: Browser, batchable: false, action: None,
    },
    ClosePage(ClosePageRequest) => PageOperationResult {
        stable_name: "close_page", description: "Close a supervised browser page.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: false, action: None,
    },
    NavigatePage(NavigatePageRequest) => PageOperationResult {
        stable_name: "navigate_page", description: "Navigate a page to a URL and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: None,
    },
    ReloadPage(ReloadPageRequest) => PageOperationResult {
        stable_name: "reload_page", description: "Reload a page and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: None,
    },
    GoBack(GoBackRequest) => PageOperationResult {
        stable_name: "go_back", description: "Move backward in page history and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: None,
    },
    GoForward(GoForwardRequest) => PageOperationResult {
        stable_name: "go_forward", description: "Move forward in page history and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: None,
    },
    Click(ClickRequest) => InteractionResult {
        stable_name: "click", description: "Click an element or declared coordinate and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_CLICK),
    },
    Fill(FillRequest) => InteractionResult {
        stable_name: "fill", description: "Fill an editable element and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_FILL),
    },
    PressKeys(PressKeysRequest) => InteractionResult {
        stable_name: "press_keys", description: "Press validated key chords and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_PRESS_KEYS),
    },
    SelectOption(SelectOptionRequest) => InteractionResult {
        stable_name: "select_option", description: "Choose an option in a selectable element and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_SELECT),
    },
    Hover(HoverRequest) => InteractionResult {
        stable_name: "hover", description: "Move the pointer over an element or coordinate and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_HOVER),
    },
    Drag(DragRequest) => InteractionResult {
        stable_name: "drag", description: "Drag between validated locations and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_DRAG),
    },
    Scroll(ScrollRequest) => InteractionResult {
        stable_name: "scroll", description: "Scroll by an offset or to an element and return live evidence.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_SCROLL),
    },
    UploadFiles(UploadFilesRequest) => InteractionResult {
        stable_name: "upload_files", description: "Upload validated local files through a file input.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_UPLOAD),
    },
    HandleDialog(HandleDialogRequest) => InteractionResult {
        stable_name: "handle_dialog", description: "Accept or dismiss the active browser dialog.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: true, action: Some(&ACTION_DIALOG),
    },
    Wait(WaitRequest) => WaitResult {
        stable_name: "wait", description: "Wait for an explicit page, element, text, navigation, or network condition.", mutability: ReadOnly, evidence: RequestedOnly, scope: Page, batchable: true, action: None,
    },
    Batch(BatchRequest) => BatchResult {
        stable_name: "batch", description: "Execute an ordered one-page batch of browser operations.", mutability: StateChanging, evidence: LiveObservation, scope: Page, batchable: false, action: None,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ImageFormat, ScreenshotTarget, TargetId};
    use uuid::Uuid;

    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(1))
    }

    #[test]
    fn declaration_is_the_complete_operation_registry() {
        assert_eq!(BrowserOperationKind::ALL.len(), 24);
        assert_eq!(
            BROWSER_OPERATION_REGISTRY.len(),
            BrowserOperationKind::ALL.len()
        );
        for (kind, definition) in BrowserOperationKind::ALL
            .iter()
            .zip(BROWSER_OPERATION_REGISTRY)
        {
            assert_eq!(definition.kind, *kind);
            assert_eq!(definition.stable_name, kind.stable_name());
            assert!(!definition.description.trim().is_empty());
            assert_eq!(definition.capability, CapabilityId::Control);
            assert_eq!(definition.action.is_some(), kind.is_interaction());
            let schema = serde_json::to_value(kind.input_schema()).unwrap();
            assert_eq!(
                schema["type"],
                "object",
                "{} input schema",
                kind.stable_name()
            );
        }
        assert_eq!(
            BROWSER_OPERATION_REGISTRY
                .iter()
                .filter(|definition| definition.action.is_some())
                .count(),
            9
        );
        assert!(
            BROWSER_OPERATION_REGISTRY
                .iter()
                .filter(|definition| matches!(
                    definition.kind,
                    BrowserOperationKind::ObserveLive
                        | BrowserOperationKind::ListPages
                        | BrowserOperationKind::CreatePage
                        | BrowserOperationKind::SelectPage
                        | BrowserOperationKind::ClosePage
                        | BrowserOperationKind::Batch
                ))
                .all(|definition| !definition.batchable)
        );
        assert!(
            BROWSER_OPERATION_REGISTRY
                .iter()
                .filter(|definition| !matches!(
                    definition.kind,
                    BrowserOperationKind::ObserveLive
                        | BrowserOperationKind::ListPages
                        | BrowserOperationKind::CreatePage
                        | BrowserOperationKind::SelectPage
                        | BrowserOperationKind::ClosePage
                        | BrowserOperationKind::Batch
                ))
                .all(|definition| definition.batchable)
        );
        assert_eq!(
            BROWSER_OPERATION_REGISTRY[5].scope,
            BrowserOperationScopeKind::Browser
        );
        assert_eq!(
            BROWSER_OPERATION_REGISTRY[6].mutability,
            OperationMutability::StateChanging
        );
    }

    #[test]
    fn request_scope_is_generated_for_browser_and_selected_or_direct_page_operations() {
        let direct = BrowserOperationRequest::InspectPage(InspectPageRequest::new(target()));
        assert_eq!(
            direct.scope(),
            BrowserOperationScope::Page(PageSelection::Target(target()))
        );
        let selected = BrowserOperationRequest::TakeScreenshot(
            ScreenshotRequest::for_selection(
                PageSelection::Selected,
                ScreenshotTarget::Viewport,
                ImageFormat::Png,
                None,
            )
            .unwrap(),
        );
        assert_eq!(
            selected.scope(),
            BrowserOperationScope::Page(PageSelection::Selected)
        );
        assert_eq!(
            BrowserOperationRequest::ListPages(ListPagesRequest).scope(),
            BrowserOperationScope::Browser
        );
    }

    #[test]
    fn generated_request_schemas_preserve_validated_wire_shapes_and_recursion() {
        let list_schema =
            serde_json::to_value(BrowserOperationKind::ListPages.input_schema()).unwrap();
        assert_eq!(list_schema["type"], "object");

        let wait_schema = serde_json::to_value(BrowserOperationKind::Wait.input_schema()).unwrap();
        let encoded = wait_schema.to_string();
        assert!(encoded.contains("poll_interval"));
        assert!(encoded.contains("integer"));
        assert!(!encoded.contains("secs"));
        assert!(!encoded.contains("nanos"));

        let batch_schema =
            serde_json::to_value(BrowserOperationKind::Batch.input_schema()).unwrap();
        assert_eq!(batch_schema["type"], "object");
        assert!(batch_schema.to_string().contains("BrowserOperationRequest"));
    }

    #[test]
    fn requests_are_tagged_and_preserve_associated_kind() {
        let request = BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target()));
        let encoded = serde_json::to_string(&request).unwrap();
        assert!(encoded.contains(BrowserOperationKind::SnapshotPage.stable_name()));
        assert_eq!(
            serde_json::from_str::<BrowserOperationRequest>(&encoded)
                .unwrap()
                .kind(),
            BrowserOperationKind::SnapshotPage
        );
    }
}

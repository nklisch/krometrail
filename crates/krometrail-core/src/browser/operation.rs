use serde::{Deserialize, Serialize};

use super::{
    ClosePageRequest, CreatePageRequest, EncodedScreenshot, EvaluationResult, GoBackRequest,
    GoForwardRequest, InspectPageRequest, ListPagesRequest, LiveObservation,
    LiveObservationRequest, NavigatePageRequest, PageOperationResult, PageSelection, PageSnapshot,
    PageState, PageStatus, ReadOnlyEvaluationRequest, ReloadPageRequest, ScreenshotRequest,
    SelectPageRequest, SnapshotPageRequest,
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
    pub mutability: OperationMutability,
    pub evidence: OperationEvidence,
    pub scope: BrowserOperationScopeKind,
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
            mutability: $mutability:ident,
            evidence: $evidence:ident,
            scope: $scope:ident,
        }
    ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        pub enum BrowserOperationKind {
            $(#[serde(rename = $stable_name)] $variant),+
        }

        impl BrowserOperationKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn stable_name(self) -> &'static str {
                match self { $(Self::$variant => $stable_name),+ }
            }
        }

        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
                mutability: OperationMutability::$mutability,
                evidence: OperationEvidence::$evidence,
                scope: BrowserOperationScopeKind::$scope,
            }),+
        ];
    };
}

define_browser_operations! {
    InspectPage(InspectPageRequest) => PageState {
        stable_name: "inspect_page", mutability: ReadOnly, evidence: RequestedOnly, scope: Page,
    },
    SnapshotPage(SnapshotPageRequest) => PageSnapshot {
        stable_name: "snapshot_page", mutability: ReadOnly, evidence: RequestedOnly, scope: Page,
    },
    TakeScreenshot(ScreenshotRequest) => EncodedScreenshot {
        stable_name: "take_screenshot", mutability: ReadOnly, evidence: RequestedOnly, scope: Page,
    },
    EvaluatePage(ReadOnlyEvaluationRequest) => EvaluationResult {
        stable_name: "evaluate_page", mutability: ReadOnly, evidence: RequestedOnly, scope: Page,
    },
    ObserveLive(LiveObservationRequest) => LiveObservation {
        stable_name: "observe_live", mutability: ReadOnly, evidence: LiveObservation, scope: Page,
    },
    ListPages(ListPagesRequest) => Vec<PageStatus> {
        stable_name: "list_pages", mutability: ReadOnly, evidence: RequestedOnly, scope: Browser,
    },
    CreatePage(CreatePageRequest) => PageOperationResult {
        stable_name: "create_page", mutability: StateChanging, evidence: LiveObservation, scope: Browser,
    },
    SelectPage(SelectPageRequest) => PageOperationResult {
        stable_name: "select_page", mutability: StateChanging, evidence: LiveObservation, scope: Browser,
    },
    ClosePage(ClosePageRequest) => PageOperationResult {
        stable_name: "close_page", mutability: StateChanging, evidence: LiveObservation, scope: Page,
    },
    NavigatePage(NavigatePageRequest) => PageOperationResult {
        stable_name: "navigate_page", mutability: StateChanging, evidence: LiveObservation, scope: Page,
    },
    ReloadPage(ReloadPageRequest) => PageOperationResult {
        stable_name: "reload_page", mutability: StateChanging, evidence: LiveObservation, scope: Page,
    },
    GoBack(GoBackRequest) => PageOperationResult {
        stable_name: "go_back", mutability: StateChanging, evidence: LiveObservation, scope: Page,
    },
    GoForward(GoForwardRequest) => PageOperationResult {
        stable_name: "go_forward", mutability: StateChanging, evidence: LiveObservation, scope: Page,
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
        assert_eq!(BrowserOperationKind::ALL.len(), 13);
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
        }
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

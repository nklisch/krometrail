use serde::{Deserialize, Serialize};

use super::observation::{
    EncodedScreenshot, EvaluationResult, InspectPageRequest, LiveObservation,
    LiveObservationRequest, PageSnapshot, PageState, ReadOnlyEvaluationRequest, ScreenshotRequest,
    SnapshotPageRequest,
};
use crate::TargetId;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserOperationDefinition {
    pub kind: BrowserOperationKind,
    pub stable_name: &'static str,
    pub mutability: OperationMutability,
    pub evidence: OperationEvidence,
}

macro_rules! define_browser_operations {
    ($(
        $variant:ident($request:ty) => $result:ty {
            stable_name: $stable_name:literal,
            mutability: $mutability:ident,
            evidence: $evidence:ident,
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
            pub const fn target_id(&self) -> TargetId {
                match self { $(Self::$variant(request) => request.target_id),+ }
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
            }),+
        ];
    };
}

define_browser_operations! {
    InspectPage(InspectPageRequest) => PageState {
        stable_name: "inspect_page", mutability: ReadOnly, evidence: RequestedOnly,
    },
    SnapshotPage(SnapshotPageRequest) => PageSnapshot {
        stable_name: "snapshot_page", mutability: ReadOnly, evidence: RequestedOnly,
    },
    TakeScreenshot(ScreenshotRequest) => EncodedScreenshot {
        stable_name: "take_screenshot", mutability: ReadOnly, evidence: RequestedOnly,
    },
    EvaluatePage(ReadOnlyEvaluationRequest) => EvaluationResult {
        stable_name: "evaluate_page", mutability: ReadOnly, evidence: RequestedOnly,
    },
    ObserveLive(LiveObservationRequest) => LiveObservation {
        stable_name: "observe_live", mutability: ReadOnly, evidence: LiveObservation,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ImageFormat, ScreenshotTarget, SnapshotPageRequest};

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
    fn target() -> TargetId {
        TargetId::from_uuid(UUID.parse().unwrap())
    }

    #[test]
    fn declaration_is_the_complete_operation_registry() {
        assert_eq!(BrowserOperationKind::ALL.len(), 5);
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
            assert_eq!(definition.mutability, OperationMutability::ReadOnly);
        }
        assert_eq!(
            BROWSER_OPERATION_REGISTRY[4].evidence,
            OperationEvidence::LiveObservation
        );
    }

    #[test]
    fn requests_are_tagged_and_preserve_associated_kind() {
        let requests = vec![
            BrowserOperationRequest::InspectPage(InspectPageRequest {
                target_id: target(),
            }),
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest {
                target_id: target(),
            }),
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target(),
                    ScreenshotTarget::Viewport,
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target(), "document.title", false).unwrap(),
            ),
            BrowserOperationRequest::ObserveLive(LiveObservationRequest {
                target_id: target(),
            }),
        ];
        for (request, kind) in requests.into_iter().zip(BrowserOperationKind::ALL) {
            assert_eq!(request.kind(), *kind);
            let encoded = serde_json::to_string(&request).unwrap();
            assert!(encoded.contains(kind.stable_name()));
            let decoded: BrowserOperationRequest = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded.kind(), *kind);
        }
    }
}

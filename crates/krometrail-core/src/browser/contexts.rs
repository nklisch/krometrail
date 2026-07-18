use std::num::NonZeroU64;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{NonEmptyText, SanitizedUrl, TargetId, error::invalid};

use super::{PageSelection, PageStatus};

pub const MAX_KNOWN_PAGE_TARGETS: usize = 128;
pub const MAX_PAGE_FRAMES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct PageSequence(NonZeroU64);

impl PageSequence {
    pub fn new(value: u64) -> crate::Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| invalid("page sequence must be non-zero"))
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for PageSequence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageContextStatus {
    pub page: PageStatus,
    pub sequence: PageSequence,
    pub opener_target_id: Option<TargetId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageContextInventory {
    pub cursor: PageSequence,
    pub pages: Vec<PageContextStatus>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPageContextsRequest {}

#[allow(non_upper_case_globals)]
pub const ListPageContextsRequest: ListPageContextsRequest = ListPageContextsRequest {};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitForPageRequest {
    pub after: PageSequence,
    pub opener_target_id: Option<TargetId>,
    #[schemars(range(min = 1_u64, max = 30_000_u64))]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WaitForPageResult {
    pub matched: PageContextStatus,
    pub cursor: PageSequence,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageFrameReference {
    pub target_id: TargetId,
    pub attachment_generation: u64,
    pub frame_generation: PageSequence,
    pub frame_key: NonEmptyText,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameAccess {
    MainDocument,
    SameOriginSameProcess,
    CrossOrigin,
    OutOfProcess,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageFrameStatus {
    pub reference: PageFrameReference,
    pub parent: Option<PageFrameReference>,
    pub depth: u16,
    pub access: FrameAccess,
    pub url: SanitizedUrl,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListFramesRequest {
    #[serde(default)]
    pub target: PageSelection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageFrameInventory {
    pub target_id: TargetId,
    pub frames: Vec<PageFrameStatus>,
    pub omitted_frame_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "document", content = "frame", rename_all = "snake_case")]
pub enum SemanticDocumentScope {
    MainDocument,
    Frame(PageFrameReference),
}

impl Default for SemanticDocumentScope {
    fn default() -> Self {
        Self::MainDocument
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_sequence_rejects_zero() {
        assert!(serde_json::from_str::<PageSequence>("0").is_err());
    }
}

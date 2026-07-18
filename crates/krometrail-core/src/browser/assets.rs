use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{SanitizedUrl, TargetId};

use super::PageSelection;

pub const MAX_PAGE_ASSETS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PageAssetKind {
    Script,
    Stylesheet,
    Image,
    Font,
    Media,
    Fetch,
    XmlHttpRequest,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageAssetMetadata {
    pub url: SanitizedUrl,
    pub kind: PageAssetKind,
    pub duration_ms: f64,
    pub transfer_bytes: Option<u64>,
    pub encoded_body_bytes: Option<u64>,
    pub decoded_body_bytes: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPageAssetsRequest {
    #[serde(default)]
    pub target: PageSelection,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageAssetInventory {
    pub target_id: TargetId,
    pub assets: Vec<PageAssetMetadata>,
    pub omitted_asset_count: u32,
}

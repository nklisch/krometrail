use std::{fmt, num::NonZeroU64};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{DownloadId, NonEmptyText, SessionId, TargetId, error::invalid};

use super::{InteractionAnchor, PageOperationResult, PageSelection, SanitizedUrl};

pub const MAX_CLIPBOARD_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_MANAGED_DOWNLOADS: usize = 32;
pub const MAX_MANAGED_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_DOWNLOAD_WAIT_MILLIS: u64 = 120_000;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ReadClipboardRequest {
    pub target: PageSelection,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ClipboardRead {
    pub target_id: TargetId,
    pub text: String,
    pub utf8_bytes: u64,
}

impl fmt::Debug for ClipboardRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipboardRead")
            .field("target_id", &self.target_id)
            .field("utf8_bytes", &self.utf8_bytes)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteClipboardRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub text: String,
}

impl fmt::Debug for WriteClipboardRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WriteClipboardRequest")
            .field("target", &self.target)
            .field("utf8_bytes", &self.text.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardWriteResult {
    pub utf8_bytes: u64,
    pub operation: PageOperationResult,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct DownloadSequence(NonZeroU64);

impl DownloadSequence {
    pub fn new(value: u64) -> crate::Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| invalid("download sequence must be positive"))
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for DownloadSequence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    InProgress,
    Completed,
    Cancelled,
    Failed,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct DownloadDisplayName(NonEmptyText);

impl DownloadDisplayName {
    pub fn sanitize(value: &str) -> Self {
        let value: String = value
            .chars()
            .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
            .take(255)
            .collect();
        let value = value.trim();
        Self(
            NonEmptyText::new(if value.is_empty() { "download" } else { value })
                .expect("fallback is non-empty"),
        )
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ManagedDownload {
    pub id: DownloadId,
    pub sequence: DownloadSequence,
    pub target_id: Option<TargetId>,
    pub state: DownloadState,
    pub suggested_filename: DownloadDisplayName,
    pub source_url: SanitizedUrl,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub resource_uri: Option<String>,
}

impl fmt::Debug for ManagedDownload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedDownload")
            .field("id", &self.id)
            .field("sequence", &self.sequence)
            .field("target_id", &self.target_id)
            .field("state", &self.state)
            .field("received_bytes", &self.received_bytes)
            .field("total_bytes", &self.total_bytes)
            .field("has_resource", &self.resource_uri.is_some())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct DownloadInventory {
    pub session_id: SessionId,
    pub cursor: Option<DownloadSequence>,
    pub downloads: Vec<ManagedDownload>,
}

impl fmt::Debug for DownloadInventory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DownloadInventory")
            .field("session_id", &self.session_id)
            .field("cursor", &self.cursor)
            .field("download_count", &self.downloads.len())
            .finish()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListDownloadsRequest {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitForDownloadRequest {
    pub after: Option<DownloadSequence>,
    pub download_id: Option<DownloadId>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default = "default_download_wait_millis")]
    pub timeout: u64,
}
const fn default_download_wait_millis() -> u64 {
    30_000
}
impl WaitForDownloadRequest {
    pub fn validate(&self) -> crate::Result<()> {
        if self.timeout == 0 || self.timeout > MAX_DOWNLOAD_WAIT_MILLIS {
            return Err(invalid(
                "download wait timeout must be between 1 and 120000 milliseconds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelDownloadRequest {
    pub download_id: DownloadId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CancelDownloadResult {
    pub download_id: DownloadId,
    pub state: DownloadState,
    pub operation: InteractionAnchor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadManagedDownloadRequest {
    pub session_id: SessionId,
    pub download_id: DownloadId,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedDownloadRead {
    pub session_id: SessionId,
    pub download_id: DownloadId,
    pub media_type: NonEmptyText,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_debug_is_redacted() {
        let request = WriteClipboardRequest {
            target: PageSelection::Selected,
            text: "secret-clipboard".into(),
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("secret-clipboard"));
        assert!(debug.contains("16"));
    }
    #[test]
    fn display_names_are_basenames_and_sequences_are_positive() {
        assert_eq!(
            serde_json::to_value(DownloadDisplayName::sanitize("../a\\b\n")).unwrap(),
            "..ab"
        );
        assert!(DownloadSequence::new(0).is_err());
    }

    #[test]
    fn download_debug_omits_names_urls_and_resource_uris() {
        let download = ManagedDownload {
            id: DownloadId::from_uuid(uuid::Uuid::from_u128(1)),
            sequence: DownloadSequence::new(1).unwrap(),
            target_id: None,
            state: DownloadState::Completed,
            suggested_filename: DownloadDisplayName::sanitize("secret-name.txt"),
            source_url: SanitizedUrl::sanitize("https://example.test/secret-path").unwrap(),
            received_bytes: 4,
            total_bytes: Some(4),
            resource_uri: Some("krometrail://local/secret-resource".into()),
        };
        let item_debug = format!("{download:?}");
        assert!(!item_debug.contains("secret-name"));
        assert!(!item_debug.contains("secret-path"));
        assert!(!item_debug.contains("secret-resource"));
        let inventory = DownloadInventory {
            session_id: SessionId::from_uuid(uuid::Uuid::from_u128(2)),
            cursor: Some(download.sequence),
            downloads: vec![download],
        };
        let inventory_debug = format!("{inventory:?}");
        assert!(inventory_debug.contains("download_count: 1"));
        assert!(!inventory_debug.contains("secret"));
    }
}

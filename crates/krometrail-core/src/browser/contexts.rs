use std::num::NonZeroU64;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{NonEmptyText, SanitizedUrl, TargetId, error::invalid};

use super::{PageSelection, PageStatus};

pub const MAX_KNOWN_PAGE_TARGETS: usize = 128;
pub const MAX_PAGE_FRAMES: usize = 256;
pub const MAX_FRAME_NAME_BYTES: usize = 128;
pub const MAX_FRAME_PATH_BYTES: usize = 512;

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WaitForPageRequest {
    pub after: PageSequence,
    pub opener_target_id: Option<TargetId>,
    pub timeout_ms: u64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WaitForPageRequestWire {
    after: PageSequence,
    opener_target_id: Option<TargetId>,
    #[schemars(range(min = 1_u64, max = 30_000_u64))]
    timeout_ms: u64,
}

impl<'de> Deserialize<'de> for WaitForPageRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WaitForPageRequestWire::deserialize(deserializer)?;
        if !(1..=30_000).contains(&wire.timeout_ms) {
            return Err(serde::de::Error::custom(
                "page wait timeout must be between 1 and 30000 milliseconds",
            ));
        }
        Ok(Self {
            after: wire.after,
            opener_target_id: wire.opener_target_id,
            timeout_ms: wire.timeout_ms,
        })
    }
}

impl JsonSchema for WaitForPageRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "WaitForPageRequest".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        WaitForPageRequestWire::json_schema(generator)
    }
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
    /// The frame element's author-assigned `name`, bounded and control-character free.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<NonEmptyText>,
    /// The frame document's real URL path, exposed only for a frame that shares the main
    /// document's origin. Cross-origin and out-of-process frames keep `url`'s hashed path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_origin_path: Option<NonEmptyText>,
}

impl PageFrameStatus {
    pub fn new(
        reference: PageFrameReference,
        parent: Option<PageFrameReference>,
        depth: u16,
        access: FrameAccess,
        url: SanitizedUrl,
        raw_name: Option<&str>,
        raw_url: &str,
    ) -> crate::Result<Self> {
        if depth == 0 && parent.is_some() {
            return Err(invalid("a root frame must not declare a parent frame"));
        }
        Ok(Self {
            reference,
            parent,
            depth,
            access,
            url,
            name: raw_name.and_then(bounded_frame_label),
            same_origin_path: same_origin_frame_path(access, raw_url),
        })
    }
}

/// Frame `name` is author-controlled page text. Bound it and reject control characters before it
/// reaches a response, exactly as every other author-controlled identity is bounded.
fn bounded_frame_label(raw: &str) -> Option<NonEmptyText> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_FRAME_NAME_BYTES
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    NonEmptyText::new(trimmed).ok()
}

/// The same-origin frame-path relaxation. Only a frame that shares the main document's origin
/// gets its real path; everything else keeps the hashed path in `SanitizedUrl`.
fn same_origin_frame_path(access: FrameAccess, raw_url: &str) -> Option<NonEmptyText> {
    if !matches!(
        access,
        FrameAccess::MainDocument | FrameAccess::SameOriginSameProcess
    ) {
        return None;
    }
    let without_fragment = raw_url.split('#').next().unwrap_or(raw_url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let (_, remainder) = without_query.split_once("://")?;
    let (_, path) = remainder.split_once('/')?;
    let path = format!("/{path}");
    if path.len() > MAX_FRAME_PATH_BYTES || path.chars().any(char::is_control) {
        return None;
    }
    NonEmptyText::new(path).ok()
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "document", content = "frame", rename_all = "snake_case")]
pub enum SemanticDocumentScope {
    #[default]
    MainDocument,
    Frame(PageFrameReference),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_sequence_rejects_zero() {
        assert!(serde_json::from_str::<PageSequence>("0").is_err());
    }

    fn frame_reference() -> PageFrameReference {
        PageFrameReference {
            target_id: TargetId::from_uuid("11111111-1111-4111-8111-111111111111".parse().unwrap()),
            attachment_generation: 1,
            frame_key: NonEmptyText::new("frame-0").unwrap(),
        }
    }

    fn frame(access: FrameAccess, raw_url: &str, raw_name: Option<&str>) -> PageFrameStatus {
        PageFrameStatus::new(
            frame_reference(),
            None,
            0,
            access,
            SanitizedUrl::sanitize(raw_url).unwrap(),
            raw_name,
            raw_url,
        )
        .unwrap()
    }

    /// The same-origin relaxation boundary: a frame sharing the main document's origin exposes its
    /// real path, and every other access class keeps the hashed path.
    #[test]
    fn only_same_origin_frames_expose_a_real_path() {
        let same_origin = frame(
            FrameAccess::SameOriginSameProcess,
            "https://example.test/nested/left.html?token=secret#frag",
            Some("left"),
        );
        assert_eq!(
            same_origin
                .same_origin_path
                .as_ref()
                .map(NonEmptyText::as_str),
            Some("/nested/left.html")
        );
        assert_eq!(
            same_origin.name.as_ref().map(NonEmptyText::as_str),
            Some("left")
        );

        for access in [
            FrameAccess::CrossOrigin,
            FrameAccess::OutOfProcess,
            FrameAccess::Indeterminate,
        ] {
            let redacted = frame(access, "https://other.test/tracker/pixel.html", Some("ad"));
            assert!(redacted.same_origin_path.is_none(), "{access:?}");
            // Name stays available: it is author-assigned markup, not the third-party URL.
            assert_eq!(redacted.name.as_ref().map(NonEmptyText::as_str), Some("ad"));
        }
    }

    #[test]
    fn frame_labels_and_paths_are_bounded_and_control_free() {
        let oversized = "x".repeat(MAX_FRAME_NAME_BYTES + 1);
        assert!(
            frame(
                FrameAccess::MainDocument,
                "https://a.test/",
                Some(&oversized)
            )
            .name
            .is_none()
        );
        assert!(
            frame(
                FrameAccess::MainDocument,
                "https://a.test/",
                Some("a\u{7}b")
            )
            .name
            .is_none()
        );
        let long_path = format!("https://a.test/{}", "p".repeat(MAX_FRAME_PATH_BYTES + 1));
        assert!(
            frame(FrameAccess::MainDocument, &long_path, None)
                .same_origin_path
                .is_none()
        );
        // An origin-only URL has no path to report.
        assert!(
            frame(FrameAccess::MainDocument, "https://a.test", None)
                .same_origin_path
                .is_none()
        );
    }

    #[test]
    fn page_wait_timeout_is_bounded() {
        let valid = serde_json::json!({"after":1,"timeout_ms":500});
        assert!(serde_json::from_value::<WaitForPageRequest>(valid).is_ok());
        for timeout_ms in [0, 30_001] {
            assert!(
                serde_json::from_value::<WaitForPageRequest>(
                    serde_json::json!({"after":1,"timeout_ms":timeout_ms})
                )
                .is_err()
            );
        }
    }
}

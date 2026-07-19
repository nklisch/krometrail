//! Validated contracts for progressive temporal-evidence retrieval.
//!
//! Requests consume an already-resolved range. They cannot parse natural anchors,
//! expose physical storage, or turn current browser references into historical identity.

use std::{
    collections::HashSet,
    fmt,
    num::{NonZeroU16, NonZeroU64},
    sync::Arc,
    time::Instant,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use sha2::{Digest, Sha256};

use crate::{
    AnalysisScale, ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGenerationResult,
    ArtifactId, ArtifactLabelsRequest, ArtifactManifest, ArtifactMarker, CancellationSignal,
    CapturedFrame, CssRect, ErrorCode, FrameId, ImageFormat, KrometrailError, NodeReference,
    NonEmptyText, OutputLimitsRequest, ResolvedRange, Result, RetentionPolicy, RetentionRange,
    RetentionStatus, SegmentId, SessionId, SessionRange, SessionTime, TargetId, VisualEpoch,
    error::invalid,
    ports::{CurrentReferenceGeometry, PortFuture},
    validation::{delegate_json_schema, deserialize_validated},
};

pub const MAX_SOURCE_READ_FRAMES: u16 = 64;
pub const MAX_SOURCE_ITEM_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_SOURCE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ARTIFACT_READ_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MASK_DIMENSION: u32 = 8_192;
pub const MAX_MASK_PIXELS: usize = 16_777_216;
pub const MAX_MASK_BYTES: usize = 2 * 1024 * 1024;

fn validate_id<T>(id: &T, label: &str, uuid: impl FnOnce(&T) -> &uuid::Uuid) -> Result<()> {
    if uuid(id).is_nil() {
        Err(invalid(format!("{label} must be non-nil")))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct EvidenceScope {
    pub session_id: SessionId,
    pub target_id: TargetId,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct EvidenceScopeWire {
    session_id: SessionId,
    target_id: TargetId,
}

impl EvidenceScope {
    pub fn new(session_id: SessionId, target_id: TargetId) -> Result<Self> {
        validate_id(&session_id, "evidence session id", SessionId::as_uuid)?;
        validate_id(&target_id, "evidence target id", TargetId::as_uuid)?;
        Ok(Self {
            session_id,
            target_id,
        })
    }

    pub fn from_range(range: &ResolvedRange) -> Result<Self> {
        Self::new(range.session_id, range.target_id)
    }
}

impl<'de> Deserialize<'de> for EvidenceScope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: EvidenceScopeWire| {
            Self::new(wire.session_id, wire.target_id)
        })
    }
}

delegate_json_schema!(EvidenceScope => EvidenceScopeWire);

/// Canonical lowercase SHA-256 digest used by weak evidence handles.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Sha256Digest([u8; 32]);

#[derive(schemars::JsonSchema)]
#[schemars(transparent)]
#[allow(dead_code)]
struct Sha256DigestSchema(
    #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))] String,
);

impl Sha256Digest {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn digest(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() != 64
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
        {
            return Err(D::Error::custom(
                "SHA-256 must be 64 lowercase hexadecimal characters",
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let nibble = |byte| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => 0,
            };
            bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        Ok(Self(bytes))
    }
}

delegate_json_schema!(Sha256Digest => Sha256DigestSchema);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArtifactEvidenceHandle {
    pub artifact_id: ArtifactId,
    pub scope: EvidenceScope,
    pub media_type: NonEmptyText,
    pub content_sha256: Sha256Digest,
    pub encoded_byte_len: u64,
    pub provenance: ArtifactManifest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactEvidenceHandleWire {
    artifact_id: ArtifactId,
    scope: EvidenceScope,
    media_type: NonEmptyText,
    content_sha256: Sha256Digest,
    encoded_byte_len: u64,
    provenance: ArtifactManifest,
}

impl ArtifactEvidenceHandle {
    pub fn new(
        artifact_id: ArtifactId,
        scope: EvidenceScope,
        media_type: NonEmptyText,
        content_sha256: Sha256Digest,
        encoded_byte_len: u64,
        provenance: ArtifactManifest,
    ) -> Result<Self> {
        validate_id(&artifact_id, "artifact id", ArtifactId::as_uuid)?;
        if provenance.artifact_id() != &artifact_id {
            return Err(invalid("artifact handle id must match its provenance"));
        }
        if media_type.as_str() != "image/png" {
            return Err(invalid("derived artifact media type must be image/png"));
        }
        if encoded_byte_len == 0 {
            return Err(invalid("artifact encoded byte length must be non-zero"));
        }
        if provenance.output_hash().as_bytes() != content_sha256.as_bytes() {
            return Err(invalid(
                "artifact handle digest must match its exact provenance",
            ));
        }
        Ok(Self {
            artifact_id,
            scope,
            media_type,
            content_sha256,
            encoded_byte_len,
            provenance,
        })
    }
}

impl<'de> Deserialize<'de> for ArtifactEvidenceHandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: ArtifactEvidenceHandleWire| {
            Self::new(
                wire.artifact_id,
                wire.scope,
                wire.media_type,
                wire.content_sha256,
                wire.encoded_byte_len,
                wire.provenance,
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceFrameHandle {
    pub frame_id: FrameId,
    pub scope: EvidenceScope,
    pub request_position: u32,
    pub resolved_position: u32,
    pub media_type: NonEmptyText,
    pub content_sha256: Sha256Digest,
    pub encoded_byte_len: u64,
    pub provenance: CapturedFrame,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFrameHandleWire {
    frame_id: FrameId,
    scope: EvidenceScope,
    request_position: u32,
    resolved_position: u32,
    media_type: NonEmptyText,
    content_sha256: Sha256Digest,
    encoded_byte_len: u64,
    provenance: CapturedFrame,
}

impl SourceFrameHandle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        frame_id: FrameId,
        scope: EvidenceScope,
        request_position: u32,
        resolved_position: u32,
        media_type: NonEmptyText,
        content_sha256: Sha256Digest,
        encoded_byte_len: u64,
        provenance: CapturedFrame,
    ) -> Result<Self> {
        validate_id(&frame_id, "source frame id", FrameId::as_uuid)?;
        if frame_id != provenance.id()
            || scope.session_id != provenance.session_id()
            || scope.target_id != provenance.target_id()
        {
            return Err(invalid(
                "source frame handle scope and id must match its provenance",
            ));
        }
        let expected_media_type = match provenance.format() {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
        };
        if media_type.as_str() != expected_media_type {
            return Err(invalid(
                "source frame media type must match its encoded image format",
            ));
        }
        if encoded_byte_len == 0 {
            return Err(invalid("source frame encoded byte length must be non-zero"));
        }
        Ok(Self {
            frame_id,
            scope,
            request_position,
            resolved_position,
            media_type,
            content_sha256,
            encoded_byte_len,
            provenance,
        })
    }
}

impl<'de> Deserialize<'de> for SourceFrameHandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: SourceFrameHandleWire| {
            Self::new(
                wire.frame_id,
                wire.scope,
                wire.request_position,
                wire.resolved_position,
                wire.media_type,
                wire.content_sha256,
                wire.encoded_byte_len,
                wire.provenance,
            )
        })
    }
}

/// Request-scoped bytes are deliberately not a Serde or schema value.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactRead {
    pub handle: ArtifactEvidenceHandle,
    encoded_bytes: Arc<[u8]>,
}

impl ArtifactRead {
    pub fn new(
        handle: ArtifactEvidenceHandle,
        encoded_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self> {
        let encoded_bytes = encoded_bytes.into();
        if encoded_bytes.len() as u64 != handle.encoded_byte_len
            || Sha256Digest::digest(&encoded_bytes) != handle.content_sha256
        {
            return Err(invalid(
                "artifact payload must match its handle length and SHA-256",
            ));
        }
        Ok(Self {
            handle,
            encoded_bytes,
        })
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded_bytes
    }
}

/// Request-scoped bytes are deliberately not a Serde or schema value.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFrameRead {
    pub handle: SourceFrameHandle,
    encoded_bytes: Arc<[u8]>,
}

impl SourceFrameRead {
    pub fn new(handle: SourceFrameHandle, encoded_bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let encoded_bytes = encoded_bytes.into();
        if encoded_bytes.len() as u64 != handle.encoded_byte_len
            || Sha256Digest::digest(&encoded_bytes) != handle.content_sha256
        {
            return Err(invalid(
                "source frame payload must match its handle length and SHA-256",
            ));
        }
        Ok(Self {
            handle,
            encoded_bytes,
        })
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SourceReadLimitsRequest {
    max_frames: NonZeroU16,
    max_item_bytes: NonZeroU64,
    max_total_bytes: NonZeroU64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SourceReadLimitsWire {
    max_frames: u16,
    max_item_bytes: u64,
    max_total_bytes: u64,
}

impl SourceReadLimitsRequest {
    pub fn new(max_frames: u16, max_item_bytes: u64, max_total_bytes: u64) -> Result<Self> {
        let max_frames = NonZeroU16::new(max_frames)
            .ok_or_else(|| invalid("source frame limit must be non-zero"))?;
        let max_item_bytes = NonZeroU64::new(max_item_bytes)
            .ok_or_else(|| invalid("source item byte limit must be non-zero"))?;
        let max_total_bytes = NonZeroU64::new(max_total_bytes)
            .ok_or_else(|| invalid("source total byte limit must be non-zero"))?;
        let mut actual = Vec::new();
        let mut limits = Vec::new();
        if max_frames.get() > MAX_SOURCE_READ_FRAMES {
            actual.push(format!("max_frames={}", max_frames.get()));
            limits.push(format!("max_frames={MAX_SOURCE_READ_FRAMES}"));
        }
        if max_item_bytes.get() > MAX_SOURCE_ITEM_BYTES {
            actual.push(format!("max_item_bytes={}", max_item_bytes.get()));
            limits.push(format!("max_item_bytes={MAX_SOURCE_ITEM_BYTES}"));
        }
        if max_total_bytes.get() > MAX_SOURCE_TOTAL_BYTES {
            actual.push(format!("max_total_bytes={}", max_total_bytes.get()));
            limits.push(format!("max_total_bytes={MAX_SOURCE_TOTAL_BYTES}"));
        }
        if !actual.is_empty() {
            return Err(KrometrailError::limit_exceeded(
                ErrorCode::InvalidInput,
                "source read limits",
                actual.join(", "),
                limits.join(", "),
                Some(format!(
                    "max_frames ≤ {MAX_SOURCE_READ_FRAMES}, max_item_bytes ≤ {MAX_SOURCE_ITEM_BYTES}, max_total_bytes ≤ {MAX_SOURCE_TOTAL_BYTES}"
                )),
            )
            .with_recovery(
                NonEmptyText::new(
                    "lower each named source limit to its runtime ceiling, then retry the request",
                )
                .expect("source request limit recovery is non-empty"),
            ));
        }
        if max_item_bytes > max_total_bytes {
            return Err(invalid(
                "source item byte limit must not exceed total byte limit",
            ));
        }
        Ok(Self {
            max_frames,
            max_item_bytes,
            max_total_bytes,
        })
    }

    pub const fn max_frames(self) -> u16 {
        self.max_frames.get()
    }
    pub const fn max_item_bytes(self) -> u64 {
        self.max_item_bytes.get()
    }
    pub const fn max_total_bytes(self) -> u64 {
        self.max_total_bytes.get()
    }
}

impl<'de> Deserialize<'de> for SourceReadLimitsRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: SourceReadLimitsWire| {
            Self::new(wire.max_frames, wire.max_item_bytes, wire.max_total_bytes)
        })
    }
}

delegate_json_schema!(SourceReadLimitsRequest => SourceReadLimitsWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetrieveArtifactRequest {
    pub scope: EvidenceScope,
    pub artifact_id: ArtifactId,
    max_encoded_bytes: NonZeroU64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RetrieveArtifactRequestWire {
    scope: EvidenceScope,
    artifact_id: ArtifactId,
    max_encoded_bytes: u64,
}

impl RetrieveArtifactRequest {
    pub fn new(
        scope: EvidenceScope,
        artifact_id: ArtifactId,
        max_encoded_bytes: u64,
    ) -> Result<Self> {
        validate_id(&artifact_id, "artifact id", ArtifactId::as_uuid)?;
        let max_encoded_bytes = NonZeroU64::new(max_encoded_bytes)
            .ok_or_else(|| invalid("artifact read byte limit must be non-zero"))?;
        if max_encoded_bytes.get() > MAX_ARTIFACT_READ_BYTES {
            return Err(invalid("artifact read byte limit exceeds runtime ceiling"));
        }
        Ok(Self {
            scope,
            artifact_id,
            max_encoded_bytes,
        })
    }

    pub const fn max_encoded_bytes(&self) -> u64 {
        self.max_encoded_bytes.get()
    }
}

impl<'de> Deserialize<'de> for RetrieveArtifactRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: RetrieveArtifactRequestWire| {
            Self::new(wire.scope, wire.artifact_id, wire.max_encoded_bytes)
        })
    }
}

delegate_json_schema!(RetrieveArtifactRequest => RetrieveArtifactRequestWire);

/// One scoped source-frame read used by durable resource adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetrieveSourceFrameRequest {
    pub scope: EvidenceScope,
    pub frame_id: FrameId,
    max_encoded_bytes: NonZeroU64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RetrieveSourceFrameRequestWire {
    scope: EvidenceScope,
    frame_id: FrameId,
    max_encoded_bytes: u64,
}

impl RetrieveSourceFrameRequest {
    pub fn new(scope: EvidenceScope, frame_id: FrameId, max_encoded_bytes: u64) -> Result<Self> {
        validate_id(&frame_id, "source frame id", FrameId::as_uuid)?;
        let max_encoded_bytes = NonZeroU64::new(max_encoded_bytes)
            .ok_or_else(|| invalid("source frame read byte limit must be non-zero"))?;
        if max_encoded_bytes.get() > MAX_SOURCE_ITEM_BYTES {
            return Err(invalid(
                "source frame read byte limit exceeds runtime ceiling",
            ));
        }
        Ok(Self {
            scope,
            frame_id,
            max_encoded_bytes,
        })
    }

    pub const fn max_encoded_bytes(&self) -> u64 {
        self.max_encoded_bytes.get()
    }
}

impl<'de> Deserialize<'de> for RetrieveSourceFrameRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: RetrieveSourceFrameRequestWire| {
            Self::new(wire.scope, wire.frame_id, wire.max_encoded_bytes)
        })
    }
}

delegate_json_schema!(RetrieveSourceFrameRequest => RetrieveSourceFrameRequestWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "selection", content = "frame_ids", rename_all = "snake_case")]
pub enum SourceFrameSelection {
    ResolvedOrder,
    Ids(Vec<FrameId>),
}

impl SourceFrameSelection {
    fn validate_ids(ids: &[FrameId]) -> Result<()> {
        if ids.is_empty() {
            return Err(invalid("explicit source frame selection must not be empty"));
        }
        let mut seen = HashSet::with_capacity(ids.len());
        for id in ids {
            validate_id(id, "selected source frame id", FrameId::as_uuid)?;
            if !seen.insert(*id) {
                return Err(invalid(
                    "explicit source frame selection must contain unique ids",
                ));
            }
        }
        Ok(())
    }

    fn validate(&self, range: &ResolvedRange) -> Result<()> {
        let Self::Ids(ids) = self else {
            return Ok(());
        };
        Self::validate_ids(ids)?;
        for id in ids {
            if !range.frame_ids.contains(id) {
                return Err(invalid(
                    "selected source frame must belong to the resolved range",
                ));
            }
        }
        Ok(())
    }

    pub fn selected_count(&self, range: &ResolvedRange) -> usize {
        match self {
            Self::ResolvedOrder => range.frame_ids.len(),
            Self::Ids(ids) => ids.len(),
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "selection",
    content = "frame_ids",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SourceFrameSelectionWire {
    ResolvedOrder,
    Ids(Vec<FrameId>),
}

impl<'de> Deserialize<'de> for SourceFrameSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        match SourceFrameSelectionWire::deserialize(deserializer)? {
            SourceFrameSelectionWire::ResolvedOrder => Ok(Self::ResolvedOrder),
            SourceFrameSelectionWire::Ids(ids) => {
                Self::validate_ids(&ids).map_err(D::Error::custom)?;
                Ok(Self::Ids(ids))
            }
        }
    }
}

delegate_json_schema!(SourceFrameSelection => SourceFrameSelectionWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceFramesRequest {
    pub range: ResolvedRange,
    pub selection: SourceFrameSelection,
    pub offset: u32,
    pub limits: SourceReadLimitsRequest,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SourceFramesRequestWire {
    range: ResolvedRange,
    selection: SourceFrameSelection,
    #[serde(default)]
    offset: u32,
    limits: SourceReadLimitsRequest,
}

impl SourceFramesRequest {
    pub fn new(
        range: ResolvedRange,
        selection: SourceFrameSelection,
        limits: SourceReadLimitsRequest,
    ) -> Result<Self> {
        Self::new_with_offset(range, selection, 0, limits)
    }

    pub fn new_with_offset(
        range: ResolvedRange,
        selection: SourceFrameSelection,
        offset: u32,
        limits: SourceReadLimitsRequest,
    ) -> Result<Self> {
        validate_resolved_range(&range)?;
        selection.validate(&range)?;
        if matches!(selection, SourceFrameSelection::Ids(_)) && offset != 0 {
            return Err(invalid(
                "source frame offset is only valid for resolved-order selection",
            ));
        }
        Ok(Self {
            range,
            selection,
            offset,
            limits,
        })
    }

    pub fn validate_for_fetch(&self) -> Result<()> {
        if self.offset != 0 {
            return Err(invalid(
                "fetch_source_frames does not support offsets; use list_source_frames pagination to discover the next offset",
            ));
        }
        let selected_count = self.selection.selected_count(&self.range);
        if selected_count > usize::from(self.limits.max_frames()) {
            return Err(KrometrailError::limit_exceeded(
                ErrorCode::ResourceLimitExceeded,
                "selected source frame count",
                selected_count,
                self.limits.max_frames(),
                Some(self.limits.max_frames()),
            )
            .with_recovery(
                NonEmptyText::new("request a source-frame page no larger than the limit")
                    .expect("source page limit recovery is non-empty"),
            ));
        }
        Ok(())
    }

    pub fn selected_frame_ids(&self) -> Vec<FrameId> {
        match &self.selection {
            SourceFrameSelection::ResolvedOrder => {
                let start = usize::try_from(self.offset).unwrap_or(usize::MAX);
                let start = start.min(self.range.frame_ids.len());
                let end = start
                    .saturating_add(usize::from(self.limits.max_frames()))
                    .min(self.range.frame_ids.len());
                self.range.frame_ids[start..end].to_vec()
            }
            SourceFrameSelection::Ids(ids) => ids.clone(),
        }
    }

    pub fn omitted_frame_count(&self) -> u64 {
        u64::try_from(
            self.selection
                .selected_count(&self.range)
                .saturating_sub(self.selected_frame_ids().len()),
        )
        .unwrap_or(u64::MAX)
    }
}

impl<'de> Deserialize<'de> for SourceFramesRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: SourceFramesRequestWire| {
            Self::new_with_offset(wire.range, wire.selection, wire.offset, wire.limits)
        })
    }
}

delegate_json_schema!(SourceFramesRequest => SourceFramesRequestWire);

/// Generic generation stays owned by the artifact registry while this wrapper
/// revalidates the one already-resolved range at the progressive boundary.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct GenerateArtifactsRequest(ArtifactGenerationRequest);

impl GenerateArtifactsRequest {
    pub fn new(request: ArtifactGenerationRequest) -> Result<Self> {
        validate_resolved_range(request.range())?;
        Ok(Self(request))
    }

    pub const fn request(&self) -> &ArtifactGenerationRequest {
        &self.0
    }

    pub fn into_request(self) -> ArtifactGenerationRequest {
        self.0
    }
}

impl<'de> Deserialize<'de> for GenerateArtifactsRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |request: ArtifactGenerationRequest| {
            Self::new(request)
        })
    }
}

impl schemars::JsonSchema for GenerateArtifactsRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <ArtifactGenerationRequest as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        <ArtifactGenerationRequest as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <ArtifactGenerationRequest as schemars::JsonSchema>::json_schema(generator)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceFrameList {
    pub range: ResolvedRange,
    pub frames: Vec<SourceFrameHandle>,
    pub omitted_frame_count: u64,
    pub next_offset: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceFrameBatch {
    pub range: ResolvedRange,
    pub frames: Vec<SourceFrameRead>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum CallerRegionShape {
    Rect {
        rect: temporal_vision::SignedPixelRect,
    },
    Mask {
        mask: temporal_vision::BinaryMask,
    },
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
enum CallerRegionShapeWire {
    Rect {
        rect: temporal_vision::SignedPixelRect,
    },
    Mask {
        mask: temporal_vision::BinaryMask,
    },
}

impl<'de> Deserialize<'de> for CallerRegionShape {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = match CallerRegionShapeWire::deserialize(deserializer)? {
            CallerRegionShapeWire::Rect { rect } => Self::Rect { rect },
            CallerRegionShapeWire::Mask { mask } => {
                validate_mask(&mask).map_err(D::Error::custom)?;
                Self::Mask { mask }
            }
        };
        Ok(value)
    }
}

delegate_json_schema!(CallerRegionShape => CallerRegionShapeWire);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "region", rename_all = "snake_case")]
pub enum ProgressiveRegion {
    SourcePixels {
        rect: temporal_vision::SignedPixelRect,
        source_frame_id: FrameId,
    },
    ViewportCss {
        rect: CssRect,
        source_frame_id: FrameId,
    },
    SelectedFromSourceFrame {
        source_frame_id: FrameId,
        shape: CallerRegionShape,
    },
    CurrentReference {
        session_id: SessionId,
        reference: NodeReference,
        source_frame_id: FrameId,
    },
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "region", rename_all = "snake_case", deny_unknown_fields)]
enum ProgressiveRegionWire {
    SourcePixels {
        rect: temporal_vision::SignedPixelRect,
        source_frame_id: FrameId,
    },
    ViewportCss {
        rect: CssRect,
        source_frame_id: FrameId,
    },
    SelectedFromSourceFrame {
        source_frame_id: FrameId,
        shape: CallerRegionShape,
    },
    CurrentReference {
        session_id: SessionId,
        reference: NodeReference,
        source_frame_id: FrameId,
    },
}

impl ProgressiveRegion {
    pub const fn source_frame_id(&self) -> FrameId {
        match self {
            Self::SourcePixels {
                source_frame_id, ..
            }
            | Self::ViewportCss {
                source_frame_id, ..
            }
            | Self::SelectedFromSourceFrame {
                source_frame_id, ..
            }
            | Self::CurrentReference {
                source_frame_id, ..
            } => *source_frame_id,
        }
    }

    fn validate_declaration(&self) -> Result<()> {
        let source_frame_id = self.source_frame_id();
        validate_id(&source_frame_id, "region source frame id", FrameId::as_uuid)?;
        match self {
            Self::ViewportCss { rect, .. } => {
                CssRect::new(rect.origin, rect.size)?;
                if !rect.right().is_finite() || !rect.bottom().is_finite() {
                    return Err(invalid("viewport CSS region bounds must be finite"));
                }
                temporal_vision::SignedPixelRect::from_outward_f64_bounds(
                    rect.origin.x,
                    rect.origin.y,
                    rect.right(),
                    rect.bottom(),
                )
                .map_err(vision_input_error)?;
            }
            Self::SelectedFromSourceFrame {
                shape: CallerRegionShape::Mask { mask },
                ..
            } => validate_mask(mask)?,
            Self::CurrentReference {
                session_id,
                reference,
                ..
            } => {
                validate_id(
                    session_id,
                    "current-reference session id",
                    SessionId::as_uuid,
                )?;
                validate_id(
                    &reference.target_id,
                    "current-reference target id",
                    TargetId::as_uuid,
                )?;
            }
            Self::SourcePixels { .. }
            | Self::SelectedFromSourceFrame {
                shape: CallerRegionShape::Rect { .. },
                ..
            } => {}
        }
        Ok(())
    }

    fn validate(&self, range: &ResolvedRange) -> Result<()> {
        self.validate_declaration()?;
        if !range.frame_ids.contains(&self.source_frame_id()) {
            return Err(invalid(
                "region source frame must belong to the resolved range",
            ));
        }
        if let Self::CurrentReference {
            session_id,
            reference,
            ..
        } = self
        {
            if *session_id != range.session_id || reference.target_id != range.target_id {
                return Err(invalid(
                    "current reference must match the resolved session and target",
                ));
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ProgressiveRegion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = ProgressiveRegionWire::deserialize(deserializer)?;
        let value = match wire {
            ProgressiveRegionWire::SourcePixels {
                rect,
                source_frame_id,
            } => Self::SourcePixels {
                rect,
                source_frame_id,
            },
            ProgressiveRegionWire::ViewportCss {
                rect,
                source_frame_id,
            } => Self::ViewportCss {
                rect,
                source_frame_id,
            },
            ProgressiveRegionWire::SelectedFromSourceFrame {
                source_frame_id,
                shape,
            } => Self::SelectedFromSourceFrame {
                source_frame_id,
                shape,
            },
            ProgressiveRegionWire::CurrentReference {
                session_id,
                reference,
                source_frame_id,
            } => Self::CurrentReference {
                session_id,
                reference,
                source_frame_id,
            },
        };
        value.validate_declaration().map_err(D::Error::custom)?;
        Ok(value)
    }
}

delegate_json_schema!(ProgressiveRegion => ProgressiveRegionWire);

fn validate_mask(mask: &temporal_vision::BinaryMask) -> Result<()> {
    let dimensions = mask.dimensions();
    if dimensions.width() > MAX_MASK_DIMENSION || dimensions.height() > MAX_MASK_DIMENSION {
        return Err(invalid("mask dimensions exceed the request ceiling"));
    }
    if dimensions.pixel_count().map_err(vision_input_error)? > MAX_MASK_PIXELS
        || mask.bits().len() > MAX_MASK_BYTES
    {
        return Err(invalid("mask payload exceeds the request ceiling"));
    }
    if mask.bounds().map_err(vision_input_error)?.is_none() {
        return Err(invalid("mask must select at least one source pixel"));
    }
    Ok(())
}

fn vision_input_error(error: temporal_vision::VisionError) -> KrometrailError {
    invalid(error.to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegionFilmstripEvidenceRequest {
    pub range: ResolvedRange,
    pub region: ProgressiveRegion,
    pub markers: Vec<ArtifactMarker>,
    pub anchor: SessionTime,
    pub tile_limit: u8,
    pub background: temporal_vision::Rgb8,
    pub padding: temporal_vision::Rgb8,
    pub display_scale: AnalysisScale,
    pub labels: ArtifactLabelsRequest,
    pub output: OutputLimitsRequest,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RegionFilmstripEvidenceRequestWire {
    range: ResolvedRange,
    region: ProgressiveRegion,
    markers: Vec<ArtifactMarker>,
    anchor: SessionTime,
    tile_limit: u8,
    background: temporal_vision::Rgb8,
    padding: temporal_vision::Rgb8,
    display_scale: AnalysisScale,
    labels: ArtifactLabelsRequest,
    output: OutputLimitsRequest,
}

impl RegionFilmstripEvidenceRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        range: ResolvedRange,
        region: ProgressiveRegion,
        markers: Vec<ArtifactMarker>,
        anchor: SessionTime,
        tile_limit: u8,
        background: temporal_vision::Rgb8,
        padding: temporal_vision::Rgb8,
        display_scale: AnalysisScale,
        labels: ArtifactLabelsRequest,
        output: OutputLimitsRequest,
    ) -> Result<Self> {
        validate_resolved_range(&range)?;
        region.validate(&range)?;
        if !(1..=24).contains(&tile_limit) {
            return Err(invalid(
                "region filmstrip tile limit must be between one and twenty-four",
            ));
        }
        if !range.resolved_range.contains(anchor) {
            return Err(invalid(
                "region filmstrip anchor must belong to the resolved range",
            ));
        }
        display_scale.validate()?;
        if display_scale == AnalysisScale::FitLimits {
            return Err(invalid("region filmstrip display scale must be explicit"));
        }
        let mut marker_ids = HashSet::new();
        for marker in &markers {
            if !marker_ids.insert(marker.id().clone())
                || !range.resolved_range.contains(marker.session_time())
            {
                return Err(invalid(
                    "region filmstrip markers must be unique and inside the range",
                ));
            }
        }
        Ok(Self {
            range,
            region,
            markers,
            anchor,
            tile_limit,
            background,
            padding,
            display_scale,
            labels,
            output,
        })
    }

    /// Proves that store metadata is the exact one-epoch source sequence this
    /// fixed region may use. No geometry is tracked or re-resolved per frame.
    pub fn validate_epoch(&self, frames: &[CapturedFrame]) -> Result<VisualEpoch> {
        if frames.len() != self.range.frame_ids.len() {
            return Err(invalid(
                "region epoch metadata must contain every resolved frame exactly once",
            ));
        }
        for (position, (expected_id, frame)) in self.range.frame_ids.iter().zip(frames).enumerate()
        {
            if frame.id() != *expected_id
                || frame.session_id() != self.range.session_id
                || frame.target_id() != self.range.target_id
            {
                return Err(invalid(format!(
                    "region epoch metadata disagrees with resolved frame at position {position}",
                )));
            }
        }
        if frames
            .windows(2)
            .any(|pair| pair[0].capture_ordinal() >= pair[1].capture_ordinal())
        {
            return Err(invalid(
                "region epoch metadata must use strict capture-ordinal order",
            ));
        }
        let first = frames
            .first()
            .expect("resolved range validation guarantees source frames");
        if frames.iter().any(|frame| {
            frame.image() != first.image()
                || frame.viewport() != first.viewport()
                || frame.device_scale_factor().get().to_bits()
                    != first.device_scale_factor().get().to_bits()
        }) {
            return Err(invalid(
                "region generation requires one exact image, viewport, and scale epoch",
            ));
        }
        if let ProgressiveRegion::SelectedFromSourceFrame {
            shape: CallerRegionShape::Mask { mask },
            ..
        } = &self.region
        {
            if mask.dimensions().width() != first.image().width()
                || mask.dimensions().height() != first.image().height()
            {
                return Err(invalid(
                    "full-frame mask dimensions must match the exact visual epoch",
                ));
            }
            validate_mask(mask)?;
        }
        Ok(VisualEpoch {
            index: 0,
            frame_ids: self.range.frame_ids.clone(),
            image: first.image(),
            viewport: first.viewport(),
            device_scale_factor: first.device_scale_factor(),
        })
    }
}

impl<'de> Deserialize<'de> for RegionFilmstripEvidenceRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: RegionFilmstripEvidenceRequestWire| {
            Self::new(
                wire.range,
                wire.region,
                wire.markers,
                wire.anchor,
                wire.tile_limit,
                wire.background,
                wire.padding,
                wire.display_scale,
                wire.labels,
                wire.output,
            )
        })
    }
}

delegate_json_schema!(RegionFilmstripEvidenceRequest => RegionFilmstripEvidenceRequestWire);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedProgressiveRegion {
    pub declared: ProgressiveRegion,
    pub source_frame: CapturedFrame,
    pub temporal_region: temporal_vision::RegionDefinition,
    pub mask: Option<temporal_vision::BinaryMask>,
    pub viewport_mapping: Option<temporal_vision::ViewportMapping>,
    pub reference_geometry: Option<crate::ResolvedReferenceGeometry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegionFilmstripEvidence {
    pub region: ResolvedProgressiveRegion,
    pub generation: ArtifactGenerationResult,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedRangeEvidenceRequest {
    pub range: ResolvedRange,
}

impl ResolvedRangeEvidenceRequest {
    pub fn new(range: ResolvedRange) -> Result<Self> {
        validate_resolved_range(&range)?;
        Ok(Self { range })
    }

    pub fn pin_request(&self) -> Result<RetentionPinRequest> {
        RetentionPinRequest::from_resolved(&self.range)
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResolvedRangeEvidenceRequestWire {
    range: ResolvedRange,
}

impl<'de> Deserialize<'de> for ResolvedRangeEvidenceRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: ResolvedRangeEvidenceRequestWire| {
            Self::new(wire.range)
        })
    }
}

delegate_json_schema!(ResolvedRangeEvidenceRequest => ResolvedRangeEvidenceRequestWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetentionPinRequest {
    pub request: RetentionRange,
    pub expected_frame_ids: Vec<FrameId>,
}

impl RetentionPinRequest {
    pub fn from_resolved(range: &ResolvedRange) -> Result<Self> {
        validate_resolved_range(range)?;
        Self::new(
            RetentionRange {
                session_id: range.session_id,
                target_id: range.target_id,
                range: range.resolved_range,
            },
            range.frame_ids.clone(),
        )
    }

    pub fn new(request: RetentionRange, expected_frame_ids: Vec<FrameId>) -> Result<Self> {
        validate_id(&request.session_id, "pin session id", SessionId::as_uuid)?;
        validate_id(&request.target_id, "pin target id", TargetId::as_uuid)?;
        if expected_frame_ids.is_empty() {
            return Err(invalid("pin request must expect at least one source frame"));
        }
        let mut seen = HashSet::new();
        for id in &expected_frame_ids {
            validate_id(id, "expected pin frame id", FrameId::as_uuid)?;
            if !seen.insert(*id) {
                return Err(invalid("pin expected frame ids must be unique"));
            }
        }
        Ok(Self {
            request,
            expected_frame_ids,
        })
    }
}

impl<'de> Deserialize<'de> for RetentionPinRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            request: RetentionRange,
            expected_frame_ids: Vec<FrameId>,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(wire.request, wire.expected_frame_ids)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinProtectionScope {
    SourceSegmentsOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProtectedSegment {
    pub segment_id: SegmentId,
    pub retained_range: SessionRange,
    pub byte_len: u64,
}

impl ProtectedSegment {
    pub fn new(segment_id: SegmentId, retained_range: SessionRange, byte_len: u64) -> Result<Self> {
        validate_id(&segment_id, "protected segment id", SegmentId::as_uuid)?;
        if byte_len == 0 {
            return Err(invalid("protected segment byte length must be non-zero"));
        }
        Ok(Self {
            segment_id,
            retained_range,
            byte_len,
        })
    }
}

impl<'de> Deserialize<'de> for ProtectedSegment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            segment_id: SegmentId,
            retained_range: SessionRange,
            byte_len: u64,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(wire.segment_id, wire.retained_range, wire.byte_len)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "snake_case", deny_unknown_fields)]
pub enum RangeEvidenceAvailability {
    Complete,
    PartiallyUnavailable {
        retained_frame_ids: Vec<FrameId>,
        missing_frame_ids: Vec<FrameId>,
    },
    Unavailable {
        missing_frame_ids: Vec<FrameId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PinState {
    pub request: RetentionPinRequest,
    pub exact_pin_active: bool,
    pub evidence: RangeEvidenceAvailability,
    pub protection_scope: PinProtectionScope,
    pub protected_segments: Vec<ProtectedSegment>,
    pub coalesced_protected_ranges: Vec<SessionRange>,
    pub pinned_usage_bytes: u64,
    pub retention: RetentionStatus,
}

impl PinState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request: RetentionPinRequest,
        exact_pin_active: bool,
        evidence: RangeEvidenceAvailability,
        protection_scope: PinProtectionScope,
        protected_segments: Vec<ProtectedSegment>,
        coalesced_protected_ranges: Vec<SessionRange>,
        pinned_usage_bytes: u64,
        retention: RetentionStatus,
    ) -> Result<Self> {
        validate_availability(&request.expected_frame_ids, &evidence)?;
        let mut previous: Option<&ProtectedSegment> = None;
        let mut ids = HashSet::new();
        let mut request_bytes = 0_u64;
        for segment in &protected_segments {
            if !ids.insert(segment.segment_id) {
                return Err(invalid("protected segment ids must be unique"));
            }
            if !ranges_intersect(segment.retained_range, request.request.range) {
                return Err(invalid(
                    "protected segment ranges must intersect the exact request",
                ));
            }
            if previous.is_some_and(|prior| {
                (
                    prior.retained_range.start(),
                    prior.retained_range.end(),
                    prior.segment_id,
                ) > (
                    segment.retained_range.start(),
                    segment.retained_range.end(),
                    segment.segment_id,
                )
            }) {
                return Err(invalid(
                    "protected segments must use deterministic range and id order",
                ));
            }
            request_bytes = request_bytes
                .checked_add(segment.byte_len)
                .ok_or_else(|| invalid("protected segment byte total overflow"))?;
            previous = Some(segment);
        }
        if coalesced_protected_ranges != coalesce_ranges(&protected_segments)? {
            return Err(invalid(
                "coalesced protected ranges must be the true segment union",
            ));
        }
        if request_bytes > pinned_usage_bytes || pinned_usage_bytes != retention.pinned_usage_bytes
        {
            return Err(invalid(
                "pin usage must include request segments and match final retention status",
            ));
        }
        if exact_pin_active
            && (!matches!(evidence, RangeEvidenceAvailability::Complete)
                || protected_segments.is_empty())
        {
            return Err(invalid(
                "an active exact pin must completely protect source segments",
            ));
        }
        Ok(Self {
            request,
            exact_pin_active,
            evidence,
            protection_scope,
            protected_segments,
            coalesced_protected_ranges,
            pinned_usage_bytes,
            retention,
        })
    }
}

impl<'de> Deserialize<'de> for PinState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            request: RetentionPinRequest,
            exact_pin_active: bool,
            evidence: RangeEvidenceAvailability,
            protection_scope: PinProtectionScope,
            protected_segments: Vec<ProtectedSegment>,
            coalesced_protected_ranges: Vec<SessionRange>,
            pinned_usage_bytes: u64,
            retention: RetentionStatus,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(
                wire.request,
                wire.exact_pin_active,
                wire.evidence,
                wire.protection_scope,
                wire.protected_segments,
                wire.coalesced_protected_ranges,
                wire.pinned_usage_bytes,
                wire.retention,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinChange {
    pub changed: bool,
    pub state: PinState,
}

fn validate_availability(
    expected: &[FrameId],
    availability: &RangeEvidenceAvailability,
) -> Result<()> {
    let validate_partition = |retained: &[FrameId], missing: &[FrameId]| -> Result<()> {
        if retained.is_empty() || missing.is_empty() {
            return Err(invalid(
                "partial evidence must contain retained and missing frame ids",
            ));
        }
        let retained_expected: Vec<_> = expected
            .iter()
            .copied()
            .filter(|id| retained.contains(id))
            .collect();
        let missing_expected: Vec<_> = expected
            .iter()
            .copied()
            .filter(|id| missing.contains(id))
            .collect();
        if retained_expected != retained
            || missing_expected != missing
            || retained.len() + missing.len() != expected.len()
            || expected
                .iter()
                .any(|id| retained.contains(id) == missing.contains(id))
        {
            return Err(invalid(
                "evidence availability must partition expected frames in resolved order",
            ));
        }
        Ok(())
    };
    match availability {
        RangeEvidenceAvailability::Complete => Ok(()),
        RangeEvidenceAvailability::PartiallyUnavailable {
            retained_frame_ids,
            missing_frame_ids,
        } => validate_partition(retained_frame_ids, missing_frame_ids),
        RangeEvidenceAvailability::Unavailable { missing_frame_ids } => {
            if missing_frame_ids == expected {
                Ok(())
            } else {
                Err(invalid(
                    "unavailable evidence must report every expected frame in order",
                ))
            }
        }
    }
}

fn ranges_intersect(left: SessionRange, right: SessionRange) -> bool {
    left.start() <= right.end() && right.start() <= left.end()
}

fn coalesce_ranges(segments: &[ProtectedSegment]) -> Result<Vec<SessionRange>> {
    let mut ranges: Vec<_> = segments
        .iter()
        .map(|segment| segment.retained_range)
        .collect();
    ranges.sort_by_key(|range| (range.start(), range.end()));
    let mut coalesced: Vec<SessionRange> = Vec::new();
    for range in ranges {
        if let Some(last) = coalesced.last_mut() {
            let adjacent = last
                .end()
                .as_nanos()
                .checked_add(1)
                .is_some_and(|next| range.start().as_nanos() <= next);
            if range.start() <= last.end() || adjacent {
                *last = SessionRange::new(last.start(), last.end().max(range.end()))?;
                continue;
            }
        }
        coalesced.push(range);
    }
    Ok(coalesced)
}

fn validate_resolved_range(range: &ResolvedRange) -> Result<()> {
    EvidenceScope::from_range(range)?;
    if range.frame_ids.is_empty() {
        return Err(invalid(
            "progressive evidence requires at least one resolved source frame",
        ));
    }
    let mut frame_ids = HashSet::new();
    for frame_id in &range.frame_ids {
        validate_id(frame_id, "resolved source frame id", FrameId::as_uuid)?;
        if !frame_ids.insert(*frame_id) {
            return Err(invalid("resolved source frame ids must be unique"));
        }
    }
    if range.resolved_range.start() < range.requested_range.start()
        || range.resolved_range.end() > range.requested_range.end()
    {
        return Err(invalid(
            "resolved range must remain inside the requested range",
        ));
    }
    let partial = range.resolved_range != range.requested_range;
    if partial && range.options.retention == RetentionPolicy::RequireComplete {
        return Err(invalid(
            "complete retention cannot carry a partial resolved range",
        ));
    }
    if partial == range.retention_warnings.is_empty() {
        return Err(invalid(
            "resolved range retention warnings do not match partial availability",
        ));
    }
    for gap in &range.gaps {
        if gap.session_id() != range.session_id
            || gap.target_id() != range.target_id
            || !ranges_intersect(gap.range(), range.resolved_range)
        {
            return Err(invalid(
                "resolved capture gaps must match and intersect the resolved scope",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationExposure {
    Tool,
    ResourceOnly,
}

macro_rules! define_progressive_evidence_operations {
    (
        $(
            $variant:ident($request:ty) => $result:ty {
                stable_name: $stable_name:literal,
                description: $description:literal,
                capability: $capability:expr,
                mutability: $mutability:expr,
                exposure: $exposure:expr,
            }
        ),+ $(,)?
    ) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum ProgressiveEvidenceOperationKind {
            $($variant),+
        }

        impl ProgressiveEvidenceOperationKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $stable_name),+
                }
            }

            pub fn input_schema(self) -> schemars::Schema {
                match self {
                    $(Self::$variant => schemars::schema_for!($request)),+
                }
            }
        }

        impl Serialize for ProgressiveEvidenceOperationKind {
            fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for ProgressiveEvidenceOperationKind {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                match value.as_str() {
                    $($stable_name => Ok(Self::$variant),)+
                    _ => Err(D::Error::unknown_variant(&value, &[$($stable_name),+])),
                }
            }
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct ProgressiveEvidenceOperationDefinition {
            pub kind: ProgressiveEvidenceOperationKind,
            pub stable_name: &'static str,
            pub description: &'static str,
            pub capability: crate::CapabilityId,
            pub mutability: crate::OperationMutability,
            pub exposure: OperationExposure,
            pub request_type: &'static str,
            pub result_type: &'static str,
        }

        pub const PROGRESSIVE_EVIDENCE_REGISTRY: &[ProgressiveEvidenceOperationDefinition] = &[
            $(ProgressiveEvidenceOperationDefinition {
                kind: ProgressiveEvidenceOperationKind::$variant,
                stable_name: $stable_name,
                description: $description,
                capability: $capability,
                mutability: $mutability,
                exposure: $exposure,
                request_type: stringify!($request),
                result_type: stringify!($result),
            }),+
        ];

        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(tag = "operation", content = "request", deny_unknown_fields)]
        pub enum ProgressiveEvidenceRequest {
            $(#[serde(rename = $stable_name)] $variant($request)),+
        }

        impl ProgressiveEvidenceRequest {
            pub const fn kind(&self) -> ProgressiveEvidenceOperationKind {
                match self {
                    $(Self::$variant(_) => ProgressiveEvidenceOperationKind::$variant),+
                }
            }
        }

        #[derive(Clone, Debug, PartialEq)]
        pub enum ProgressiveEvidenceResult {
            $($variant(Box<$result>)),+
        }

        impl ProgressiveEvidenceResult {
            pub const fn kind(&self) -> ProgressiveEvidenceOperationKind {
                match self {
                    $(Self::$variant(_) => ProgressiveEvidenceOperationKind::$variant),+
                }
            }
        }
    };
}

define_progressive_evidence_operations! {
    RetrieveArtifact(RetrieveArtifactRequest) => ArtifactRead {
        stable_name: "retrieve_artifact",
        description: "Read one retained generated artifact by scoped evidence identity.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::ResourceOnly,
    },
    RetrieveSourceFrame(RetrieveSourceFrameRequest) => SourceFrameRead {
        stable_name: "retrieve_source_frame",
        description: "Read one retained source frame by scoped evidence identity.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::ResourceOnly,
    },
    ListSourceFrames(SourceFramesRequest) => SourceFrameList {
        stable_name: "list_source_frames",
        description: "List retained source-frame metadata for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::Tool,
    },
    FetchSourceFrames(SourceFramesRequest) => SourceFrameBatch {
        stable_name: "fetch_source_frames",
        description: "Fetch selected retained source frames for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::Tool,
    },
    GenerateArtifacts(GenerateArtifactsRequest) => ArtifactGenerationResult {
        stable_name: "generate_artifacts",
        description: "Generate supported visual artifacts for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::Tool,
    },
    GenerateRegionFilmstrip(RegionFilmstripEvidenceRequest) => RegionFilmstripEvidence {
        stable_name: "generate_region_filmstrip",
        description: "Generate a fixed-region filmstrip for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::Tool,
    },
    PinResolvedRange(ResolvedRangeEvidenceRequest) => PinChange {
        stable_name: "pin_resolved_range",
        description: "Protect the source segments for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::StateChanging,
        exposure: OperationExposure::Tool,
    },
    UnpinResolvedRange(ResolvedRangeEvidenceRequest) => PinChange {
        stable_name: "unpin_resolved_range",
        description: "Release the exact retention pin for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::StateChanging,
        exposure: OperationExposure::Tool,
    },
    QueryPinState(ResolvedRangeEvidenceRequest) => PinState {
        stable_name: "query_pin_state",
        description: "Report retention and pin state for a resolved range.",
        capability: crate::CapabilityId::TemporalVision,
        mutability: crate::OperationMutability::ReadOnly,
        exposure: OperationExposure::Tool,
    },
}

#[derive(Clone, Default)]
pub struct ProgressiveEvidenceContext {
    pub deadline: Option<Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
    pub current_reference_geometry: Option<Arc<dyn CurrentReferenceGeometry>>,
}

impl ProgressiveEvidenceContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|signal| signal.is_cancelled())
    }

    pub fn artifact_generation_context(&self) -> ArtifactGenerationContext {
        ArtifactGenerationContext {
            deadline: self.deadline,
            cancellation: self.cancellation.clone(),
            epoch_selection: crate::ArtifactEpochSelection::All,
        }
    }
}

pub trait ProgressiveEvidence: Send + Sync {
    fn execute(
        &self,
        request: ProgressiveEvidenceRequest,
        context: ProgressiveEvidenceContext,
    ) -> PortFuture<'_, Result<ProgressiveEvidenceResult>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityId, CaptureOrdinal, DeviceScaleFactor, DiskBudgetBytes, ErrorCode, ObservedTime,
        PixelDimensions, RangeResolutionOptions, RecordingBudgetState, SourceTime, StorageUsage,
        TemporalRangeAnchorKind,
    };
    use uuid::Uuid;

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }
    fn frame(value: u128) -> FrameId {
        FrameId::from_uuid(Uuid::from_u128(value))
    }
    fn range() -> ResolvedRange {
        ResolvedRange::new(
            session(),
            target(),
            TemporalRangeAnchorKind::SessionTime,
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(5)).unwrap(),
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(5)).unwrap(),
            vec![frame(3), frame(4)],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }
    fn limits() -> SourceReadLimitsRequest {
        SourceReadLimitsRequest::new(2, 1024, 2048).unwrap()
    }

    #[test]
    fn source_limits_name_each_exceeded_value_and_runtime_ceiling() {
        let error = SourceReadLimitsRequest::new(
            MAX_SOURCE_READ_FRAMES + 1,
            MAX_SOURCE_ITEM_BYTES + 1,
            MAX_SOURCE_TOTAL_BYTES + 1,
        )
        .unwrap_err();
        for name in ["max_frames", "max_item_bytes", "max_total_bytes"] {
            assert!(error.message.as_str().contains(name));
        }
        assert!(error.message.as_str().contains("try ≤"));
    }

    #[test]
    fn source_fetch_limit_names_selected_count_and_page_size() {
        let request = SourceFramesRequest::new(
            range(),
            SourceFrameSelection::ResolvedOrder,
            SourceReadLimitsRequest::new(1, 1024, 2048).unwrap(),
        )
        .unwrap();
        let error = request.validate_for_fetch().unwrap_err();
        assert!(
            error
                .message
                .as_str()
                .contains("selected source frame count")
        );
        assert!(error.message.as_str().contains("2"));
        assert!(error.message.as_str().contains("1"));
    }
    fn metadata(id: FrameId, ordinal: u64) -> CapturedFrame {
        CapturedFrame::new(
            id,
            session(),
            target(),
            CaptureOrdinal::new(ordinal).unwrap(),
            Some(SourceTime::from_nanos(1)),
            ObservedTime::from_nanos(3),
            SessionTime::from_nanos(2),
            ImageFormat::Png,
            PixelDimensions::new(4, 4).unwrap(),
            PixelDimensions::new(4, 4).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn registry_is_exhaustive_and_metadata_is_generated_with_each_operation() {
        assert_eq!(PROGRESSIVE_EVIDENCE_REGISTRY.len(), 9);
        assert_eq!(
            PROGRESSIVE_EVIDENCE_REGISTRY
                .iter()
                .map(|entry| entry.stable_name)
                .collect::<Vec<_>>(),
            [
                "retrieve_artifact",
                "retrieve_source_frame",
                "list_source_frames",
                "fetch_source_frames",
                "generate_artifacts",
                "generate_region_filmstrip",
                "pin_resolved_range",
                "unpin_resolved_range",
                "query_pin_state",
            ]
        );
        assert_eq!(
            ProgressiveEvidenceOperationKind::ALL,
            &PROGRESSIVE_EVIDENCE_REGISTRY
                .iter()
                .map(|entry| entry.kind)
                .collect::<Vec<_>>()
        );
        for entry in PROGRESSIVE_EVIDENCE_REGISTRY {
            assert_eq!(entry.kind.as_str(), entry.stable_name);
            assert!(!entry.description.trim().is_empty());
            assert_eq!(entry.capability, CapabilityId::TemporalVision);
            assert!(matches!(
                entry.exposure,
                OperationExposure::Tool | OperationExposure::ResourceOnly
            ));
            let schema = serde_json::to_value(entry.kind.input_schema()).unwrap();
            assert_eq!(schema["type"], "object");
            assert!(!entry.request_type.is_empty());
            assert!(!entry.result_type.is_empty());
            let json = serde_json::to_string(&entry.kind).unwrap();
            assert_eq!(json, format!("\"{}\"", entry.stable_name));
            assert_eq!(
                serde_json::from_str::<ProgressiveEvidenceOperationKind>(&json).unwrap(),
                entry.kind
            );
        }
    }

    #[test]
    fn scoped_source_reads_validate_before_storage_and_round_trip() {
        let request = RetrieveSourceFrameRequest::new(
            EvidenceScope::new(session(), target()).unwrap(),
            frame(3),
            1024,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<RetrieveSourceFrameRequest>(
                serde_json::to_value(&request).unwrap()
            )
            .unwrap(),
            request
        );
        assert!(EvidenceScope::new(SessionId::from_uuid(Uuid::nil()), target()).is_err());
        assert!(
            RetrieveSourceFrameRequest::new(
                EvidenceScope::new(session(), target()).unwrap(),
                FrameId::from_uuid(Uuid::nil()),
                1024,
            )
            .is_err()
        );
        assert!(
            RetrieveSourceFrameRequest::new(
                EvidenceScope::new(session(), target()).unwrap(),
                frame(3),
                0,
            )
            .is_err()
        );
        assert!(
            RetrieveSourceFrameRequest::new(
                EvidenceScope::new(session(), target()).unwrap(),
                frame(3),
                MAX_SOURCE_ITEM_BYTES + 1,
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<RetrieveSourceFrameRequest>(serde_json::json!({
                "scope": { "session_id": session(), "target_id": target() },
                "frame_id": frame(3),
                "max_encoded_bytes": 1024,
                "extra": true
            }))
            .is_err()
        );
    }

    #[test]
    fn serde_revalidates_scope_selection_limits_and_resolved_range() {
        assert!(EvidenceScope::new(SessionId::from_uuid(Uuid::nil()), target()).is_err());
        for malformed in [
            serde_json::json!({"max_frames": 0, "max_item_bytes": 1, "max_total_bytes": 1}),
            serde_json::json!({"max_frames": 65, "max_item_bytes": 1, "max_total_bytes": 1}),
            serde_json::json!({"max_frames": 1, "max_item_bytes": 2, "max_total_bytes": 1}),
            serde_json::json!({"max_frames": 1, "max_item_bytes": 1, "max_total_bytes": 1, "extra": true}),
        ] {
            assert!(serde_json::from_value::<SourceReadLimitsRequest>(malformed).is_err());
        }
        assert!(
            SourceFramesRequest::new(
                range(),
                SourceFrameSelection::Ids(vec![frame(3), frame(3)]),
                limits(),
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<SourceFrameSelection>(serde_json::json!({
                "selection": "ids",
                "frame_ids": []
            }))
            .is_err()
        );
        assert!(
            SourceFramesRequest::new(range(), SourceFrameSelection::Ids(vec![frame(9)]), limits(),)
                .is_err()
        );
        let mut malformed = serde_json::to_value(
            SourceFramesRequest::new(range(), SourceFrameSelection::ResolvedOrder, limits())
                .unwrap(),
        )
        .unwrap();
        malformed["range"]["frame_ids"] = serde_json::json!([]);
        assert!(serde_json::from_value::<SourceFramesRequest>(malformed).is_err());
    }

    #[test]
    fn region_forms_are_fixed_scoped_and_mask_bounded() {
        let css = CssRect::new(
            crate::CssPoint::new(-0.25, 1.1).unwrap(),
            crate::CssSize::new(2.5, 3.1).unwrap(),
        )
        .unwrap();
        let region = ProgressiveRegion::ViewportCss {
            rect: css,
            source_frame_id: frame(3),
        };
        region.validate(&range()).unwrap();
        let rect = temporal_vision::SignedPixelRect::from_outward_f64_bounds(
            css.origin.x,
            css.origin.y,
            css.right(),
            css.bottom(),
        )
        .unwrap();
        assert_eq!(
            (rect.x(), rect.y(), rect.width(), rect.height()),
            (-1, 1, 4, 4)
        );

        let all_zero = temporal_vision::BinaryMask::new(
            temporal_vision::PixelDimensions::new(2, 2).unwrap(),
            [0_u8],
        )
        .unwrap();
        assert!(validate_mask(&all_zero).is_err());
        assert!(
            serde_json::from_value::<CallerRegionShape>(serde_json::json!({
                "shape": "mask",
                "mask": {"dimensions": {"width": 2, "height": 2}, "bits": [0]}
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<ProgressiveRegion>(serde_json::json!({
                "region": "source_pixels",
                "rect": {"x": 0, "y": 0, "width": 1, "height": 1},
                "source_frame_id": frame(3),
                "tracking": true
            }))
            .is_err()
        );
        let mask = temporal_vision::BinaryMask::new(
            temporal_vision::PixelDimensions::new(4, 4).unwrap(),
            [0x80, 0],
        )
        .unwrap();
        let request = RegionFilmstripEvidenceRequest::new(
            range(),
            ProgressiveRegion::SelectedFromSourceFrame {
                source_frame_id: frame(3),
                shape: CallerRegionShape::Mask { mask },
            },
            vec![],
            SessionTime::from_nanos(2),
            2,
            temporal_vision::Rgb8::new(0, 0, 0),
            temporal_vision::Rgb8::new(1, 2, 3),
            AnalysisScale::Identity,
            ArtifactLabelsRequest::new(
                NonEmptyText::new("region").unwrap(),
                NonEmptyText::new("fixture").unwrap(),
            ),
            OutputLimitsRequest::new(1024, 1024, 1_000_000).unwrap(),
        )
        .unwrap();
        assert!(
            request
                .validate_epoch(&[metadata(frame(3), 1), metadata(frame(4), 2)])
                .is_ok()
        );
        assert!(
            request
                .validate_epoch(&[metadata(frame(3), 2), metadata(frame(4), 1)])
                .is_err()
        );

        let wrong_scope = ProgressiveRegion::CurrentReference {
            session_id: session(),
            reference: NodeReference {
                target_id: TargetId::from_uuid(Uuid::from_u128(99)),
                generation: crate::SnapshotGeneration::new(1).unwrap(),
                node_id: crate::SnapshotNodeId::new(1).unwrap(),
            },
            source_frame_id: frame(3),
        };
        assert!(wrong_scope.validate(&range()).is_err());
    }

    #[test]
    fn source_handles_serialize_metadata_but_never_payload_or_location() {
        let bytes = [1_u8, 2, 3];
        let handle = SourceFrameHandle::new(
            frame(3),
            EvidenceScope::new(session(), target()).unwrap(),
            0,
            0,
            NonEmptyText::new("image/png").unwrap(),
            Sha256Digest::digest(&bytes),
            bytes.len() as u64,
            metadata(frame(3), 1),
        )
        .unwrap();
        let json = serde_json::to_string(&handle).unwrap();
        for forbidden in [
            "path",
            "base64",
            "data:",
            "mcp",
            "encoded_bytes",
            "segment_id",
        ] {
            assert!(!json.to_ascii_lowercase().contains(forbidden));
        }
        let read = SourceFrameRead::new(handle.clone(), bytes).unwrap();
        assert_eq!(read.encoded_bytes(), bytes);
        assert_eq!(
            serde_json::from_str::<SourceFrameHandle>(&json).unwrap(),
            handle
        );
    }

    fn retention(pinned_usage_bytes: u64) -> RetentionStatus {
        RetentionStatus::new(
            DiskBudgetBytes::new(10_000).unwrap(),
            StorageUsage::new(500, 10, 0, 0, 0, 0, 0).unwrap(),
            pinned_usage_bytes,
            None,
            None,
            RecordingBudgetState::Available,
            false,
            false,
            0,
            0,
            0,
        )
        .unwrap()
    }

    #[test]
    fn pin_state_proves_partitions_true_unions_overlap_and_idempotence_shape() {
        let request = RetentionPinRequest::new(
            RetentionRange {
                session_id: session(),
                target_id: target(),
                range: SessionRange::new(SessionTime::from_nanos(10), SessionTime::from_nanos(30))
                    .unwrap(),
            },
            vec![frame(3), frame(4)],
        )
        .unwrap();
        let segments = vec![
            ProtectedSegment::new(
                SegmentId::from_uuid(Uuid::from_u128(10)),
                SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(15)).unwrap(),
                100,
            )
            .unwrap(),
            ProtectedSegment::new(
                SegmentId::from_uuid(Uuid::from_u128(11)),
                SessionRange::new(SessionTime::from_nanos(14), SessionTime::from_nanos(20))
                    .unwrap(),
                100,
            )
            .unwrap(),
            ProtectedSegment::new(
                SegmentId::from_uuid(Uuid::from_u128(12)),
                SessionRange::new(SessionTime::from_nanos(25), SessionTime::from_nanos(35))
                    .unwrap(),
                100,
            )
            .unwrap(),
        ];
        let unions = vec![
            SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(20)).unwrap(),
            SessionRange::new(SessionTime::from_nanos(25), SessionTime::from_nanos(35)).unwrap(),
        ];
        let state = PinState::new(
            request.clone(),
            false,
            RangeEvidenceAvailability::Complete,
            PinProtectionScope::SourceSegmentsOnly,
            segments.clone(),
            unions,
            300,
            retention(300),
        )
        .unwrap();
        let change = PinChange {
            changed: false,
            state,
        };
        let json = serde_json::to_string(&change).unwrap();
        assert_eq!(serde_json::from_str::<PinChange>(&json).unwrap(), change);

        let partial = PinState::new(
            request.clone(),
            false,
            RangeEvidenceAvailability::PartiallyUnavailable {
                retained_frame_ids: vec![frame(3)],
                missing_frame_ids: vec![frame(4)],
            },
            PinProtectionScope::SourceSegmentsOnly,
            vec![],
            vec![],
            0,
            retention(0),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<PinState>(&serde_json::to_string(&partial).unwrap()).unwrap(),
            partial
        );
        assert!(
            PinState::new(
                request.clone(),
                false,
                RangeEvidenceAvailability::Unavailable {
                    missing_frame_ids: vec![frame(3), frame(4)],
                },
                PinProtectionScope::SourceSegmentsOnly,
                vec![],
                vec![],
                0,
                retention(0),
            )
            .is_ok()
        );

        assert!(
            PinState::new(
                request.clone(),
                false,
                RangeEvidenceAvailability::PartiallyUnavailable {
                    retained_frame_ids: vec![frame(3)],
                    missing_frame_ids: vec![frame(3)],
                },
                PinProtectionScope::SourceSegmentsOnly,
                vec![],
                vec![],
                0,
                retention(0),
            )
            .is_err()
        );
        assert!(
            PinState::new(
                request,
                false,
                RangeEvidenceAvailability::Complete,
                PinProtectionScope::SourceSegmentsOnly,
                segments,
                vec![
                    SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(35),)
                        .unwrap(),
                ],
                300,
                retention(300),
            )
            .is_err()
        );
    }

    #[test]
    fn stable_invalidation_error_is_available_to_future_adapters() {
        assert_eq!(
            ErrorCode::EvidenceInvalidated.as_str(),
            "evidence_invalidated"
        );
    }
}

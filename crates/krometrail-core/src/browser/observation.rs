use std::{
    collections::HashSet,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    browser::PageSelection,
    error::{KrometrailError, NonEmptyText, Result, invalid},
    ids::{SessionId, TargetId},
    recording::{DeviceScaleFactor, ImageFormat, PixelDimensions},
    time::SessionTime,
    validation::deserialize_validated,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SnapshotGeneration(NonZeroU64);

impl SnapshotGeneration {
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| invalid("snapshot generation must be non-zero"))
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for SnapshotGeneration {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, Self::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SnapshotNodeId(NonZeroU32);

impl SnapshotNodeId {
    pub fn new(value: u32) -> Result<Self> {
        NonZeroU32::new(value)
            .map(Self)
            .ok_or_else(|| invalid("snapshot node id must be non-zero"))
    }
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for SnapshotNodeId {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, Self::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NodeReference {
    pub target_id: TargetId,
    pub generation: SnapshotGeneration,
    pub node_id: SnapshotNodeId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ObservationContext {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub attachment_generation: u64,
    pub started_at: SessionTime,
    pub completed_at: SessionTime,
}

#[derive(Deserialize)]
struct ObservationContextWire {
    session_id: SessionId,
    target_id: TargetId,
    attachment_generation: u64,
    started_at: SessionTime,
    completed_at: SessionTime,
}

impl ObservationContext {
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        attachment_generation: u64,
        started_at: SessionTime,
        completed_at: SessionTime,
    ) -> Result<Self> {
        if started_at > completed_at {
            return Err(invalid("observation start must not exceed completion"));
        }
        Ok(Self {
            session_id,
            target_id,
            attachment_generation,
            started_at,
            completed_at,
        })
    }
}

impl<'de> Deserialize<'de> for ObservationContext {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: ObservationContextWire| {
            Self::new(
                wire.session_id,
                wire.target_id,
                wire.attachment_generation,
                wire.started_at,
                wire.completed_at,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CssPoint {
    pub x: f64,
    pub y: f64,
}

impl CssPoint {
    pub fn new(x: f64, y: f64) -> Result<Self> {
        if x.is_finite() && y.is_finite() {
            Ok(Self { x, y })
        } else {
            Err(invalid("CSS point coordinates must be finite"))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CssSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize)]
struct CssSizeWire {
    width: f64,
    height: f64,
}

impl CssSize {
    pub fn new(width: f64, height: f64) -> Result<Self> {
        if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
            Ok(Self { width, height })
        } else {
            Err(invalid("CSS size must be finite and positive"))
        }
    }
}
impl<'de> Deserialize<'de> for CssSize {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: CssSizeWire| {
            Self::new(wire.width, wire.height)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CssRect {
    pub origin: CssPoint,
    pub size: CssSize,
}
#[derive(Deserialize)]
struct CssRectWire {
    origin: CssPoint,
    size: CssSize,
}
impl CssRect {
    pub fn new(origin: CssPoint, size: CssSize) -> Result<Self> {
        CssPoint::new(origin.x, origin.y)?;
        CssSize::new(size.width, size.height)?;
        Ok(Self { origin, size })
    }
    pub fn right(self) -> f64 {
        self.origin.x + self.size.width
    }
    pub fn bottom(self) -> f64 {
        self.origin.y + self.size.height
    }
}
impl<'de> Deserialize<'de> for CssRect {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: CssRectWire| {
            Self::new(wire.origin, wire.size)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ViewportState {
    pub layout_viewport: CssRect,
    pub visual_viewport: CssRect,
    pub content_size: CssSize,
    pub device_scale_factor: DeviceScaleFactor,
    pub page_scale_factor: f64,
}
#[derive(Deserialize)]
struct ViewportStateWire {
    layout_viewport: CssRect,
    visual_viewport: CssRect,
    content_size: CssSize,
    device_scale_factor: DeviceScaleFactor,
    page_scale_factor: f64,
}
impl ViewportState {
    pub fn new(
        layout_viewport: CssRect,
        visual_viewport: CssRect,
        content_size: CssSize,
        device_scale_factor: DeviceScaleFactor,
        page_scale_factor: f64,
    ) -> Result<Self> {
        if !page_scale_factor.is_finite() || page_scale_factor <= 0.0 {
            return Err(invalid("page scale factor must be finite and positive"));
        }
        Ok(Self {
            layout_viewport,
            visual_viewport,
            content_size,
            device_scale_factor,
            page_scale_factor,
        })
    }
}
impl<'de> Deserialize<'de> for ViewportState {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |w: ViewportStateWire| {
            Self::new(
                w.layout_viewport,
                w.visual_viewport,
                w.content_size,
                w.device_scale_factor,
                w.page_scale_factor,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentReadiness {
    Loading,
    Interactive,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NavigationState {
    pub current_entry_index: u32,
    pub entry_count: NonZeroU32,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub readiness: DocumentReadiness,
}
#[derive(Deserialize)]
struct NavigationStateWire {
    current_entry_index: u32,
    entry_count: NonZeroU32,
    can_go_back: bool,
    can_go_forward: bool,
    readiness: DocumentReadiness,
}
impl NavigationState {
    pub fn new(
        current_entry_index: u32,
        entry_count: u32,
        readiness: DocumentReadiness,
    ) -> Result<Self> {
        let entry_count = NonZeroU32::new(entry_count)
            .ok_or_else(|| invalid("navigation history must contain an entry"))?;
        if current_entry_index >= entry_count.get() {
            return Err(invalid("navigation history index is out of range"));
        }
        Ok(Self {
            current_entry_index,
            entry_count,
            can_go_back: current_entry_index > 0,
            can_go_forward: current_entry_index + 1 < entry_count.get(),
            readiness,
        })
    }
}
impl<'de> Deserialize<'de> for NavigationState {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |w: NavigationStateWire| {
            let value = Self::new(w.current_entry_index, w.entry_count.get(), w.readiness)?;
            if value.can_go_back != w.can_go_back || value.can_go_forward != w.can_go_forward {
                return Err(invalid("navigation capability flags do not match history"));
            }
            Ok(value)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageState {
    pub context: ObservationContext,
    pub url: String,
    pub title: String,
    pub viewport: ViewportState,
    pub navigation: NavigationState,
}
#[derive(Deserialize)]
struct PageStateWire {
    context: ObservationContext,
    url: String,
    title: String,
    viewport: ViewportState,
    navigation: NavigationState,
}
impl PageState {
    pub fn new(
        context: ObservationContext,
        url: impl Into<String>,
        title: impl Into<String>,
        viewport: ViewportState,
        navigation: NavigationState,
    ) -> Result<Self> {
        let url = url.into();
        if url.trim().is_empty() {
            return Err(invalid("page URL must not be empty"));
        }
        Ok(Self {
            context,
            url,
            title: title.into(),
            viewport,
            navigation,
        })
    }
}
impl<'de> Deserialize<'de> for PageState {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |w: PageStateWire| {
            Self::new(w.context, w.url, w.title, w.viewport, w.navigation)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccessibleProperty {
    pub name: String,
    pub value: AccessibleValue,
}
impl AccessibleProperty {
    pub fn new(name: impl Into<String>, value: AccessibleValue) -> Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            Err(invalid("accessible property name must not be empty"))
        } else {
            Ok(Self { name, value })
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AccessibleValue {
    Boolean(bool),
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub id: SnapshotNodeId,
    pub parent: Option<SnapshotNodeId>,
    pub depth: u16,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub properties: Vec<AccessibleProperty>,
    pub actionable: bool,
    pub reference: Option<NodeReference>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageSnapshot {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub nodes: Vec<SnapshotNode>,
    pub omitted_node_count: u32,
}
#[derive(Deserialize)]
struct PageSnapshotWire {
    context: ObservationContext,
    generation: SnapshotGeneration,
    nodes: Vec<SnapshotNode>,
    omitted_node_count: u32,
}
impl PageSnapshot {
    pub fn new(
        context: ObservationContext,
        generation: SnapshotGeneration,
        nodes: Vec<SnapshotNode>,
        omitted_node_count: u32,
    ) -> Result<Self> {
        let mut seen = HashSet::new();
        for node in &nodes {
            if node.role.trim().is_empty() {
                return Err(invalid("snapshot node role must not be empty"));
            }
            if !seen.insert(node.id) {
                return Err(invalid("snapshot node ids must be unique"));
            }
            match node.parent {
                None if node.depth != 0 => {
                    return Err(invalid("root snapshot nodes must have depth zero"));
                }
                Some(parent) if !seen.contains(&parent) => {
                    return Err(invalid("snapshot parents must precede children"));
                }
                Some(parent) => {
                    let parent_depth = nodes
                        .iter()
                        .find(|candidate| candidate.id == parent)
                        .map(|candidate| candidate.depth)
                        .ok_or_else(|| invalid("snapshot parent is missing"))?;
                    if node.depth != parent_depth.saturating_add(1) {
                        return Err(invalid("snapshot node depth does not follow its parent"));
                    }
                }
                None => {}
            }
            if node.actionable != node.reference.is_some() {
                return Err(invalid(
                    "actionable snapshot nodes must have exactly one reference",
                ));
            }
            if let Some(reference) = node.reference {
                if reference.target_id != context.target_id
                    || reference.generation != generation
                    || reference.node_id != node.id
                {
                    return Err(invalid("snapshot reference scope does not match its node"));
                }
            }
            for property in &node.properties {
                if property.name.trim().is_empty() {
                    return Err(invalid("accessible property name must not be empty"));
                }
                if let AccessibleValue::Number(value) = property.value {
                    if !value.is_finite() {
                        return Err(invalid("accessible numeric properties must be finite"));
                    }
                }
            }
        }
        Ok(Self {
            context,
            generation,
            nodes,
            omitted_node_count,
        })
    }
}
impl<'de> Deserialize<'de> for PageSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |w: PageSnapshotWire| {
            Self::new(w.context, w.generation, w.nodes, w.omitted_node_count)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ElementLocator {
    Reference(NodeReference),
    CssSelector(NonEmptyText),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    ViewportCss,
    DocumentCss,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ScreenshotTarget {
    Viewport,
    FullPage,
    Element(ElementLocator),
    Region {
        rect: CssRect,
        space: CoordinateSpace,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScreenshotRequest {
    pub page: PageSelection,
    pub target: ScreenshotTarget,
    pub format: ImageFormat,
    pub jpeg_quality: Option<u8>,
}
#[derive(Deserialize)]
struct ScreenshotRequestWire {
    page: PageSelection,
    target: ScreenshotTarget,
    format: ImageFormat,
    jpeg_quality: Option<u8>,
}
impl ScreenshotRequest {
    pub fn new(
        target_id: TargetId,
        target: ScreenshotTarget,
        format: ImageFormat,
        jpeg_quality: Option<u8>,
    ) -> Result<Self> {
        Self::for_selection(
            PageSelection::Target(target_id),
            target,
            format,
            jpeg_quality,
        )
    }

    pub fn for_selection(
        page: PageSelection,
        target: ScreenshotTarget,
        format: ImageFormat,
        jpeg_quality: Option<u8>,
    ) -> Result<Self> {
        match (format, jpeg_quality) {
            (ImageFormat::Png, Some(_)) => {
                return Err(invalid("PNG screenshots cannot specify JPEG quality"));
            }
            (ImageFormat::Jpeg, Some(quality)) if quality > 100 => {
                return Err(invalid("JPEG quality must be between 0 and 100"));
            }
            _ => {}
        }
        Ok(Self {
            page,
            target,
            format,
            jpeg_quality,
        })
    }
}
impl<'de> Deserialize<'de> for ScreenshotRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: ScreenshotRequestWire| {
            Self::for_selection(w.page, w.target, w.format, w.jpeg_quality)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScreenshotMetadata {
    pub context: ObservationContext,
    pub requested_target: ScreenshotTarget,
    pub resolved_document_rect: CssRect,
    pub image: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
}
#[derive(Deserialize)]
struct ScreenshotMetadataWire {
    context: ObservationContext,
    requested_target: ScreenshotTarget,
    resolved_document_rect: CssRect,
    image: PixelDimensions,
    device_scale_factor: DeviceScaleFactor,
}
impl ScreenshotMetadata {
    pub fn new(
        context: ObservationContext,
        requested_target: ScreenshotTarget,
        resolved_document_rect: CssRect,
        image: PixelDimensions,
        device_scale_factor: DeviceScaleFactor,
    ) -> Result<Self> {
        Ok(Self {
            context,
            requested_target,
            resolved_document_rect,
            image,
            device_scale_factor,
        })
    }
}
impl<'de> Deserialize<'de> for ScreenshotMetadata {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: ScreenshotMetadataWire| {
            Self::new(
                w.context,
                w.requested_target,
                w.resolved_document_rect,
                w.image,
                w.device_scale_factor,
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncodedScreenshot {
    metadata: ScreenshotMetadata,
    bytes: Arc<[u8]>,
}
impl EncodedScreenshot {
    pub fn new(metadata: ScreenshotMetadata, bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(invalid("encoded screenshot payload must not be empty"));
        }
        Ok(Self { metadata, bytes })
    }
    pub fn metadata(&self) -> &ScreenshotMetadata {
        &self.metadata
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

macro_rules! page_request {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub target: PageSelection,
        }

        impl $name {
            pub const fn new(target_id: TargetId) -> Self {
                Self {
                    target: PageSelection::Target(target_id),
                }
            }
        }
    };
}
page_request!(InspectPageRequest);
page_request!(SnapshotPageRequest);
page_request!(LiveObservationRequest);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReadOnlyEvaluationRequest {
    pub target: PageSelection,
    pub expression: NonEmptyText,
    pub await_promise: bool,
}
#[derive(Deserialize)]
struct ReadOnlyEvaluationRequestWire {
    target: PageSelection,
    expression: NonEmptyText,
    await_promise: bool,
}
impl ReadOnlyEvaluationRequest {
    pub fn new(
        target_id: TargetId,
        expression: impl Into<String>,
        await_promise: bool,
    ) -> Result<Self> {
        Ok(Self {
            target: PageSelection::Target(target_id),
            expression: NonEmptyText::new(expression.into())
                .map_err(|_| invalid("evaluation expression must not be empty"))?,
            await_promise,
        })
    }
}
impl<'de> Deserialize<'de> for ReadOnlyEvaluationRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: ReadOnlyEvaluationRequestWire| {
            if w.expression.as_str().trim().is_empty() {
                return Err(invalid("evaluation expression must not be empty"));
            }
            Ok(Self {
                target: w.target,
                expression: w.expression,
                await_promise: w.await_promise,
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum EvaluationValue {
    Undefined,
    Json(serde_json::Value),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub context: ObservationContext,
    pub value: EvaluationValue,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ObservationPart<T> {
    Available(T),
    Unavailable(KrometrailError),
}
#[derive(Clone, Debug, PartialEq)]
pub struct LiveObservation {
    pub context: ObservationContext,
    pub page: ObservationPart<PageState>,
    pub snapshot: ObservationPart<PageSnapshot>,
    pub screenshot: ObservationPart<EncodedScreenshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
    fn session() -> SessionId {
        SessionId::from_uuid(UUID.parse().unwrap())
    }
    fn target() -> TargetId {
        TargetId::from_uuid(UUID.parse().unwrap())
    }
    fn context() -> ObservationContext {
        ObservationContext::new(
            session(),
            target(),
            2,
            SessionTime::from_nanos(3),
            SessionTime::from_nanos(4),
        )
        .unwrap()
    }

    #[test]
    fn boundary_scalars_reject_invalid_direct_and_wire_values() {
        assert!(SnapshotGeneration::new(0).is_err());
        assert!(SnapshotNodeId::new(0).is_err());
        assert!(CssPoint::new(f64::NAN, 0.0).is_err());
        assert!(CssSize::new(0.0, 1.0).is_err());
        assert!(
            ObservationContext::new(
                session(),
                target(),
                0,
                SessionTime::from_nanos(2),
                SessionTime::from_nanos(1)
            )
            .is_err()
        );
        assert!(serde_json::from_str::<CssSize>(r#"{"width":0,"height":1}"#).is_err());
        assert!(serde_json::from_str::<ObservationContext>(&format!(r#"{{"session_id":"{}","target_id":"{}","attachment_generation":1,"started_at":2,"completed_at":1}}"#, session(), target())).is_err());
    }

    #[test]
    fn snapshot_enforces_preorder_scope_and_actionable_references() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root_id = SnapshotNodeId::new(1).unwrap();
        let child_id = SnapshotNodeId::new(2).unwrap();
        let nodes = vec![
            SnapshotNode {
                id: root_id,
                parent: None,
                depth: 0,
                role: "document".into(),
                name: None,
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
            },
            SnapshotNode {
                id: child_id,
                parent: Some(root_id),
                depth: 1,
                role: "button".into(),
                name: Some("Save".into()),
                value: None,
                description: None,
                properties: vec![],
                actionable: true,
                reference: Some(NodeReference {
                    target_id: target(),
                    generation,
                    node_id: child_id,
                }),
            },
        ];
        let snapshot = PageSnapshot::new(context(), generation, nodes.clone(), 0).unwrap();
        assert_eq!(
            serde_json::from_str::<PageSnapshot>(&serde_json::to_string(&snapshot).unwrap())
                .unwrap(),
            snapshot
        );
        let mut malformed = nodes;
        malformed[1].reference = None;
        assert!(PageSnapshot::new(context(), generation, malformed, 0).is_err());
    }

    #[test]
    fn screenshot_and_evaluation_requests_validate_external_options() {
        assert!(
            ScreenshotRequest::new(
                target(),
                ScreenshotTarget::Viewport,
                ImageFormat::Png,
                Some(80)
            )
            .is_err()
        );
        assert!(
            ScreenshotRequest::new(
                target(),
                ScreenshotTarget::Viewport,
                ImageFormat::Jpeg,
                Some(101)
            )
            .is_err()
        );
        assert!(ReadOnlyEvaluationRequest::new(target(), " ", false).is_err());
        let metadata = ScreenshotMetadata::new(
            context(),
            ScreenshotTarget::Viewport,
            CssRect::new(
                CssPoint::new(0.0, 0.0).unwrap(),
                CssSize::new(10.0, 10.0).unwrap(),
            )
            .unwrap(),
            PixelDimensions::new(10, 10).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
        )
        .unwrap();
        assert!(EncodedScreenshot::new(metadata, Vec::<u8>::new()).is_err());
    }
}

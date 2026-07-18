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
    validation::{delegate_json_schema, deserialize_validated},
};

pub const DEFAULT_SEMANTIC_MATCH_LIMIT: u16 = 20;
pub const MAX_SEMANTIC_MATCH_LIMIT: u16 = 100;
pub const MAX_SEMANTIC_QUERY_TEXT_BYTES: usize = 1_024;

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
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

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeReference {
    pub target_id: TargetId,
    pub generation: SnapshotGeneration,
    pub node_id: SnapshotNodeId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTextMatchMode {
    #[default]
    Exact,
    Contains,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SemanticTextMatch {
    pub value: NonEmptyText,
    pub mode: SemanticTextMatchMode,
    pub case_sensitive: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SemanticTextMatchWire {
    #[schemars(length(min = 1, max = 1_024))]
    value: String,
    #[serde(default)]
    mode: SemanticTextMatchMode,
    #[serde(default)]
    case_sensitive: bool,
}

impl SemanticTextMatch {
    pub fn new(
        value: impl Into<String>,
        mode: SemanticTextMatchMode,
        case_sensitive: bool,
    ) -> Result<Self> {
        let value = value.into();
        if value.len() > MAX_SEMANTIC_QUERY_TEXT_BYTES {
            return Err(invalid(
                "semantic query text must not exceed 1024 UTF-8 bytes",
            ));
        }
        let value = NonEmptyText::new(value)
            .map_err(|_| invalid("semantic query text must not be empty"))?;
        Ok(Self {
            value,
            mode,
            case_sensitive,
        })
    }

    pub fn matches(&self, candidate: &str) -> bool {
        let expected = normalize_semantic_text(self.value.as_str(), self.case_sensitive);
        let candidate = normalize_semantic_text(candidate, self.case_sensitive);
        match self.mode {
            SemanticTextMatchMode::Exact => candidate == expected,
            SemanticTextMatchMode::Contains => candidate.contains(&expected),
        }
    }
}

delegate_json_schema!(SemanticTextMatch => SemanticTextMatchWire);

impl<'de> Deserialize<'de> for SemanticTextMatch {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: SemanticTextMatchWire| {
            Self::new(wire.value, wire.mode, wire.case_sensitive)
        })
    }
}

fn normalize_semantic_text(value: &str, case_sensitive: bool) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.trim().chars() {
        if character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        if case_sensitive {
            normalized.push(character);
        } else {
            normalized.extend(character.to_lowercase());
        }
    }
    normalized
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticQuery {
    Role {
        role: NonEmptyText,
        name: Option<SemanticTextMatch>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container_text: Option<SemanticTextMatch>,
    },
    Label {
        text: SemanticTextMatch,
    },
    Text {
        text: SemanticTextMatch,
    },
    TestId {
        value: NonEmptyText,
    },
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SemanticQueryWire {
    Role {
        #[schemars(length(min = 1, max = 1_024))]
        role: String,
        name: Option<SemanticTextMatch>,
        #[serde(default)]
        container_text: Option<SemanticTextMatch>,
    },
    Label {
        text: SemanticTextMatch,
    },
    Text {
        text: SemanticTextMatch,
    },
    TestId {
        #[schemars(length(min = 1, max = 1_024))]
        value: String,
    },
}

impl SemanticQuery {
    pub fn role(role: impl Into<String>, name: Option<SemanticTextMatch>) -> Result<Self> {
        let role = validate_semantic_role(role.into())?;
        Ok(Self::Role {
            role,
            name,
            container_text: None,
        })
    }

    pub fn role_in_container(
        role: impl Into<String>,
        name: Option<SemanticTextMatch>,
        container_text: SemanticTextMatch,
    ) -> Result<Self> {
        let role = validate_semantic_role(role.into())?;
        Ok(Self::Role {
            role,
            name,
            container_text: Some(container_text),
        })
    }

    pub fn test_id(value: impl Into<String>) -> Result<Self> {
        let value = validate_semantic_query_value(value.into(), "test identifier")?;
        Ok(Self::TestId { value })
    }
}

delegate_json_schema!(SemanticQuery => SemanticQueryWire);

impl<'de> Deserialize<'de> for SemanticQuery {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: SemanticQueryWire| match wire {
            SemanticQueryWire::Role {
                role,
                name,
                container_text,
            } => match container_text {
                Some(container_text) => Self::role_in_container(role, name, container_text),
                None => Self::role(role, name),
            },
            SemanticQueryWire::Label { text } => Ok(Self::Label { text }),
            SemanticQueryWire::Text { text } => Ok(Self::Text { text }),
            SemanticQueryWire::TestId { value } => Self::test_id(value),
        })
    }
}

fn validate_semantic_role(value: String) -> Result<NonEmptyText> {
    if value.len() > MAX_SEMANTIC_QUERY_TEXT_BYTES {
        return Err(invalid("semantic role must not exceed 1024 UTF-8 bytes"));
    }
    if value.chars().any(|character| {
        !character.is_ascii()
            || character.is_ascii_uppercase()
            || character.is_ascii_whitespace()
            || character.is_ascii_control()
    }) {
        return Err(invalid(
            "semantic role must be lowercase ASCII without whitespace or control characters",
        ));
    }
    NonEmptyText::new(value).map_err(|_| invalid("semantic role must not be empty"))
}

fn validate_semantic_query_value(value: String, field: &str) -> Result<NonEmptyText> {
    if value.len() > MAX_SEMANTIC_QUERY_TEXT_BYTES {
        return Err(invalid(format!(
            "semantic {field} must not exceed 1024 UTF-8 bytes"
        )));
    }
    NonEmptyText::new(value).map_err(|_| invalid(format!("semantic {field} must not be empty")))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QueryPageRequest {
    pub target: PageSelection,
    pub document: crate::browser::SemanticDocumentScope,
    pub query: SemanticQuery,
    pub scope: Option<NodeReference>,
    pub max_matches: u16,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct QueryPageRequestWire {
    #[serde(default)]
    target: PageSelection,
    #[serde(default)]
    document: crate::browser::SemanticDocumentScope,
    query: SemanticQuery,
    scope: Option<NodeReference>,
    #[serde(default = "default_semantic_match_limit")]
    #[schemars(range(min = 1_u16, max = 100_u16))]
    max_matches: u16,
}

const fn default_semantic_match_limit() -> u16 {
    DEFAULT_SEMANTIC_MATCH_LIMIT
}

impl QueryPageRequest {
    pub fn new(
        target: PageSelection,
        query: SemanticQuery,
        scope: Option<NodeReference>,
        max_matches: u16,
    ) -> Result<Self> {
        if !(1..=MAX_SEMANTIC_MATCH_LIMIT).contains(&max_matches) {
            return Err(invalid("semantic match limit must be between 1 and 100"));
        }
        if let (PageSelection::Target(target_id), Some(scope)) = (target, scope)
            && scope.target_id != target_id
        {
            return Err(invalid("semantic query scope targets another page"));
        }
        Ok(Self {
            target,
            document: crate::browser::SemanticDocumentScope::MainDocument,
            query,
            scope,
            max_matches,
        })
    }
}

delegate_json_schema!(QueryPageRequest => QueryPageRequestWire);

impl<'de> Deserialize<'de> for QueryPageRequest {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: QueryPageRequestWire| {
            Self::new(wire.target, wire.query, wire.scope, wire.max_matches).map(|mut request| {
                request.document = wire.document;
                request
            })
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticQueryOutcome {
    NoMatch,
    Unique,
    Ambiguous,
    Truncated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SemanticMatch {
    pub reference: NodeReference,
    pub role: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QueryPageResult {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub outcome: SemanticQueryOutcome,
    pub matches: Vec<SemanticMatch>,
    pub omitted_match_count: u32,
}

impl QueryPageResult {
    pub fn new(
        context: ObservationContext,
        generation: SnapshotGeneration,
        mut matches: Vec<SemanticMatch>,
        max_matches: u16,
    ) -> Result<Self> {
        if !(1..=MAX_SEMANTIC_MATCH_LIMIT).contains(&max_matches) {
            return Err(invalid("semantic match limit must be between 1 and 100"));
        }
        if matches.iter().any(|candidate| {
            candidate.role.trim().is_empty()
                || candidate.reference.target_id != context.target_id
                || candidate.reference.generation != generation
        }) {
            return Err(invalid(
                "semantic matches do not belong to the result snapshot",
            ));
        }
        let total = matches.len();
        let retained = total.min(usize::from(max_matches));
        matches.truncate(retained);
        let omitted_match_count = u32::try_from(total - retained).unwrap_or(u32::MAX);
        let outcome = match total {
            0 => SemanticQueryOutcome::NoMatch,
            1 => SemanticQueryOutcome::Unique,
            count if count <= usize::from(max_matches) => SemanticQueryOutcome::Ambiguous,
            _ => SemanticQueryOutcome::Truncated,
        };
        Ok(Self {
            context,
            generation,
            outcome,
            matches,
            omitted_match_count,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ElementLocator {
    Reference(NodeReference),
    CssSelector(NonEmptyText),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    ViewportCss,
    DocumentCss,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Deserialize, schemars::JsonSchema)]
struct ScreenshotRequestWire {
    #[serde(default)]
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
delegate_json_schema!(ScreenshotRequest => ScreenshotRequestWire);

impl<'de> Deserialize<'de> for ScreenshotRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |w: ScreenshotRequestWire| {
            Self::for_selection(w.page, w.target, w.format, w.jpeg_quality)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
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
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
        pub struct $name {
            #[serde(default)]
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
#[derive(Deserialize, schemars::JsonSchema)]
struct ReadOnlyEvaluationRequestWire {
    #[serde(default)]
    target: PageSelection,
    expression: NonEmptyText,
    #[serde(default)]
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
delegate_json_schema!(ReadOnlyEvaluationRequest => ReadOnlyEvaluationRequestWire);

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
    fn semantic_text_matching_normalizes_whitespace_and_unicode_case() {
        let exact = SemanticTextMatch::new(
            "  STRAßE\n  SPEICHERN ",
            SemanticTextMatchMode::Exact,
            false,
        )
        .unwrap();
        assert!(exact.matches("straße   speichern"));
        assert!(!exact.matches("straße speichern jetzt"));

        let contains =
            SemanticTextMatch::new("ΣΩΣ", SemanticTextMatchMode::Contains, false).unwrap();
        assert!(contains.matches("prefix σωσ suffix"));

        let sensitive = SemanticTextMatch::new("Save", SemanticTextMatchMode::Exact, true).unwrap();
        assert!(!sensitive.matches("save"));
    }

    #[test]
    fn semantic_query_wire_defaults_and_bounds_are_validated() {
        let request: QueryPageRequest = serde_json::from_str(
            r#"{"query":{"kind":"role","role":"button","name":{"value":" Save\n now "},"container_text":{"value":"Todo one","mode":"contains"}}}"#,
        )
        .unwrap();
        assert_eq!(request.target, PageSelection::Selected);
        assert_eq!(request.max_matches, DEFAULT_SEMANTIC_MATCH_LIMIT);
        let SemanticQuery::Role {
            name: Some(name),
            container_text: Some(container_text),
            ..
        } = request.query
        else {
            panic!("expected role query with name");
        };
        assert_eq!(name.mode, SemanticTextMatchMode::Exact);
        assert!(!name.case_sensitive);
        assert!(name.matches("save now"));
        assert_eq!(container_text.mode, SemanticTextMatchMode::Contains);
        assert!(container_text.matches("first Todo one item"));

        let legacy = SemanticQuery::role("button", None).unwrap();
        assert!(
            serde_json::to_value(legacy)
                .unwrap()
                .get("container_text")
                .is_none()
        );

        for invalid in [
            r#"{"query":{"kind":"role","role":"Button"}}"#,
            r#"{"query":{"kind":"role","role":"push button"}}"#,
            r#"{"query":{"kind":"text","text":{"value":"x"}},"max_matches":0}"#,
            r#"{"query":{"kind":"text","text":{"value":"x"}},"max_matches":101}"#,
            r#"{"query":{"kind":"test_id","value":"x","unused":true}}"#,
        ] {
            assert!(serde_json::from_str::<QueryPageRequest>(invalid).is_err());
        }
        assert!(
            SemanticTextMatch::new(
                "x".repeat(MAX_SEMANTIC_QUERY_TEXT_BYTES + 1),
                SemanticTextMatchMode::Exact,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn semantic_result_outcomes_are_derived_from_complete_candidate_count() {
        let generation = SnapshotGeneration::new(7).unwrap();
        let candidate = |node_id| SemanticMatch {
            reference: NodeReference {
                target_id: target(),
                generation,
                node_id: SnapshotNodeId::new(node_id).unwrap(),
            },
            role: "button".into(),
            name: Some(format!("button {node_id}")),
        };

        let no_match = QueryPageResult::new(context(), generation, vec![], 2).unwrap();
        assert_eq!(no_match.outcome, SemanticQueryOutcome::NoMatch);
        let unique = QueryPageResult::new(context(), generation, vec![candidate(1)], 2).unwrap();
        assert_eq!(unique.outcome, SemanticQueryOutcome::Unique);
        let ambiguous =
            QueryPageResult::new(context(), generation, vec![candidate(1), candidate(2)], 2)
                .unwrap();
        assert_eq!(ambiguous.outcome, SemanticQueryOutcome::Ambiguous);
        let truncated = QueryPageResult::new(
            context(),
            generation,
            vec![candidate(1), candidate(2), candidate(3)],
            2,
        )
        .unwrap();
        assert_eq!(truncated.outcome, SemanticQueryOutcome::Truncated);
        assert_eq!(truncated.matches.len(), 2);
        assert_eq!(truncated.omitted_match_count, 1);
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

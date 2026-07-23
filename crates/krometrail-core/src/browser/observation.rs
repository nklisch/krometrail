use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    browser::PageSelection,
    error::{ErrorCode, KrometrailError, NonEmptyText, Result, RetryAdvice, invalid},
    ids::{SessionId, TargetId},
    recording::{DeviceScaleFactor, ImageFormat, PixelDimensions},
    time::SessionTime,
    validation::{delegate_json_schema, deserialize_validated},
};

pub const DEFAULT_SEMANTIC_MATCH_LIMIT: u16 = 20;
pub const MAX_SEMANTIC_MATCH_LIMIT: u16 = 100;
/// The declared bound on the `no_match` relaxed-candidate scan. Counting stops here and the
/// reported count is marked saturated rather than becoming an unbounded page scan.
pub const MAX_SEMANTIC_RELAXED_CANDIDATES: u16 = 100;
pub const MAX_SEMANTIC_QUERY_TEXT_BYTES: usize = 1_024;
/// The declared bound on generic-ancestor container eligibility. A generic-role ancestor may
/// qualify a container-text query only while its whitespace-collapsed rendered text fits this
/// many UTF-8 bytes; page-scale containers exceed it and never qualify.
pub const MAX_GENERIC_CONTAINER_TEXT_BYTES: usize = 1_024;

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
            Err(KrometrailError::limit_exceeded(
                ErrorCode::InvalidInput,
                "CSS size",
                format!("{width}×{height}"),
                "finite and positive width×height",
                None::<String>,
            )
            .with_retry(RetryAdvice::AfterRecovery)
            .with_recovery(
                NonEmptyText::new(
                    "reload or navigate the page; a cross-origin navigation restores observation when a same-origin reload does not",
                )
                .expect("CSS size recovery is non-empty"),
            ))
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_rect: Option<CssRect>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageSnapshot {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub nodes: Vec<SnapshotNode>,
    pub omitted_node_count: u32,
    #[serde(skip_serializing_if = "is_false")]
    pub geometry_omitted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_viewport: Option<CssRect>,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Deserialize)]
struct PageSnapshotWire {
    context: ObservationContext,
    generation: SnapshotGeneration,
    nodes: Vec<SnapshotNode>,
    omitted_node_count: u32,
    #[serde(default)]
    geometry_omitted: bool,
    #[serde(default)]
    visual_viewport: Option<CssRect>,
}
impl PageSnapshot {
    pub fn new(
        context: ObservationContext,
        generation: SnapshotGeneration,
        nodes: Vec<SnapshotNode>,
        omitted_node_count: u32,
    ) -> Result<Self> {
        let mut seen = HashMap::new();
        for node in &nodes {
            if node.role.trim().is_empty() {
                return Err(invalid("snapshot node role must not be empty"));
            }
            if seen.insert(node.id, node.depth).is_some() {
                return Err(invalid("snapshot node ids must be unique"));
            }
            match node.parent {
                None if node.depth != 0 => {
                    return Err(invalid("root snapshot nodes must have depth zero"));
                }
                Some(parent) if !seen.contains_key(&parent) => {
                    return Err(invalid("snapshot parents must precede children"));
                }
                Some(parent) => {
                    let parent_depth = seen
                        .get(&parent)
                        .copied()
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
            geometry_omitted: false,
            visual_viewport: None,
        })
    }

    pub fn with_geometry_omitted(mut self, geometry_omitted: bool) -> Self {
        self.geometry_omitted = geometry_omitted;
        self
    }

    pub fn with_visual_viewport(mut self, visual_viewport: CssRect) -> Self {
        self.visual_viewport = Some(visual_viewport);
        self
    }
}
impl<'de> Deserialize<'de> for PageSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |w: PageSnapshotWire| {
            Self::new(w.context, w.generation, w.nodes, w.omitted_node_count).map(|mut snapshot| {
                snapshot.geometry_omitted = w.geometry_omitted;
                snapshot.visual_viewport = w.visual_viewport;
                snapshot
            })
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

    /// The same matcher with `exact` relaxed to `contains`. `None` when it is already relaxed.
    fn relaxed_to_contains(&self) -> Option<Self> {
        match self.mode {
            SemanticTextMatchMode::Exact => Some(Self {
                value: self.value.clone(),
                mode: SemanticTextMatchMode::Contains,
                case_sensitive: self.case_sensitive,
            }),
            SemanticTextMatchMode::Contains => None,
        }
    }

    pub fn matches(&self, candidate: &str) -> bool {
        let expected = normalize_semantic_text(self.value.as_str(), self.case_sensitive);
        if expected.is_empty() {
            return false;
        }
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
        if is_invisible_format(character) {
            continue;
        }
        if is_private_use(character) {
            pending_space = !normalized.is_empty();
            continue;
        }
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

/// Collapsed byte length of rendered text under the same normalization semantic text matching
/// uses (whitespace collapsing, invisible-format stripping, private-use glyphs as separators),
/// without case folding.
pub fn collapsed_semantic_text_bytes(value: &str) -> usize {
    normalize_semantic_text(value, true).len()
}

const fn is_invisible_format(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{FEFF}'
    )
}

const fn is_private_use(character: char) -> bool {
    let codepoint = character as u32;
    (codepoint >= 0xE000 && codepoint <= 0xF8FF)
        || (codepoint >= 0xF0000 && codepoint <= 0xFFFFD)
        || (codepoint >= 0x100000 && codepoint <= 0x10FFFD)
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
    pub const fn requires_dom_semantics(&self) -> bool {
        match self {
            Self::Role { container_text, .. } => container_text.is_some(),
            Self::Label { .. } | Self::Text { .. } | Self::TestId { .. } => true,
        }
    }

    /// The same role query with the container qualifier dropped.
    ///
    /// `None` for non-role queries and role queries without `container_text`; this is how a
    /// `no_match` result reports that matching controls exist outside any qualifying container.
    pub fn without_container_text(&self) -> Option<Self> {
        match self {
            Self::Role {
                role,
                name,
                container_text: Some(_),
            } => Some(Self::Role {
                role: role.clone(),
                name: name.clone(),
                container_text: None,
            }),
            _ => None,
        }
    }

    /// The same query with every `exact` text matcher relaxed to `contains`.
    ///
    /// `None` when the query holds no exact matcher, so there is no relaxation to report and no
    /// second pass to run. Sites decorate accessible names routinely (`"Cargo.toml, (File)"`), so
    /// this is how a `no_match` result reports that an informed `contains` retry would land.
    pub fn relaxed_to_contains(&self) -> Option<Self> {
        match self {
            Self::Role {
                role,
                name,
                container_text,
            } => {
                let relaxed_name = name
                    .as_ref()
                    .and_then(SemanticTextMatch::relaxed_to_contains);
                let relaxed_container = container_text
                    .as_ref()
                    .and_then(SemanticTextMatch::relaxed_to_contains);
                if relaxed_name.is_none() && relaxed_container.is_none() {
                    return None;
                }
                Some(Self::Role {
                    role: role.clone(),
                    name: relaxed_name.or_else(|| name.clone()),
                    container_text: relaxed_container.or_else(|| container_text.clone()),
                })
            }
            Self::Label { text } => text.relaxed_to_contains().map(|text| Self::Label { text }),
            Self::Text { text } => text.relaxed_to_contains().map(|text| Self::Text { text }),
            // A test id is an exact identifier, not decorated prose; there is nothing to relax.
            Self::TestId { .. } => None,
        }
    }

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

/// How many nodes a specific relaxation of a `no_match` query would have reached.
///
/// The relaxation may be exact matchers relaxed to `contains`, or the container qualifier dropped.
/// The scan is capped at [`MAX_SEMANTIC_RELAXED_CANDIDATES`]; `saturated` marks the cap being
/// reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelaxedMatchCandidates {
    pub count: u16,
    pub saturated: bool,
}

impl RelaxedMatchCandidates {
    pub fn new(count: usize) -> Self {
        let limit = usize::from(MAX_SEMANTIC_RELAXED_CANDIDATES);
        Self {
            count: u16::try_from(count.min(limit)).unwrap_or(MAX_SEMANTIC_RELAXED_CANDIDATES),
            saturated: count >= limit,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QueryPageResult {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub outcome: SemanticQueryOutcome,
    pub matches: Vec<SemanticMatch>,
    pub omitted_match_count: u32,
    /// Present only on `no_match` when the query used an exact text matcher and a relaxed
    /// `contains` retry would have matched at least one node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relaxed_match_candidates: Option<RelaxedMatchCandidates>,
    /// Present only on `no_match` of a container-qualified role query when the same query with
    /// the container qualifier dropped would have matched at least one node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncontained_match_candidates: Option<RelaxedMatchCandidates>,
}

impl QueryPageResult {
    pub fn new(
        context: ObservationContext,
        generation: SnapshotGeneration,
        matches: Vec<SemanticMatch>,
        max_matches: u16,
    ) -> Result<Self> {
        Self::with_no_match_diagnostics(context, generation, matches, max_matches, None, None)
    }

    pub fn with_no_match_diagnostics(
        context: ObservationContext,
        generation: SnapshotGeneration,
        mut matches: Vec<SemanticMatch>,
        max_matches: u16,
        relaxed_match_candidates: Option<RelaxedMatchCandidates>,
        uncontained_match_candidates: Option<RelaxedMatchCandidates>,
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
        // Relaxed-candidate accounting exists to explain an empty result. Reporting it beside a
        // real match set would invite callers to treat it as a second, unranked match set.
        let relaxed_match_candidates = relaxed_match_candidates
            .filter(|_| outcome == SemanticQueryOutcome::NoMatch)
            .filter(|candidates| candidates.count > 0);
        let uncontained_match_candidates = uncontained_match_candidates
            .filter(|_| outcome == SemanticQueryOutcome::NoMatch)
            .filter(|candidates| candidates.count > 0);
        Ok(Self {
            context,
            generation,
            outcome,
            matches,
            omitted_match_count,
            relaxed_match_candidates,
            uncontained_match_candidates,
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
    warnings: Vec<KrometrailError>,
}
impl EncodedScreenshot {
    pub fn new(metadata: ScreenshotMetadata, bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(invalid("encoded screenshot payload must not be empty"));
        }
        Ok(Self {
            metadata,
            bytes,
            warnings: Vec::new(),
        })
    }
    pub fn metadata(&self) -> &ScreenshotMetadata {
        &self.metadata
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn warnings(&self) -> &[KrometrailError] {
        &self.warnings
    }
    pub fn push_warning(&mut self, warning: KrometrailError) {
        self.warnings.push(warning);
    }
    pub fn with_warning(mut self, warning: KrometrailError) -> Self {
        self.push_warning(warning);
        self
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
page_request!(LiveObservationRequest);

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotPageAnchor {
    #[default]
    Document,
    Viewport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPageRequest {
    #[serde(default)]
    pub target: PageSelection,
    #[serde(default)]
    pub anchor: SnapshotPageAnchor,
    #[serde(default)]
    pub document: crate::browser::SemanticDocumentScope,
}

impl SnapshotPageRequest {
    pub const fn new(target_id: TargetId) -> Self {
        Self {
            target: PageSelection::Target(target_id),
            anchor: SnapshotPageAnchor::Document,
            document: crate::browser::SemanticDocumentScope::MainDocument,
        }
    }
}

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

impl LiveObservation {
    /// Attach a warning to an available screenshot. An unavailable screenshot
    /// already carries its authoritative observation failure.
    pub fn attach_screenshot_warning(&mut self, warning: KrometrailError) {
        if let ObservationPart::Available(screenshot) = &mut self.screenshot {
            screenshot.push_warning(warning);
        }
    }
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
        let css_error = CssSize::new(0.0, 1.0).unwrap_err();
        assert!(css_error.message.as_str().contains("0×1"));
        assert!(css_error.message.as_str().contains("finite and positive"));
        assert_eq!(css_error.retry, RetryAdvice::AfterRecovery);
        assert!(css_error.recovery.is_some());
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
    fn snapshot_page_anchor_defaults_to_document_and_accepts_viewport() {
        let document: SnapshotPageRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(document.anchor, SnapshotPageAnchor::Document);

        let viewport: SnapshotPageRequest =
            serde_json::from_str(r#"{"anchor":"viewport"}"#).unwrap();
        assert_eq!(viewport.anchor, SnapshotPageAnchor::Viewport);
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
                document_rect: None,
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
                document_rect: None,
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
    fn exact_match_ignores_format_characters_and_private_use_glyphs() {
        for candidate in [
            "Advanced filters\u{200b}",
            "Advanced filters\u{200d}",
            "Filters \u{e5cf}",
            "\u{e5cf} Filters",
        ] {
            let exact = SemanticTextMatch::new(
                if candidate.contains("Advanced") {
                    "Advanced filters"
                } else {
                    "Filters"
                },
                SemanticTextMatchMode::Exact,
                false,
            )
            .unwrap();
            assert!(exact.matches(candidate), "candidate {candidate:?}");
        }
        let icon_only =
            SemanticTextMatch::new("\u{e5cf}", SemanticTextMatchMode::Exact, false).unwrap();
        assert!(!icon_only.matches("\u{e5cf}"));

        assert_eq!(
            collapsed_semantic_text_bytes("  Filters\u{200b} \u{e5cf}\n now "),
            "Filters now".len()
        );
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
            name: Some(ref name),
            container_text: Some(ref container_text),
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
        assert!(request.query.requires_dom_semantics());

        let legacy = SemanticQuery::role("button", None).unwrap();
        assert!(!legacy.requires_dom_semantics());
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

        assert!(SemanticQuery::Label { text: name.clone() }.requires_dom_semantics());
        assert!(SemanticQuery::Text { text: name.clone() }.requires_dom_semantics());
        assert!(
            SemanticQuery::test_id("save")
                .unwrap()
                .requires_dom_semantics()
        );
    }

    /// A no-match result is only useful if it says what a relaxed retry would reach, and only on
    /// the empty result — beside real matches it would read as a second, unranked match set.
    #[test]
    fn no_match_diagnostics_are_reported_only_for_an_empty_result() {
        let generation = SnapshotGeneration::new(7).unwrap();
        let relaxed_candidates = RelaxedMatchCandidates::new(3);
        let uncontained_candidates = RelaxedMatchCandidates::new(2);
        let no_match = QueryPageResult::with_no_match_diagnostics(
            context(),
            generation,
            vec![],
            2,
            Some(relaxed_candidates),
            Some(uncontained_candidates),
        )
        .unwrap();
        assert_eq!(no_match.outcome, SemanticQueryOutcome::NoMatch);
        assert_eq!(no_match.relaxed_match_candidates, Some(relaxed_candidates));
        assert_eq!(
            no_match.uncontained_match_candidates,
            Some(uncontained_candidates)
        );
        let round_trip =
            serde_json::from_value::<QueryPageResult>(serde_json::to_value(&no_match).unwrap())
                .unwrap();
        assert_eq!(round_trip, no_match);

        let matched = QueryPageResult::with_no_match_diagnostics(
            context(),
            generation,
            vec![SemanticMatch {
                reference: NodeReference {
                    target_id: target(),
                    generation,
                    node_id: SnapshotNodeId::new(1).unwrap(),
                },
                role: "link".into(),
                name: Some("Cargo.toml, (File)".into()),
            }],
            2,
            Some(relaxed_candidates),
            Some(uncontained_candidates),
        )
        .unwrap();
        assert!(matched.relaxed_match_candidates.is_none());
        assert!(matched.uncontained_match_candidates.is_none());

        // A relaxation that would also match nothing is silence, not a zero.
        let empty = QueryPageResult::with_no_match_diagnostics(
            context(),
            generation,
            vec![],
            2,
            Some(RelaxedMatchCandidates::new(0)),
            Some(RelaxedMatchCandidates::new(0)),
        )
        .unwrap();
        assert!(empty.relaxed_match_candidates.is_none());
        assert!(empty.uncontained_match_candidates.is_none());
        assert!(
            !serde_json::to_value(&empty)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("uncontained_match_candidates")
        );
    }

    #[test]
    fn relaxed_candidate_counts_saturate_at_the_declared_bound() {
        let under = RelaxedMatchCandidates::new(4);
        assert_eq!((under.count, under.saturated), (4, false));
        let at_cap =
            RelaxedMatchCandidates::new(usize::from(MAX_SEMANTIC_RELAXED_CANDIDATES) + 500);
        assert_eq!(at_cap.count, MAX_SEMANTIC_RELAXED_CANDIDATES);
        assert!(at_cap.saturated);
    }

    /// Only exact matchers relax; a query with nothing exact has no relaxation to report, and a
    /// test id is an identifier rather than decorated prose.
    #[test]
    fn query_relaxation_targets_exact_text_matchers_only() {
        let exact =
            SemanticTextMatch::new("Cargo.toml", SemanticTextMatchMode::Exact, false).unwrap();
        let contains =
            SemanticTextMatch::new("Cargo", SemanticTextMatchMode::Contains, false).unwrap();

        let relaxed = SemanticQuery::role("link", Some(exact.clone()))
            .unwrap()
            .relaxed_to_contains()
            .expect("an exact name matcher relaxes");
        let SemanticQuery::Role { name, .. } = &relaxed else {
            panic!("relaxation preserves the query kind");
        };
        let name = name.as_ref().expect("relaxed name is retained");
        assert_eq!(name.mode, SemanticTextMatchMode::Contains);
        assert!(name.matches("Cargo.toml, (File)"));

        assert!(
            SemanticQuery::role("link", Some(contains))
                .unwrap()
                .relaxed_to_contains()
                .is_none()
        );
        assert!(
            SemanticQuery::role("link", None)
                .unwrap()
                .relaxed_to_contains()
                .is_none()
        );
        assert!(
            SemanticQuery::test_id("save")
                .unwrap()
                .relaxed_to_contains()
                .is_none()
        );
        assert!(
            SemanticQuery::Label { text: exact }
                .relaxed_to_contains()
                .is_some()
        );

        let containerized = SemanticQuery::role_in_container(
            "checkbox",
            Some(SemanticTextMatch::new("Buy", SemanticTextMatchMode::Exact, false).unwrap()),
            SemanticTextMatch::new("Milk", SemanticTextMatchMode::Exact, false).unwrap(),
        )
        .unwrap();
        let stripped = containerized
            .without_container_text()
            .expect("container qualifier is removable");
        assert!(matches!(
            stripped,
            SemanticQuery::Role {
                container_text: None,
                ..
            }
        ));
        assert!(
            SemanticQuery::role("checkbox", None)
                .unwrap()
                .without_container_text()
                .is_none()
        );
        assert!(
            SemanticQuery::Text {
                text: SemanticTextMatch::new("Milk", SemanticTextMatchMode::Exact, false).unwrap(),
            }
            .without_container_text()
            .is_none()
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

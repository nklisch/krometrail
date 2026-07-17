use std::{
    num::NonZeroU32,
    path::{Component, Path},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    InteractionId, NonEmptyText, Result, SessionTime,
    error::invalid,
    validation::{delegate_json_schema, deserialize_validated},
};

use super::{
    BrowserOperationKind, CoordinateSpace, CssPoint, ElementLocator, LiveObservation,
    NodeReference, ObservationContext, PageSelection,
};

const MAX_SANITIZED_PARAMETERS_BYTES: usize = 4_096;
const MAX_FILES_PER_UPLOAD: usize = 8;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_PATH_COMPONENTS: usize = 32;
const MAX_KEY_CHORDS: usize = 32;
const MAX_CLICK_COUNT: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Pointer,
    Keyboard,
    Form,
    Scroll,
    DragDrop,
    FileDialog,
    Dialog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionabilityRequirement {
    Actionable,
    VisibleGeometry,
    Editable,
    Selectable,
    FileInput,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptedLocator {
    Element,
    ElementOrCoordinate,
    OptionalElement,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    InputAcknowledged,
    Settled,
    NavigationAware,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionDefinition {
    pub category: ActionCategory,
    pub actionability: ActionabilityRequirement,
    pub locator: AcceptedLocator,
    pub completion: CompletionKind,
    pub display_name: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum InteractionLocator {
    Element(ElementLocator),
    Coordinate {
        point: CssPoint,
        space: CoordinateSpace,
    },
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum InteractionLocatorWire {
    Element(ElementLocator),
    Coordinate {
        point: CssPoint,
        space: CoordinateSpace,
    },
}

impl InteractionLocator {
    pub fn element(locator: ElementLocator) -> Self {
        Self::Element(locator)
    }
    pub fn coordinate(point: CssPoint, space: CoordinateSpace) -> Result<Self> {
        Ok(Self::Coordinate {
            point: CssPoint::new(point.x, point.y)?,
            space,
        })
    }
    pub fn page_selection(&self) -> PageSelection {
        match self {
            Self::Element(ElementLocator::Reference(reference)) => {
                PageSelection::Target(reference.target_id)
            }
            Self::Element(ElementLocator::CssSelector(_)) | Self::Coordinate { .. } => {
                PageSelection::Selected
            }
        }
    }
}

delegate_json_schema!(InteractionLocator => InteractionLocatorWire);

impl<'de> Deserialize<'de> for InteractionLocator {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: InteractionLocatorWire| match wire {
            InteractionLocatorWire::Element(locator) => Ok(Self::Element(locator)),
            InteractionLocatorWire::Coordinate { point, space } => Self::coordinate(point, space),
        })
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Middle,
    Right,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct Modifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Alt,
    Control,
    Shift,
    Meta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl NamedKey {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::Tab => "Tab",
            Self::Escape => "Escape",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
            Self::Space => "Space",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|key| key.as_str().eq_ignore_ascii_case(value))
    }
    pub const ALL: &'static [Self] = &[
        Self::Enter,
        Self::Tab,
        Self::Escape,
        Self::Backspace,
        Self::Delete,
        Self::Space,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::Home,
        Self::End,
        Self::PageUp,
        Self::PageDown,
        Self::F1,
        Self::F2,
        Self::F3,
        Self::F4,
        Self::F5,
        Self::F6,
        Self::F7,
        Self::F8,
        Self::F9,
        Self::F10,
        Self::F11,
        Self::F12,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum KeySegment {
    Modifier(Modifier),
    NamedKey(NamedKey),
    Char(char),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct KeyChord(String);

impl KeyChord {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let segments = parse_chord(&value.into())?;
        Ok(Self(canonical_chord(&segments)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn segments(&self) -> Vec<KeySegment> {
        parse_chord(&self.0).expect("validated key chord")
    }
}
impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |value: String| Self::new(value))
    }
}

fn parse_chord(value: &str) -> Result<Vec<KeySegment>> {
    if value.trim().is_empty() {
        return Err(invalid("key chord must not be empty"));
    }
    let segments = value
        .split('+')
        .map(|raw| {
            let token = raw.trim();
            if token.is_empty() {
                return Err(invalid("key chord contains an empty segment"));
            }
            let modifier = match token.to_ascii_lowercase().as_str() {
                "alt" => Some(Modifier::Alt),
                "control" | "ctrl" => Some(Modifier::Control),
                "shift" => Some(Modifier::Shift),
                "meta" | "cmd" | "command" => Some(Modifier::Meta),
                _ => None,
            };
            if let Some(value) = modifier {
                return Ok(KeySegment::Modifier(value));
            }
            if let Some(value) = NamedKey::parse(token) {
                return Ok(KeySegment::NamedKey(value));
            }
            let mut chars = token.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Ok(KeySegment::Char(ch)),
                _ => Err(invalid("key chord contains an unsupported key name")),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut modifiers = Vec::new();
    let mut action_keys = 0;
    for segment in &segments {
        match segment {
            KeySegment::Modifier(modifier) => {
                if modifiers.contains(modifier) {
                    return Err(invalid("key chord contains a duplicate modifier"));
                }
                modifiers.push(*modifier);
            }
            KeySegment::NamedKey(_) | KeySegment::Char(_) => action_keys += 1,
        }
    }
    if action_keys != 1 {
        return Err(invalid("key chord must contain exactly one action key"));
    }
    Ok(segments)
}

fn canonical_chord(segments: &[KeySegment]) -> String {
    let mut values = Vec::with_capacity(segments.len());
    for modifier in [
        Modifier::Alt,
        Modifier::Control,
        Modifier::Meta,
        Modifier::Shift,
    ] {
        if segments.contains(&KeySegment::Modifier(modifier)) {
            values.push(match modifier {
                Modifier::Alt => "Alt".to_owned(),
                Modifier::Control => "Control".to_owned(),
                Modifier::Meta => "Meta".to_owned(),
                Modifier::Shift => "Shift".to_owned(),
            });
        }
    }
    let action = segments
        .iter()
        .find(|segment| !matches!(segment, KeySegment::Modifier(_)))
        .expect("validated chord has one action key");
    values.push(match action {
        KeySegment::NamedKey(key) => key.as_str().to_owned(),
        KeySegment::Char(ch) if ch.is_ascii_alphabetic() => ch.to_ascii_lowercase().to_string(),
        KeySegment::Char(ch) => ch.to_string(),
        KeySegment::Modifier(_) => unreachable!(),
    });
    values.join("+")
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FillMode {
    #[default]
    Replace,
    Append,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SelectValue {
    Value(Option<String>),
    Index(NonZeroU32),
    Label(NonEmptyText),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ScrollDelta {
    ByOffset { dx: f64, dy: f64 },
    ToElement(ElementLocator),
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum ScrollDeltaWire {
    ByOffset { dx: f64, dy: f64 },
    ToElement(ElementLocator),
}
impl ScrollDelta {
    pub fn by_offset(dx: f64, dy: f64) -> Result<Self> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err(invalid("scroll offsets must be finite"));
        }
        if dx == 0.0 && dy == 0.0 {
            return Err(invalid("scroll offset must change at least one axis"));
        }
        Ok(Self::ByOffset { dx, dy })
    }
}
delegate_json_schema!(ScrollDelta => ScrollDeltaWire);

impl<'de> Deserialize<'de> for ScrollDelta {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: ScrollDeltaWire| match wire {
            ScrollDeltaWire::ByOffset { dx, dy } => Self::by_offset(dx, dy),
            ScrollDeltaWire::ToElement(locator) => Ok(Self::ToElement(locator)),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DialogAction {
    Accept { prompt_text: Option<NonEmptyText> },
    Dismiss,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct ValidatedFilePath(String);
impl ValidatedFilePath {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        if path.len() > MAX_PATH_BYTES {
            return Err(invalid("file path exceeds the byte limit"));
        }
        let parsed = Path::new(&path);
        if !parsed.is_absolute() {
            return Err(invalid("file path must be absolute"));
        }
        let mut components = 0usize;
        for component in parsed.components() {
            match component {
                Component::ParentDir => {
                    return Err(invalid("file path must not contain parent traversal"));
                }
                Component::Normal(_) => components += 1,
                Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            }
        }
        if components == 0 || components > MAX_PATH_COMPONENTS {
            return Err(invalid("file path component count is invalid"));
        }
        Ok(Self(path))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn basename(&self) -> &str {
        Path::new(&self.0)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("file")
    }
}
impl<'de> Deserialize<'de> for ValidatedFilePath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |value: String| Self::new(value))
    }
}

macro_rules! request_wire {
    ($name:ident, $wire:ident, $ctor:expr) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(
                d: D,
            ) -> std::result::Result<Self, D::Error> {
                deserialize_validated(d, |wire: $wire| $ctor(wire))
            }
        }
    };
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClickRequest {
    pub target: PageSelection,
    pub locator: InteractionLocator,
    pub button: MouseButton,
    pub modifiers: Modifiers,
    pub click_count: u8,
    pub wait_for_navigation: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ClickRequestWire {
    #[serde(default)]
    target: PageSelection,
    locator: InteractionLocator,
    #[serde(default)]
    button: MouseButton,
    #[serde(default)]
    modifiers: Modifiers,
    #[serde(default = "default_click_count")]
    click_count: u8,
    #[serde(default)]
    wait_for_navigation: bool,
}
const fn default_click_count() -> u8 {
    1
}
impl ClickRequest {
    pub fn new(
        target: PageSelection,
        locator: InteractionLocator,
        button: MouseButton,
        modifiers: Modifiers,
        click_count: u8,
        wait_for_navigation: bool,
    ) -> Result<Self> {
        if !(1..=MAX_CLICK_COUNT).contains(&click_count) {
            return Err(invalid("click count must be between one and three"));
        }
        Ok(Self {
            target,
            locator,
            button,
            modifiers,
            click_count,
            wait_for_navigation,
        })
    }
}
delegate_json_schema!(ClickRequest => ClickRequestWire);
request_wire!(ClickRequest, ClickRequestWire, |w: ClickRequestWire| {
    Self::new(
        w.target,
        w.locator,
        w.button,
        w.modifiers,
        w.click_count,
        w.wait_for_navigation,
    )
});

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FillRequest {
    pub target: PageSelection,
    pub locator: InteractionLocator,
    pub value: NonEmptyText,
    pub mode: FillMode,
    pub wait_for_navigation: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct FillRequestWire {
    #[serde(default)]
    target: PageSelection,
    locator: InteractionLocator,
    value: NonEmptyText,
    #[serde(default)]
    mode: FillMode,
    #[serde(default)]
    wait_for_navigation: bool,
}
impl FillRequest {
    pub fn new(
        target: PageSelection,
        locator: InteractionLocator,
        value: impl Into<String>,
        mode: FillMode,
        wait_for_navigation: bool,
    ) -> Result<Self> {
        require_element(&locator, "fill")?;
        Ok(Self {
            target,
            locator,
            value: NonEmptyText::new(value.into())
                .map_err(|_| invalid("fill value must not be empty"))?,
            mode,
            wait_for_navigation,
        })
    }
}
delegate_json_schema!(FillRequest => FillRequestWire);
request_wire!(
    FillRequest,
    FillRequestWire,
    |w: FillRequestWire| Self::new(
        w.target,
        w.locator,
        w.value.as_str(),
        w.mode,
        w.wait_for_navigation
    )
);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PressKeysRequest {
    pub target: PageSelection,
    pub locator: Option<InteractionLocator>,
    pub keys: Vec<KeyChord>,
    pub wait_for_navigation: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct PressKeysRequestWire {
    #[serde(default)]
    target: PageSelection,
    #[serde(default)]
    locator: Option<InteractionLocator>,
    keys: Vec<KeyChord>,
    #[serde(default)]
    wait_for_navigation: bool,
}
impl PressKeysRequest {
    pub fn new(
        target: PageSelection,
        locator: Option<InteractionLocator>,
        keys: Vec<KeyChord>,
        wait_for_navigation: bool,
    ) -> Result<Self> {
        if keys.is_empty() || keys.len() > MAX_KEY_CHORDS {
            return Err(invalid(
                "key sequence must contain between one and 32 chords",
            ));
        }
        if locator
            .as_ref()
            .is_some_and(|l| !matches!(l, InteractionLocator::Element(_)))
        {
            return Err(invalid("key focus locator must target an element"));
        }
        Ok(Self {
            target,
            locator,
            keys,
            wait_for_navigation,
        })
    }
}
delegate_json_schema!(PressKeysRequest => PressKeysRequestWire);
request_wire!(
    PressKeysRequest,
    PressKeysRequestWire,
    |w: PressKeysRequestWire| Self::new(w.target, w.locator, w.keys, w.wait_for_navigation)
);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SelectOptionRequest {
    pub target: PageSelection,
    pub locator: InteractionLocator,
    pub value: SelectValue,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct SelectOptionRequestWire {
    #[serde(default)]
    target: PageSelection,
    locator: InteractionLocator,
    value: SelectValue,
}
impl SelectOptionRequest {
    pub fn new(
        target: PageSelection,
        locator: InteractionLocator,
        value: SelectValue,
    ) -> Result<Self> {
        require_element(&locator, "select option")?;
        Ok(Self {
            target,
            locator,
            value,
        })
    }
}
delegate_json_schema!(SelectOptionRequest => SelectOptionRequestWire);
request_wire!(
    SelectOptionRequest,
    SelectOptionRequestWire,
    |w: SelectOptionRequestWire| Self::new(w.target, w.locator, w.value)
);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HoverRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub locator: InteractionLocator,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DragRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub source: InteractionLocator,
    pub destination: InteractionLocator,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScrollRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub delta: ScrollDelta,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UploadFilesRequest {
    pub target: PageSelection,
    pub locator: InteractionLocator,
    pub files: Vec<ValidatedFilePath>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct UploadFilesRequestWire {
    #[serde(default)]
    target: PageSelection,
    locator: InteractionLocator,
    files: Vec<ValidatedFilePath>,
}
impl UploadFilesRequest {
    pub fn new(
        target: PageSelection,
        locator: InteractionLocator,
        files: Vec<ValidatedFilePath>,
    ) -> Result<Self> {
        require_element(&locator, "file upload")?;
        if files.is_empty() || files.len() > MAX_FILES_PER_UPLOAD {
            return Err(invalid("upload must contain between one and eight files"));
        }
        Ok(Self {
            target,
            locator,
            files,
        })
    }
}
delegate_json_schema!(UploadFilesRequest => UploadFilesRequestWire);
request_wire!(
    UploadFilesRequest,
    UploadFilesRequestWire,
    |w: UploadFilesRequestWire| Self::new(w.target, w.locator, w.files)
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HandleDialogRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub action: DialogAction,
}

fn require_element(locator: &InteractionLocator, action: &str) -> Result<()> {
    if matches!(locator, InteractionLocator::Element(_)) {
        Ok(())
    } else {
        Err(invalid(format!("{action} requires an element locator")))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionOutcome {
    Dispatched,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SanitizedParameters(Value);
impl SanitizedParameters {
    pub fn new(value: Value) -> Result<Self> {
        if !value.is_object() {
            return Err(invalid("sanitized parameters must be a JSON object"));
        }
        if serde_json::to_vec(&value)
            .map_err(|_| invalid("sanitized parameters are invalid"))?
            .len()
            > MAX_SANITIZED_PARAMETERS_BYTES
        {
            return Err(invalid("sanitized parameters exceed the byte limit"));
        }
        Ok(Self(value))
    }
    pub fn as_json(&self) -> &Value {
        &self.0
    }
}
impl<'de> Deserialize<'de> for SanitizedParameters {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |value: Value| Self::new(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocatorKind {
    Reference,
    Selector,
    CoordinateViewport,
    CoordinateDocument,
    TargetWide,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocatorSummary {
    pub kind: LocatorKind,
    pub reference: Option<NodeReference>,
    pub selector_length: Option<u32>,
    pub coordinate: Option<CssPoint>,
}
impl LocatorSummary {
    pub fn from_locator(locator: Option<&InteractionLocator>) -> Self {
        match locator {
            None => Self {
                kind: LocatorKind::TargetWide,
                reference: None,
                selector_length: None,
                coordinate: None,
            },
            Some(InteractionLocator::Element(ElementLocator::Reference(reference))) => Self {
                kind: LocatorKind::Reference,
                reference: Some(*reference),
                selector_length: None,
                coordinate: None,
            },
            Some(InteractionLocator::Element(ElementLocator::CssSelector(selector))) => Self {
                kind: LocatorKind::Selector,
                reference: None,
                selector_length: Some(
                    u32::try_from(selector.as_str().chars().count()).unwrap_or(u32::MAX),
                ),
                coordinate: None,
            },
            Some(InteractionLocator::Coordinate { point, space }) => Self {
                kind: match space {
                    CoordinateSpace::ViewportCss => LocatorKind::CoordinateViewport,
                    CoordinateSpace::DocumentCss => LocatorKind::CoordinateDocument,
                },
                reference: None,
                selector_length: None,
                coordinate: Some(*point),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InteractionRecord {
    pub id: InteractionId,
    pub context: ObservationContext,
    pub dispatch_time: SessionTime,
    pub live_observation_time: SessionTime,
    pub action: BrowserOperationKind,
    pub sanitized_parameters: SanitizedParameters,
    pub locator: LocatorSummary,
    pub outcome: InteractionOutcome,
    pub parent_batch: Option<InteractionId>,
}
#[derive(Deserialize)]
struct InteractionRecordWire {
    id: InteractionId,
    context: ObservationContext,
    dispatch_time: SessionTime,
    live_observation_time: SessionTime,
    action: BrowserOperationKind,
    sanitized_parameters: SanitizedParameters,
    locator: LocatorSummary,
    outcome: InteractionOutcome,
    parent_batch: Option<InteractionId>,
}
impl InteractionRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: InteractionId,
        context: ObservationContext,
        dispatch_time: SessionTime,
        live_observation_time: SessionTime,
        action: BrowserOperationKind,
        sanitized_parameters: SanitizedParameters,
        locator: LocatorSummary,
        outcome: InteractionOutcome,
        parent_batch: Option<InteractionId>,
    ) -> Result<Self> {
        if context.started_at > dispatch_time
            || dispatch_time > live_observation_time
            || live_observation_time > context.completed_at
        {
            return Err(invalid(
                "interaction record times must be monotonically ordered",
            ));
        }
        if !action.is_interaction() {
            return Err(invalid(
                "interaction record action must be an interaction operation",
            ));
        }
        Ok(Self {
            id,
            context,
            dispatch_time,
            live_observation_time,
            action,
            sanitized_parameters,
            locator,
            outcome,
            parent_batch,
        })
    }

    pub fn anchor(&self) -> Result<super::InteractionAnchor> {
        super::InteractionAnchor::new(
            self.id,
            self.context.session_id,
            self.context.target_id,
            self.action,
            super::InteractionTiming::new(
                self.context.started_at,
                self.dispatch_time,
                self.live_observation_time,
                Some(self.live_observation_time),
            )?,
        )
    }
}
request_wire!(
    InteractionRecord,
    InteractionRecordWire,
    |w: InteractionRecordWire| Self::new(
        w.id,
        w.context,
        w.dispatch_time,
        w.live_observation_time,
        w.action,
        w.sanitized_parameters,
        w.locator,
        w.outcome,
        w.parent_batch
    )
);

#[derive(Clone, Debug, PartialEq)]
pub struct InteractionResult {
    pub record: InteractionRecord,
    pub observation: LiveObservation,
}

impl InteractionResult {
    pub fn anchor(&self) -> Result<super::InteractionAnchor> {
        self.record.anchor()
    }
}

pub trait BrowserActionRequest {
    fn locator(&self) -> Option<&InteractionLocator>;
    fn sanitize(&self) -> SanitizedParameters;
}

fn safe_parameters(value: Value) -> SanitizedParameters {
    SanitizedParameters::new(value).expect("bounded action sanitization")
}
fn locator_json(locator: Option<&InteractionLocator>) -> Value {
    serde_json::to_value(LocatorSummary::from_locator(locator)).expect("locator summary serializes")
}
fn modifiers_json(value: Modifiers) -> Value {
    json!({"alt":value.alt,"control":value.control,"shift":value.shift,"meta":value.meta})
}

impl BrowserActionRequest for ClickRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        Some(&self.locator)
    }
    fn sanitize(&self) -> SanitizedParameters {
        safe_parameters(
            json!({"button":self.button,"modifiers":modifiers_json(self.modifiers),"click_count":self.click_count,"wait_for_navigation":self.wait_for_navigation,"locator":locator_json(self.locator())}),
        )
    }
}
impl BrowserActionRequest for FillRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        Some(&self.locator)
    }
    fn sanitize(&self) -> SanitizedParameters {
        safe_parameters(
            json!({"mode":self.mode,"value_length":self.value.as_str().chars().count(),"wait_for_navigation":self.wait_for_navigation,"locator":locator_json(self.locator())}),
        )
    }
}
impl BrowserActionRequest for PressKeysRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        self.locator.as_ref()
    }
    fn sanitize(&self) -> SanitizedParameters {
        safe_parameters(
            json!({"keys":self.keys.iter().map(KeyChord::as_str).collect::<Vec<_>>(),"wait_for_navigation":self.wait_for_navigation,"locator":locator_json(self.locator())}),
        )
    }
}
impl BrowserActionRequest for SelectOptionRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        Some(&self.locator)
    }
    fn sanitize(&self) -> SanitizedParameters {
        let value = match &self.value {
            SelectValue::Value(v) => {
                json!({"kind":"value","length":v.as_ref().map_or(0, |v| v.chars().count())})
            }
            SelectValue::Index(v) => json!({"kind":"index","index":v}),
            SelectValue::Label(v) => json!({"kind":"label","length":v.as_str().chars().count()}),
        };
        safe_parameters(json!({"value":value,"locator":locator_json(self.locator())}))
    }
}
impl BrowserActionRequest for HoverRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        Some(&self.locator)
    }
    fn sanitize(&self) -> SanitizedParameters {
        safe_parameters(json!({"locator":locator_json(self.locator())}))
    }
}
impl BrowserActionRequest for DragRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        Some(&self.source)
    }
    fn sanitize(&self) -> SanitizedParameters {
        safe_parameters(
            json!({"source":locator_json(Some(&self.source)),"destination":locator_json(Some(&self.destination))}),
        )
    }
}
impl BrowserActionRequest for ScrollRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        None
    }
    fn sanitize(&self) -> SanitizedParameters {
        match &self.delta {
            ScrollDelta::ByOffset { dx, dy } => {
                safe_parameters(json!({"delta":{"kind":"by_offset","dx":dx,"dy":dy}}))
            }
            ScrollDelta::ToElement(locator) => safe_parameters(
                json!({"delta":{"kind":"to_element","locator":serde_json::to_value(LocatorSummary::from_locator(Some(&InteractionLocator::Element(locator.clone())))).unwrap()}}),
            ),
        }
    }
}
impl BrowserActionRequest for UploadFilesRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        Some(&self.locator)
    }
    fn sanitize(&self) -> SanitizedParameters {
        safe_parameters(
            json!({"files":self.files.iter().map(ValidatedFilePath::basename).collect::<Vec<_>>(),"count":self.files.len(),"locator":locator_json(self.locator())}),
        )
    }
}
impl BrowserActionRequest for HandleDialogRequest {
    fn locator(&self) -> Option<&InteractionLocator> {
        None
    }
    fn sanitize(&self) -> SanitizedParameters {
        let (action, len) = match &self.action {
            DialogAction::Accept { prompt_text } => (
                "accept",
                prompt_text
                    .as_ref()
                    .map_or(0, |v| v.as_str().chars().count()),
            ),
            DialogAction::Dismiss => ("dismiss", 0),
        };
        safe_parameters(json!({"action":action,"prompt_text_length":len}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SnapshotGeneration, SnapshotNodeId, TargetId};
    use uuid::Uuid;
    fn target() -> crate::TargetId {
        TargetId::from_uuid(Uuid::from_u128(1))
    }
    fn reference() -> NodeReference {
        NodeReference {
            target_id: target(),
            generation: SnapshotGeneration::new(1).unwrap(),
            node_id: SnapshotNodeId::new(1).unwrap(),
        }
    }
    #[test]
    fn key_chords_are_closed_and_round_trip() {
        for value in ["Enter", "Control+S", "Shift+ArrowDown", "é"] {
            let chord = KeyChord::new(value).unwrap();
            assert_eq!(
                serde_json::from_str::<KeyChord>(&serde_json::to_string(&chord).unwrap()).unwrap(),
                chord
            );
        }
        assert!(KeyChord::new("Control+MediaPlay").is_err());
        assert!(KeyChord::new("Control+").is_err());
        assert!(KeyChord::new("Control+Control+s").is_err());
        assert!(KeyChord::new("Control+s+x").is_err());
        assert_eq!(KeyChord::new("META+A").unwrap().as_str(), "Meta+a");
        assert_eq!(KeyChord::new("cmd+a").unwrap().as_str(), "Meta+a");
        assert_eq!(KeyChord::new("Meta+a").unwrap().as_str(), "Meta+a");
    }

    #[test]
    fn interaction_wire_defaults_select_the_current_page() {
        let click: ClickRequest = serde_json::from_value(json!({
            "locator": {"kind":"coordinate","value":{"point":{"x":1.0,"y":2.0},"space":"viewport_css"}}
        })).unwrap();
        assert_eq!(click.target, PageSelection::Selected);
        assert_eq!(click.button, MouseButton::Left);
        assert_eq!(click.modifiers, Modifiers::default());
        assert_eq!(click.click_count, 1);
        assert!(!click.wait_for_navigation);

        let fill: FillRequest = serde_json::from_value(json!({
            "locator":{"kind":"element","value":{"kind":"reference","value":reference()}},
            "value":"x"
        }))
        .unwrap();
        assert_eq!(fill.target, PageSelection::Selected);
        assert_eq!(fill.mode, FillMode::Replace);
        assert!(!fill.wait_for_navigation);
    }
    #[test]
    fn request_validation_and_sanitization_protect_boundaries() {
        let locator = InteractionLocator::Element(ElementLocator::Reference(reference()));
        assert!(
            ClickRequest::new(
                PageSelection::Target(target()),
                locator.clone(),
                MouseButton::Left,
                Modifiers::default(),
                0,
                false
            )
            .is_err()
        );
        assert!(
            FillRequest::new(
                PageSelection::Target(target()),
                InteractionLocator::coordinate(
                    CssPoint::new(1.0, 2.0).unwrap(),
                    CoordinateSpace::ViewportCss
                )
                .unwrap(),
                "x",
                FillMode::Replace,
                false
            )
            .is_err()
        );
        assert!(ValidatedFilePath::new("relative/file").is_err());
        for secret in ["p@ssword", "tok_live_abc123", "482901"] {
            let fill = FillRequest::new(
                PageSelection::Target(target()),
                locator.clone(),
                secret,
                FillMode::Replace,
                false,
            )
            .unwrap();
            let sanitized = fill.sanitize();
            let encoded = serde_json::to_string(sanitized.as_json()).unwrap();
            assert!(!encoded.contains(secret));
            assert!(sanitized.as_json().get("value_preview").is_none());
            assert_eq!(
                sanitized.as_json()["value_length"],
                json!(secret.chars().count())
            );
        }
    }
    #[test]
    fn upload_sanitization_keeps_only_basenames() {
        let request = UploadFilesRequest::new(
            PageSelection::Target(target()),
            InteractionLocator::Element(ElementLocator::Reference(reference())),
            vec![ValidatedFilePath::new("/private/secret/upload.txt").unwrap()],
        )
        .unwrap();
        let encoded = serde_json::to_string(request.sanitize().as_json()).unwrap();
        assert!(encoded.contains("upload.txt"));
        assert!(!encoded.contains("private"));
        assert!(!encoded.contains("secret"));
    }
}

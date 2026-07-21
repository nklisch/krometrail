//! Browser-event semantic registry and validated privacy boundary.

use std::{
    collections::{HashMap, HashSet},
    num::{NonZeroU64, NonZeroUsize},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    error::{Result, invalid},
    ids::{BrowserEventId, NetworkRequestId, SessionId, TargetId},
    lifecycle::TargetLifecycle,
    recording::{CaptureStreamState, TargetCaptureStatus},
    time::{ObservedTime, SessionRange, SessionTime, SourceTime},
    validation::deserialize_validated,
};

use super::{EventRedactor, RedactedText, SanitizedUrl, TargetVisibility};

pub const MAX_BROWSER_EVENT_PAYLOAD_BYTES: usize = 8 * 1_024;
pub const MAX_BROWSER_EVENT_BATCH_ROWS: usize = 128;
pub const MAX_BROWSER_EVENT_BATCH_BYTES: usize = 256 * 1_024;
pub const MAX_CONSOLE_ARGUMENT_TYPES: usize = 16;
pub const MAX_EVENT_STACK_FRAMES: usize = 16;
pub const MAX_NETWORK_INITIATOR_STACK_FRAMES: usize = 8;

#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEventSeverity {
    Debug,
    Info,
    Warning,
    Error,
}

impl BrowserEventSeverity {
    pub const ALL: &'static [Self] = &[Self::Debug, Self::Info, Self::Warning, Self::Error];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEventClass {
    Console,
    Exception,
    Network,
    Navigation,
    Lifecycle,
    Target,
    Dialog,
    Capture,
    Operational,
}

impl BrowserEventClass {
    pub const ALL: &'static [Self] = &[
        Self::Console,
        Self::Exception,
        Self::Network,
        Self::Navigation,
        Self::Lifecycle,
        Self::Target,
        Self::Dialog,
        Self::Capture,
        Self::Operational,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserSourceClock {
    CdpMonotonic,
    UnixEpoch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserSourceTimestamp {
    clock: BrowserSourceClock,
    time: SourceTime,
    rounded: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSourceTimestampWire {
    clock: BrowserSourceClock,
    time: SourceTime,
    rounded: bool,
}

impl BrowserSourceTimestamp {
    pub fn new(clock: BrowserSourceClock, time: SourceTime, rounded: bool) -> Result<Self> {
        if time.as_nanos() < 0 {
            return Err(invalid("browser source timestamp must not be negative"));
        }
        Ok(Self {
            clock,
            time,
            rounded,
        })
    }

    pub const fn clock(&self) -> BrowserSourceClock {
        self.clock
    }

    pub const fn time(&self) -> SourceTime {
        self.time
    }

    pub const fn rounded(&self) -> bool {
        self.rounded
    }
}

impl<'de> Deserialize<'de> for BrowserSourceTimestamp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserSourceTimestampWire| {
            Self::new(wire.clock, wire.time, wire.rounded)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct BrowserEventOrdinal(NonZeroU64);

impl BrowserEventOrdinal {
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| invalid("browser event ordinal must be non-zero"))
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for BrowserEventOrdinal {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: u64| Self::new(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleEventSource {
    Runtime,
    Log,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl ConsoleLevel {
    const fn severity(self) -> BrowserEventSeverity {
        match self {
            Self::Debug => BrowserEventSeverity::Debug,
            Self::Info => BrowserEventSeverity::Info,
            Self::Warning => BrowserEventSeverity::Warning,
            Self::Error => BrowserEventSeverity::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleMethod {
    Log,
    Debug,
    Info,
    Error,
    Warning,
    Dir,
    DirXml,
    Table,
    Trace,
    Clear,
    StartGroup,
    StartGroupCollapsed,
    EndGroup,
    Assert,
    Profile,
    ProfileEnd,
    Count,
    TimeEnd,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleArgumentType {
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    BigInt,
    Symbol,
    Function,
    Object,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SanitizedStackFrame {
    function_name: Option<RedactedText>,
    url: Option<SanitizedUrl>,
    line_number: Option<u32>,
    column_number: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SanitizedStackFrameWire {
    function_name: Option<RedactedText>,
    url: Option<SanitizedUrl>,
    line_number: Option<u32>,
    column_number: Option<u32>,
}

impl SanitizedStackFrame {
    pub fn new(
        function_name: Option<RedactedText>,
        url: Option<SanitizedUrl>,
        line_number: Option<u32>,
        column_number: Option<u32>,
    ) -> Result<Self> {
        if function_name
            .as_ref()
            .is_some_and(|name| name.byte_len() > super::MAX_REDACTED_FUNCTION_BYTES)
        {
            return Err(invalid("stack function name exceeds its byte limit"));
        }
        Ok(Self {
            function_name,
            url,
            line_number,
            column_number,
        })
    }

    pub fn sanitize(
        function_name: Option<&str>,
        url: Option<&str>,
        line_number: Option<u32>,
        column_number: Option<u32>,
    ) -> Result<Self> {
        Self::new(
            function_name.map(|value| EventRedactor.function_name(value)),
            url.map(SanitizedUrl::sanitize).transpose()?,
            line_number,
            column_number,
        )
    }

    pub fn function_name(&self) -> Option<&RedactedText> {
        self.function_name.as_ref()
    }

    pub fn url(&self) -> Option<&SanitizedUrl> {
        self.url.as_ref()
    }

    pub const fn line_number(&self) -> Option<u32> {
        self.line_number
    }

    pub const fn column_number(&self) -> Option<u32> {
        self.column_number
    }
}

impl<'de> Deserialize<'de> for SanitizedStackFrame {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: SanitizedStackFrameWire| {
            Self::new(
                wire.function_name,
                wire.url,
                wire.line_number,
                wire.column_number,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConsoleEvent {
    source: ConsoleEventSource,
    level: ConsoleLevel,
    method: ConsoleMethod,
    argument_types: Vec<ConsoleArgumentType>,
    preview: Option<RedactedText>,
    stack: Vec<SanitizedStackFrame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsoleEventWire {
    source: ConsoleEventSource,
    level: ConsoleLevel,
    method: ConsoleMethod,
    argument_types: Vec<ConsoleArgumentType>,
    preview: Option<RedactedText>,
    stack: Vec<SanitizedStackFrame>,
}

impl ConsoleEvent {
    pub fn new(
        source: ConsoleEventSource,
        level: ConsoleLevel,
        method: ConsoleMethod,
        mut argument_types: Vec<ConsoleArgumentType>,
        preview: Option<RedactedText>,
        mut stack: Vec<SanitizedStackFrame>,
    ) -> Self {
        argument_types.truncate(MAX_CONSOLE_ARGUMENT_TYPES);
        stack.truncate(MAX_EVENT_STACK_FRAMES);
        Self {
            source,
            level,
            method,
            argument_types,
            preview,
            stack,
        }
    }

    pub const fn source(&self) -> ConsoleEventSource {
        self.source
    }

    pub const fn level(&self) -> ConsoleLevel {
        self.level
    }

    pub const fn method(&self) -> ConsoleMethod {
        self.method
    }

    pub fn argument_types(&self) -> &[ConsoleArgumentType] {
        &self.argument_types
    }

    pub fn preview(&self) -> Option<&RedactedText> {
        self.preview.as_ref()
    }

    pub fn stack(&self) -> &[SanitizedStackFrame] {
        &self.stack
    }
}

impl<'de> Deserialize<'de> for ConsoleEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: ConsoleEventWire| {
            let event = Self {
                source: wire.source,
                level: wire.level,
                method: wire.method,
                argument_types: wire.argument_types,
                preview: wire.preview,
                stack: wire.stack,
            };
            event.validate_contract()?;
            Ok(event)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExceptionEvent {
    name: Option<RedactedText>,
    text: RedactedText,
    stack: Vec<SanitizedStackFrame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExceptionEventWire {
    name: Option<RedactedText>,
    text: RedactedText,
    stack: Vec<SanitizedStackFrame>,
}

impl ExceptionEvent {
    pub fn new(
        name: Option<RedactedText>,
        text: RedactedText,
        mut stack: Vec<SanitizedStackFrame>,
    ) -> Result<Self> {
        if name
            .as_ref()
            .is_some_and(|name| name.byte_len() > super::MAX_REDACTED_NAME_BYTES)
        {
            return Err(invalid("exception name exceeds its byte limit"));
        }
        stack.truncate(MAX_EVENT_STACK_FRAMES);
        Ok(Self { name, text, stack })
    }

    pub fn name(&self) -> Option<&RedactedText> {
        self.name.as_ref()
    }

    pub const fn text(&self) -> &RedactedText {
        &self.text
    }

    pub fn stack(&self) -> &[SanitizedStackFrame] {
        &self.stack
    }
}

impl<'de> Deserialize<'de> for ExceptionEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: ExceptionEventWire| {
            let event = Self {
                name: wire.name,
                text: wire.text,
                stack: wire.stack,
            };
            event.validate_contract()?;
            Ok(event)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "sha256",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Trace,
    Connect,
    Other([u8; 32]),
}

impl HttpMethod {
    /// Common methods remain semantic; uncommon method names retain hash-only
    /// equality without exposing extension names or application-specific values.
    pub fn sanitize(raw: &str) -> Result<Self> {
        let value = raw.trim();
        if value.is_empty() || value.len() > 64 || !value.bytes().all(is_http_token_byte) {
            return Err(invalid(
                "network method is malformed or exceeds its byte limit",
            ));
        }
        Ok(match value.to_ascii_uppercase().as_str() {
            "GET" => Self::Get,
            "HEAD" => Self::Head,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "PATCH" => Self::Patch,
            "DELETE" => Self::Delete,
            "OPTIONS" => Self::Options,
            "TRACE" => Self::Trace,
            "CONNECT" => Self::Connect,
            _ => Self::Other(Sha256::digest(value.as_bytes()).into()),
        })
    }
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkResourceType {
    Document,
    Stylesheet,
    Image,
    Media,
    Font,
    Script,
    TextTrack,
    Xhr,
    Fetch,
    Prefetch,
    EventSource,
    WebSocket,
    Manifest,
    SignedExchange,
    Ping,
    CspViolationReport,
    Preflight,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkInitiatorKind {
    Parser,
    Script,
    Preload,
    SignedExchange,
    Preflight,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetworkInitiator {
    kind: NetworkInitiatorKind,
    stack: Vec<SanitizedStackFrame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkInitiatorWire {
    kind: NetworkInitiatorKind,
    stack: Vec<SanitizedStackFrame>,
}

impl NetworkInitiator {
    pub fn new(kind: NetworkInitiatorKind, mut stack: Vec<SanitizedStackFrame>) -> Self {
        stack.truncate(MAX_NETWORK_INITIATOR_STACK_FRAMES);
        Self { kind, stack }
    }

    pub const fn kind(&self) -> NetworkInitiatorKind {
        self.kind
    }

    pub fn stack(&self) -> &[SanitizedStackFrame] {
        &self.stack
    }

    fn validate(&self) -> Result<()> {
        validate_stack(&self.stack, MAX_NETWORK_INITIATOR_STACK_FRAMES)
    }
}

impl<'de> Deserialize<'de> for NetworkInitiator {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: NetworkInitiatorWire| {
            let initiator = Self {
                kind: wire.kind,
                stack: wire.stack,
            };
            initiator.validate()?;
            Ok(initiator)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct HttpStatus(u16);

impl HttpStatus {
    pub fn new(value: u16) -> Result<Self> {
        if value <= 999 {
            Ok(Self(value))
        } else {
            Err(invalid("network response status must be between 0 and 999"))
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for HttpStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: u16| Self::new(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkFailureKind {
    Cancelled,
    Blocked,
    Dns,
    Connection,
    Timeout,
    Other,
}

fn validate_request_id(id: NetworkRequestId) -> Result<()> {
    if id.as_uuid().is_nil() {
        Err(invalid("network request ID must not be nil"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetworkRequestStarted {
    request_id: NetworkRequestId,
    method: HttpMethod,
    resource_type: NetworkResourceType,
    url: SanitizedUrl,
    initiator: NetworkInitiator,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkRequestStartedWire {
    request_id: NetworkRequestId,
    method: HttpMethod,
    resource_type: NetworkResourceType,
    url: SanitizedUrl,
    initiator: NetworkInitiator,
}

impl NetworkRequestStarted {
    pub fn new(
        request_id: NetworkRequestId,
        method: HttpMethod,
        resource_type: NetworkResourceType,
        url: SanitizedUrl,
        initiator: NetworkInitiator,
    ) -> Result<Self> {
        validate_request_id(request_id)?;
        initiator.validate()?;
        Ok(Self {
            request_id,
            method,
            resource_type,
            url,
            initiator,
        })
    }

    pub const fn request_id(&self) -> NetworkRequestId {
        self.request_id
    }

    pub const fn method(&self) -> &HttpMethod {
        &self.method
    }

    pub const fn resource_type(&self) -> NetworkResourceType {
        self.resource_type
    }

    pub const fn url(&self) -> &SanitizedUrl {
        &self.url
    }

    pub const fn initiator(&self) -> &NetworkInitiator {
        &self.initiator
    }
}

impl<'de> Deserialize<'de> for NetworkRequestStarted {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: NetworkRequestStartedWire| {
            Self::new(
                wire.request_id,
                wire.method,
                wire.resource_type,
                wire.url,
                wire.initiator,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetworkResponseReceived {
    request_id: NetworkRequestId,
    method: Option<HttpMethod>,
    resource_type: Option<NetworkResourceType>,
    url: Option<SanitizedUrl>,
    status: HttpStatus,
    from_disk_cache: bool,
    from_service_worker: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkResponseReceivedWire {
    request_id: NetworkRequestId,
    method: Option<HttpMethod>,
    resource_type: Option<NetworkResourceType>,
    url: Option<SanitizedUrl>,
    status: HttpStatus,
    from_disk_cache: bool,
    from_service_worker: bool,
}

impl NetworkResponseReceived {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: NetworkRequestId,
        method: Option<HttpMethod>,
        resource_type: Option<NetworkResourceType>,
        url: Option<SanitizedUrl>,
        status: HttpStatus,
        from_disk_cache: bool,
        from_service_worker: bool,
    ) -> Result<Self> {
        validate_request_id(request_id)?;
        Ok(Self {
            request_id,
            method,
            resource_type,
            url,
            status,
            from_disk_cache,
            from_service_worker,
        })
    }

    pub const fn request_id(&self) -> NetworkRequestId {
        self.request_id
    }

    pub const fn method(&self) -> Option<&HttpMethod> {
        self.method.as_ref()
    }

    pub const fn resource_type(&self) -> Option<NetworkResourceType> {
        self.resource_type
    }

    pub const fn url(&self) -> Option<&SanitizedUrl> {
        self.url.as_ref()
    }

    pub const fn status(&self) -> HttpStatus {
        self.status
    }

    pub const fn from_disk_cache(&self) -> bool {
        self.from_disk_cache
    }

    pub const fn from_service_worker(&self) -> bool {
        self.from_service_worker
    }
}

impl<'de> Deserialize<'de> for NetworkResponseReceived {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: NetworkResponseReceivedWire| {
            Self::new(
                wire.request_id,
                wire.method,
                wire.resource_type,
                wire.url,
                wire.status,
                wire.from_disk_cache,
                wire.from_service_worker,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetworkRequestFinished {
    request_id: NetworkRequestId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkRequestFinishedWire {
    request_id: NetworkRequestId,
}

impl NetworkRequestFinished {
    pub fn new(request_id: NetworkRequestId) -> Result<Self> {
        validate_request_id(request_id)?;
        Ok(Self { request_id })
    }

    pub const fn request_id(&self) -> NetworkRequestId {
        self.request_id
    }
}

impl<'de> Deserialize<'de> for NetworkRequestFinished {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: NetworkRequestFinishedWire| {
            Self::new(wire.request_id)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetworkRequestFailed {
    request_id: NetworkRequestId,
    method: Option<HttpMethod>,
    resource_type: Option<NetworkResourceType>,
    url: Option<SanitizedUrl>,
    failure: NetworkFailureKind,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkRequestFailedWire {
    request_id: NetworkRequestId,
    method: Option<HttpMethod>,
    resource_type: Option<NetworkResourceType>,
    url: Option<SanitizedUrl>,
    failure: NetworkFailureKind,
}

impl NetworkRequestFailed {
    pub fn new(
        request_id: NetworkRequestId,
        method: Option<HttpMethod>,
        resource_type: Option<NetworkResourceType>,
        url: Option<SanitizedUrl>,
        failure: NetworkFailureKind,
    ) -> Result<Self> {
        validate_request_id(request_id)?;
        Ok(Self {
            request_id,
            method,
            resource_type,
            url,
            failure,
        })
    }

    pub const fn request_id(&self) -> NetworkRequestId {
        self.request_id
    }

    pub const fn method(&self) -> Option<&HttpMethod> {
        self.method.as_ref()
    }

    pub const fn resource_type(&self) -> Option<NetworkResourceType> {
        self.resource_type
    }

    pub const fn url(&self) -> Option<&SanitizedUrl> {
        self.url.as_ref()
    }

    pub const fn failure(&self) -> NetworkFailureKind {
        self.failure
    }
}

impl<'de> Deserialize<'de> for NetworkRequestFailed {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: NetworkRequestFailedWire| {
            Self::new(
                wire.request_id,
                wire.method,
                wire.resource_type,
                wire.url,
                wire.failure,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationFrameScope {
    Main,
    Child,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationTransition {
    Link,
    Typed,
    AutoBookmark,
    AutoSubframe,
    ManualSubframe,
    Generated,
    StartPage,
    FormSubmit,
    Reload,
    Keyword,
    KeywordGenerated,
    SameDocument,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationEvent {
    frame_scope: NavigationFrameScope,
    transition: NavigationTransition,
    url: Option<SanitizedUrl>,
}

impl NavigationEvent {
    pub const fn new(
        frame_scope: NavigationFrameScope,
        transition: NavigationTransition,
        url: Option<SanitizedUrl>,
    ) -> Self {
        Self {
            frame_scope,
            transition,
            url,
        }
    }

    pub const fn frame_scope(&self) -> NavigationFrameScope {
        self.frame_scope
    }

    pub const fn transition(&self) -> NavigationTransition {
        self.transition
    }

    pub const fn url(&self) -> Option<&SanitizedUrl> {
        self.url.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageLifecycleName {
    Init,
    Commit,
    DomContentLoaded,
    Load,
    NetworkAlmostIdle,
    NetworkIdle,
    FirstPaint,
    FirstContentfulPaint,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageLifecycleEvent {
    frame_scope: NavigationFrameScope,
    name: PageLifecycleName,
}

impl PageLifecycleEvent {
    pub const fn new(frame_scope: NavigationFrameScope, name: PageLifecycleName) -> Self {
        Self { frame_scope, name }
    }

    pub const fn frame_scope(&self) -> NavigationFrameScope {
        self.frame_scope
    }

    pub const fn name(&self) -> PageLifecycleName {
        self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetLifecycleEvent {
    lifecycle: TargetLifecycle,
}

impl TargetLifecycleEvent {
    pub const fn new(lifecycle: TargetLifecycle) -> Self {
        Self { lifecycle }
    }

    pub const fn lifecycle(&self) -> TargetLifecycle {
        self.lifecycle
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetVisibilityEvent {
    visibility: TargetVisibility,
}

impl TargetVisibilityEvent {
    pub const fn new(visibility: TargetVisibility) -> Self {
        Self { visibility }
    }

    pub const fn visibility(&self) -> TargetVisibility {
        self.visibility
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserDialogType {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
    Other,
}

impl BrowserDialogType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::Confirm => "confirm",
            Self::Prompt => "prompt",
            Self::BeforeUnload => "beforeunload",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogOpenedEvent {
    dialog_type: BrowserDialogType,
    had_message: bool,
    had_default_prompt: bool,
}

impl DialogOpenedEvent {
    pub const fn new(
        dialog_type: BrowserDialogType,
        had_message: bool,
        had_default_prompt: bool,
    ) -> Self {
        Self {
            dialog_type,
            had_message,
            had_default_prompt,
        }
    }

    pub const fn dialog_type(&self) -> BrowserDialogType {
        self.dialog_type
    }

    pub const fn had_message(&self) -> bool {
        self.had_message
    }

    pub const fn had_default_prompt(&self) -> bool {
        self.had_default_prompt
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogClosedEvent {
    dialog_type: BrowserDialogType,
    accepted: bool,
    had_user_input: bool,
}

impl DialogClosedEvent {
    pub const fn new(dialog_type: BrowserDialogType, accepted: bool, had_user_input: bool) -> Self {
        Self {
            dialog_type,
            accepted,
            had_user_input,
        }
    }

    pub const fn dialog_type(&self) -> BrowserDialogType {
        self.dialog_type
    }

    pub const fn accepted(&self) -> bool {
        self.accepted
    }

    pub const fn had_user_input(&self) -> bool {
        self.had_user_input
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEventCollectionStatus {
    Disabled,
    Starting,
    Operational,
    Degraded,
    Suspended,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserEventCollectionState {
    status: BrowserEventCollectionStatus,
    unavailable_classes: Vec<BrowserEventClass>,
    dropped_count: u64,
    persisted_count: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventCollectionStateWire {
    status: BrowserEventCollectionStatus,
    unavailable_classes: Vec<BrowserEventClass>,
    dropped_count: u64,
    persisted_count: u64,
}

impl BrowserEventCollectionState {
    pub fn new(
        status: BrowserEventCollectionStatus,
        mut unavailable_classes: Vec<BrowserEventClass>,
        dropped_count: u64,
        persisted_count: u64,
    ) -> Result<Self> {
        unavailable_classes.sort_unstable();
        unavailable_classes.dedup();
        let state = Self {
            status,
            unavailable_classes,
            dropped_count,
            persisted_count,
        };
        state.validate_contract()?;
        Ok(state)
    }

    pub const fn status(&self) -> BrowserEventCollectionStatus {
        self.status
    }

    pub fn unavailable_classes(&self) -> &[BrowserEventClass] {
        &self.unavailable_classes
    }

    pub const fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    pub const fn persisted_count(&self) -> u64 {
        self.persisted_count
    }
}

impl<'de> Deserialize<'de> for BrowserEventCollectionState {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventCollectionStateWire| {
            let state = Self {
                status: wire.status,
                unavailable_classes: wire.unavailable_classes,
                dropped_count: wire.dropped_count,
                persisted_count: wire.persisted_count,
            };
            state.validate_contract()?;
            Ok(state)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEventGapReason {
    InvalidPayload,
    PayloadLimit,
    QueueSaturated,
    FanoutLag,
    PersistenceRejected,
    SubscriptionClosed,
    SourceUnavailable,
    ReconnectBoundary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserEventCollectionGap {
    reason: BrowserEventGapReason,
    affected_class: Option<BrowserEventClass>,
    range: SessionRange,
    first_ordinal: BrowserEventOrdinal,
    last_ordinal: BrowserEventOrdinal,
    count: NonZeroU64,
    ledger_merged: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventCollectionGapWire {
    reason: BrowserEventGapReason,
    affected_class: Option<BrowserEventClass>,
    range: SessionRange,
    first_ordinal: BrowserEventOrdinal,
    last_ordinal: BrowserEventOrdinal,
    count: NonZeroU64,
    ledger_merged: bool,
}

impl BrowserEventCollectionGap {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reason: BrowserEventGapReason,
        affected_class: Option<BrowserEventClass>,
        range: SessionRange,
        first_ordinal: BrowserEventOrdinal,
        last_ordinal: BrowserEventOrdinal,
        count: NonZeroU64,
        ledger_merged: bool,
    ) -> Result<Self> {
        let gap = Self {
            reason,
            affected_class,
            range,
            first_ordinal,
            last_ordinal,
            count,
            ledger_merged,
        };
        gap.validate_contract()?;
        Ok(gap)
    }

    pub const fn reason(&self) -> BrowserEventGapReason {
        self.reason
    }

    pub const fn affected_class(&self) -> Option<BrowserEventClass> {
        self.affected_class
    }

    pub const fn range(&self) -> SessionRange {
        self.range
    }

    pub const fn first_ordinal(&self) -> BrowserEventOrdinal {
        self.first_ordinal
    }

    pub const fn last_ordinal(&self) -> BrowserEventOrdinal {
        self.last_ordinal
    }

    pub const fn count(&self) -> NonZeroU64 {
        self.count
    }

    pub const fn ledger_merged(&self) -> bool {
        self.ledger_merged
    }
}

impl<'de> Deserialize<'de> for BrowserEventCollectionGap {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventCollectionGapWire| {
            Self::new(
                wire.reason,
                wire.affected_class,
                wire.range,
                wire.first_ordinal,
                wire.last_ordinal,
                wire.count,
                wire.ledger_merged,
            )
        })
    }
}

trait EventPayloadContract {
    fn validate_contract(&self) -> Result<()>;
    fn expected_severity(&self) -> BrowserEventSeverity;
    fn compact_priority(&self, default: u8) -> u8 {
        default
    }
}

impl EventPayloadContract for ConsoleEvent {
    fn validate_contract(&self) -> Result<()> {
        if self.argument_types.len() > MAX_CONSOLE_ARGUMENT_TYPES {
            return Err(invalid("console argument type list exceeds its limit"));
        }
        validate_stack(&self.stack, MAX_EVENT_STACK_FRAMES)
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        self.level.severity()
    }

    fn compact_priority(&self, default: u8) -> u8 {
        match self.level {
            ConsoleLevel::Error => 0,
            ConsoleLevel::Warning => 48,
            _ => default,
        }
    }
}

impl EventPayloadContract for ExceptionEvent {
    fn validate_contract(&self) -> Result<()> {
        if self
            .name
            .as_ref()
            .is_some_and(|name| name.byte_len() > super::MAX_REDACTED_NAME_BYTES)
        {
            return Err(invalid("exception name exceeds its byte limit"));
        }
        validate_stack(&self.stack, MAX_EVENT_STACK_FRAMES)
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        BrowserEventSeverity::Error
    }
}

impl EventPayloadContract for NetworkRequestStarted {
    fn validate_contract(&self) -> Result<()> {
        validate_request_id(self.request_id)?;
        validate_stack(&self.initiator.stack, MAX_NETWORK_INITIATOR_STACK_FRAMES)
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        BrowserEventSeverity::Debug
    }
}

impl EventPayloadContract for NetworkResponseReceived {
    fn validate_contract(&self) -> Result<()> {
        validate_request_id(self.request_id)
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        match self.status.get() {
            500..=999 => BrowserEventSeverity::Error,
            400..=499 => BrowserEventSeverity::Warning,
            _ => BrowserEventSeverity::Info,
        }
    }

    fn compact_priority(&self, default: u8) -> u8 {
        match self.status.get() {
            500..=999 => 16,
            400..=499 => 32,
            _ => default,
        }
    }
}

impl EventPayloadContract for NetworkRequestFinished {
    fn validate_contract(&self) -> Result<()> {
        validate_request_id(self.request_id)
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        BrowserEventSeverity::Debug
    }
}

impl EventPayloadContract for NetworkRequestFailed {
    fn validate_contract(&self) -> Result<()> {
        validate_request_id(self.request_id)
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        if self.failure == NetworkFailureKind::Cancelled {
            BrowserEventSeverity::Warning
        } else {
            BrowserEventSeverity::Error
        }
    }
}

macro_rules! simple_payload_contract {
    ($type:ty, $severity:expr) => {
        impl EventPayloadContract for $type {
            fn validate_contract(&self) -> Result<()> {
                Ok(())
            }

            fn expected_severity(&self) -> BrowserEventSeverity {
                $severity
            }
        }
    };
}

simple_payload_contract!(NavigationEvent, BrowserEventSeverity::Info);
simple_payload_contract!(PageLifecycleEvent, BrowserEventSeverity::Info);
simple_payload_contract!(TargetVisibilityEvent, BrowserEventSeverity::Info);
simple_payload_contract!(DialogOpenedEvent, BrowserEventSeverity::Warning);
simple_payload_contract!(DialogClosedEvent, BrowserEventSeverity::Info);

impl EventPayloadContract for TargetLifecycleEvent {
    fn validate_contract(&self) -> Result<()> {
        Ok(())
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        match self.lifecycle {
            TargetLifecycle::Failed => BrowserEventSeverity::Error,
            TargetLifecycle::Suspended => BrowserEventSeverity::Warning,
            _ => BrowserEventSeverity::Info,
        }
    }
}

impl EventPayloadContract for TargetCaptureStatus {
    fn validate_contract(&self) -> Result<()> {
        Ok(())
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        match self.state() {
            CaptureStreamState::Failed => BrowserEventSeverity::Error,
            CaptureStreamState::PausedBudget | CaptureStreamState::Suspended => {
                BrowserEventSeverity::Warning
            }
            CaptureStreamState::Hidden
            | CaptureStreamState::Draining
            | CaptureStreamState::Stopped => BrowserEventSeverity::Info,
            CaptureStreamState::Starting | CaptureStreamState::Capturing => {
                BrowserEventSeverity::Debug
            }
        }
    }
}

impl EventPayloadContract for BrowserEventCollectionState {
    fn validate_contract(&self) -> Result<()> {
        if self.unavailable_classes.len() > BrowserEventClass::ALL.len() {
            return Err(invalid(
                "unavailable browser event class list exceeds its limit",
            ));
        }
        for window in self.unavailable_classes.windows(2) {
            if window[0] >= window[1] {
                return Err(invalid(
                    "unavailable browser event classes must be unique and registry ordered",
                ));
            }
        }
        if matches!(
            self.status,
            BrowserEventCollectionStatus::Operational | BrowserEventCollectionStatus::Disabled
        ) && !self.unavailable_classes.is_empty()
        {
            return Err(invalid(
                "operational or disabled collection state cannot report unavailable classes",
            ));
        }
        Ok(())
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        match self.status {
            BrowserEventCollectionStatus::Failed => BrowserEventSeverity::Error,
            BrowserEventCollectionStatus::Degraded | BrowserEventCollectionStatus::Suspended => {
                BrowserEventSeverity::Warning
            }
            BrowserEventCollectionStatus::Disabled
            | BrowserEventCollectionStatus::Operational
            | BrowserEventCollectionStatus::Stopped => BrowserEventSeverity::Info,
            BrowserEventCollectionStatus::Starting => BrowserEventSeverity::Debug,
        }
    }
}

impl EventPayloadContract for BrowserEventCollectionGap {
    fn validate_contract(&self) -> Result<()> {
        if self.first_ordinal > self.last_ordinal {
            return Err(invalid(
                "browser event gap first ordinal must not exceed its last ordinal",
            ));
        }
        Ok(())
    }

    fn expected_severity(&self) -> BrowserEventSeverity {
        match self.reason {
            BrowserEventGapReason::PersistenceRejected
            | BrowserEventGapReason::SubscriptionClosed
            | BrowserEventGapReason::SourceUnavailable => BrowserEventSeverity::Error,
            _ => BrowserEventSeverity::Warning,
        }
    }
}

fn validate_stack(stack: &[SanitizedStackFrame], max: usize) -> Result<()> {
    if stack.len() > max {
        return Err(invalid("browser event stack exceeds its frame limit"));
    }
    for frame in stack {
        if frame
            .function_name
            .as_ref()
            .is_some_and(|name| name.byte_len() > super::MAX_REDACTED_FUNCTION_BYTES)
        {
            return Err(invalid("stack function name exceeds its byte limit"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserEventDefinition {
    pub kind: BrowserEventKind,
    pub stable_name: &'static str,
    pub class: BrowserEventClass,
    pub default_compact_priority: u8,
}

macro_rules! define_browser_event_registry {
    ($(
        $variant:ident($payload:ty) => {
            name: $stable_name:literal,
            class: $class:ident,
            priority: $priority:literal $(,)?
        }
    ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        pub enum BrowserEventKind {
            $(#[serde(rename = $stable_name)] $variant),+
        }

        impl BrowserEventKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $stable_name),+
                }
            }

            pub fn from_stable_name(value: &str) -> Option<Self> {
                match value {
                    $($stable_name => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub fn definition(self) -> &'static BrowserEventDefinition {
                BROWSER_EVENT_REGISTRY
                    .iter()
                    .find(|definition| definition.kind == self)
                    .expect("browser event registry contains every generated kind")
            }
        }

        #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
        #[serde(tag = "kind", content = "value")]
        pub enum BrowserEventPayload {
            $(#[serde(rename = $stable_name)] $variant($payload)),+
        }

        #[derive(Deserialize)]
        #[serde(tag = "kind", content = "value", deny_unknown_fields)]
        enum BrowserEventPayloadWire {
            $(#[serde(rename = $stable_name)] $variant($payload)),+
        }

        impl BrowserEventPayload {
            pub const fn kind(&self) -> BrowserEventKind {
                match self {
                    $(Self::$variant(_) => BrowserEventKind::$variant),+
                }
            }

            pub const fn class(&self) -> BrowserEventClass {
                match self {
                    $(Self::$variant(_) => BrowserEventClass::$class),+
                }
            }

            fn validate(&self) -> Result<()> {
                match self {
                    $(Self::$variant(payload) => payload.validate_contract()),+
                }
            }

            fn expected_severity(&self) -> BrowserEventSeverity {
                match self {
                    $(Self::$variant(payload) => payload.expected_severity()),+
                }
            }

            fn compact_priority(&self) -> u8 {
                match self {
                    $(Self::$variant(payload) => payload.compact_priority($priority)),+
                }
            }
        }

        impl<'de> Deserialize<'de> for BrowserEventPayload {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserialize_validated(deserializer, |wire: BrowserEventPayloadWire| {
                    let payload = match wire {
                        $(BrowserEventPayloadWire::$variant(value) => Self::$variant(value)),+
                    };
                    payload.validate()?;
                    Ok(payload)
                })
            }
        }

        pub static BROWSER_EVENT_REGISTRY: &[BrowserEventDefinition] = &[
            $(BrowserEventDefinition {
                kind: BrowserEventKind::$variant,
                stable_name: $stable_name,
                class: BrowserEventClass::$class,
                default_compact_priority: $priority,
            }),+
        ];
    };
}

// This declaration is the only semantic event-kind registry. Stable names,
// payload compatibility, classes, and compact defaults are generated together.
define_browser_event_registry! {
    ConsoleMessage(ConsoleEvent) => {
        name: "console_message", class: Console, priority: 96,
    },
    JavascriptException(ExceptionEvent) => {
        name: "javascript_exception", class: Exception, priority: 0,
    },
    NetworkRequestStarted(NetworkRequestStarted) => {
        name: "network_request_started", class: Network, priority: 176,
    },
    NetworkResponseReceived(NetworkResponseReceived) => {
        name: "network_response_received", class: Network, priority: 144,
    },
    NetworkRequestFinished(NetworkRequestFinished) => {
        name: "network_request_finished", class: Network, priority: 192,
    },
    NetworkRequestFailed(NetworkRequestFailed) => {
        name: "network_request_failed", class: Network, priority: 0,
    },
    Navigation(NavigationEvent) => {
        name: "navigation", class: Navigation, priority: 64,
    },
    PageLifecycle(PageLifecycleEvent) => {
        name: "page_lifecycle", class: Lifecycle, priority: 128,
    },
    TargetLifecycle(TargetLifecycleEvent) => {
        name: "target_lifecycle", class: Target, priority: 128,
    },
    TargetVisibility(TargetVisibilityEvent) => {
        name: "target_visibility", class: Target, priority: 144,
    },
    DialogOpened(DialogOpenedEvent) => {
        name: "dialog_opened", class: Dialog, priority: 64,
    },
    DialogClosed(DialogClosedEvent) => {
        name: "dialog_closed", class: Dialog, priority: 72,
    },
    CaptureStatusChanged(TargetCaptureStatus) => {
        name: "capture_status_changed", class: Capture, priority: 80,
    },
    CollectionStateChanged(BrowserEventCollectionState) => {
        name: "collection_state_changed", class: Operational, priority: 112,
    },
    CollectionGap(BrowserEventCollectionGap) => {
        name: "collection_gap", class: Operational, priority: 8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserEvent {
    id: BrowserEventId,
    session_id: SessionId,
    target_id: TargetId,
    attachment_generation: NonZeroU64,
    ordinal: BrowserEventOrdinal,
    session_time: SessionTime,
    source_time: Option<BrowserSourceTimestamp>,
    observed_time: ObservedTime,
    severity: BrowserEventSeverity,
    payload: BrowserEventPayload,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventWire {
    id: BrowserEventId,
    session_id: SessionId,
    target_id: TargetId,
    attachment_generation: u64,
    ordinal: BrowserEventOrdinal,
    session_time: SessionTime,
    source_time: Option<BrowserSourceTimestamp>,
    observed_time: ObservedTime,
    severity: BrowserEventSeverity,
    payload: BrowserEventPayload,
}

impl BrowserEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: BrowserEventId,
        session_id: SessionId,
        target_id: TargetId,
        attachment_generation: u64,
        ordinal: BrowserEventOrdinal,
        session_time: SessionTime,
        source_time: Option<BrowserSourceTimestamp>,
        observed_time: ObservedTime,
        severity: BrowserEventSeverity,
        payload: BrowserEventPayload,
    ) -> Result<Self> {
        let attachment_generation = NonZeroU64::new(attachment_generation)
            .ok_or_else(|| invalid("browser event attachment generation must be non-zero"))?;
        let event = Self {
            id,
            session_id,
            target_id,
            attachment_generation,
            ordinal,
            session_time,
            source_time,
            observed_time,
            severity,
            payload,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.target_id.as_uuid().is_nil()
        {
            return Err(invalid("browser event scope IDs must not be nil"));
        }
        if self.session_time.as_nanos() > self.observed_time.as_nanos() {
            return Err(invalid(
                "browser event session time must not exceed observed time",
            ));
        }
        if self
            .source_time
            .as_ref()
            .is_some_and(|timestamp| timestamp.time().as_nanos() < 0)
        {
            return Err(invalid("browser source timestamp must not be negative"));
        }
        self.payload.validate()?;
        if self.severity != self.payload.expected_severity() {
            return Err(invalid(
                "browser event severity does not agree with its payload",
            ));
        }
        if let BrowserEventPayload::CaptureStatusChanged(status) = &self.payload
            && (status.target_id() != self.target_id
                || status.attachment_generation() != self.attachment_generation.get())
        {
            return Err(invalid(
                "capture status scope does not agree with its browser event",
            ));
        }
        if let BrowserEventPayload::CollectionGap(gap) = &self.payload
            && !gap.range().contains(self.session_time)
        {
            return Err(invalid(
                "browser collection gap range must contain its event time",
            ));
        }
        if self.serialized_payload_len()? > MAX_BROWSER_EVENT_PAYLOAD_BYTES {
            return Err(invalid(
                "browser event payload exceeds its serialized byte limit",
            ));
        }
        Ok(())
    }

    pub const fn id(&self) -> BrowserEventId {
        self.id
    }

    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub const fn target_id(&self) -> TargetId {
        self.target_id
    }

    pub const fn attachment_generation(&self) -> u64 {
        self.attachment_generation.get()
    }

    pub const fn ordinal(&self) -> BrowserEventOrdinal {
        self.ordinal
    }

    pub const fn session_time(&self) -> SessionTime {
        self.session_time
    }

    pub const fn source_time(&self) -> Option<&BrowserSourceTimestamp> {
        self.source_time.as_ref()
    }

    pub const fn observed_time(&self) -> ObservedTime {
        self.observed_time
    }

    pub const fn severity(&self) -> BrowserEventSeverity {
        self.severity
    }

    pub const fn payload(&self) -> &BrowserEventPayload {
        &self.payload
    }

    pub const fn kind(&self) -> BrowserEventKind {
        self.payload.kind()
    }

    pub const fn class(&self) -> BrowserEventClass {
        self.payload.class()
    }

    pub fn affected_range(&self) -> SessionRange {
        match &self.payload {
            BrowserEventPayload::CollectionGap(gap) => gap.range(),
            _ => SessionRange::new(self.session_time, self.session_time)
                .expect("a point browser event always forms a valid range"),
        }
    }

    pub fn compact_priority(&self) -> u8 {
        self.payload.compact_priority()
    }

    pub fn serialized_payload_len(&self) -> Result<usize> {
        serde_json::to_vec(&self.payload)
            .map(|payload| payload.len())
            .map_err(|_| invalid("browser event payload could not be serialized"))
    }
}

impl<'de> Deserialize<'de> for BrowserEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventWire| {
            Self::new(
                wire.id,
                wire.session_id,
                wire.target_id,
                wire.attachment_generation,
                wire.ordinal,
                wire.session_time,
                wire.source_time,
                wire.observed_time,
                wire.severity,
                wire.payload,
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserEventBatch {
    session_id: SessionId,
    events: Vec<BrowserEvent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserEventBatchWire {
    session_id: SessionId,
    events: Vec<BrowserEvent>,
}

impl BrowserEventBatch {
    pub fn new(session_id: SessionId, events: Vec<BrowserEvent>) -> Result<Self> {
        if session_id.as_uuid().is_nil() {
            return Err(invalid("browser event batch session ID must not be nil"));
        }
        let row_count = NonZeroUsize::new(events.len())
            .ok_or_else(|| invalid("browser event batch must contain at least one row"))?;
        if row_count.get() > MAX_BROWSER_EVENT_BATCH_ROWS {
            return Err(invalid("browser event batch exceeds its row limit"));
        }

        let mut ids = HashSet::with_capacity(events.len());
        let mut last_ordinals = HashMap::<TargetId, BrowserEventOrdinal>::new();
        let mut byte_count = 0usize;
        for event in &events {
            event.validate()?;
            if event.session_id() != session_id {
                return Err(invalid("browser event batch contains another session"));
            }
            if !ids.insert(event.id()) {
                return Err(invalid("browser event batch contains a duplicate event ID"));
            }
            if let Some(previous) = last_ordinals.insert(event.target_id(), event.ordinal())
                && event.ordinal() <= previous
            {
                return Err(invalid(
                    "browser event batch ordinals must increase strictly per target",
                ));
            }
            byte_count = byte_count
                .checked_add(
                    serde_json::to_vec(event)
                        .map_err(|_| invalid("browser event could not be serialized"))?
                        .len(),
                )
                .ok_or_else(|| invalid("browser event batch byte count overflowed"))?;
            if byte_count > MAX_BROWSER_EVENT_BATCH_BYTES {
                return Err(invalid(
                    "browser event batch exceeds its serialized byte limit",
                ));
            }
        }

        Ok(Self { session_id, events })
    }

    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub fn events(&self) -> &[BrowserEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<BrowserEvent> {
        self.events
    }
}

impl<'de> Deserialize<'de> for BrowserEventBatch {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventBatchWire| {
            Self::new(wire.session_id, wire.events)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn session_id() -> SessionId {
        SessionId::from_uuid(id(1))
    }

    fn target_id() -> TargetId {
        TargetId::from_uuid(id(2))
    }

    fn request_id() -> NetworkRequestId {
        NetworkRequestId::from_uuid(id(3))
    }

    fn console_payload(level: ConsoleLevel) -> BrowserEventPayload {
        BrowserEventPayload::ConsoleMessage(ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            level,
            ConsoleMethod::Log,
            vec![ConsoleArgumentType::String],
            Some(EventRedactor.text("safe preview")),
            vec![],
        ))
    }

    fn all_payloads() -> Vec<BrowserEventPayload> {
        let url = SanitizedUrl::sanitize("https://example.test/a.js").unwrap();
        let request = request_id();
        let timing = crate::CaptureTimingSummary::empty();
        vec![
            console_payload(ConsoleLevel::Info),
            BrowserEventPayload::JavascriptException(
                ExceptionEvent::new(None, EventRedactor.text("safe exception"), vec![]).unwrap(),
            ),
            BrowserEventPayload::NetworkRequestStarted(
                NetworkRequestStarted::new(
                    request,
                    HttpMethod::Get,
                    NetworkResourceType::Script,
                    url.clone(),
                    NetworkInitiator::new(NetworkInitiatorKind::Parser, vec![]),
                )
                .unwrap(),
            ),
            BrowserEventPayload::NetworkResponseReceived(
                NetworkResponseReceived::new(
                    request,
                    Some(HttpMethod::Get),
                    Some(NetworkResourceType::Script),
                    Some(url.clone()),
                    HttpStatus::new(200).unwrap(),
                    false,
                    false,
                )
                .unwrap(),
            ),
            BrowserEventPayload::NetworkRequestFinished(
                NetworkRequestFinished::new(request).unwrap(),
            ),
            BrowserEventPayload::NetworkRequestFailed(
                NetworkRequestFailed::new(
                    request,
                    None,
                    None,
                    None,
                    NetworkFailureKind::Connection,
                )
                .unwrap(),
            ),
            BrowserEventPayload::Navigation(NavigationEvent::new(
                NavigationFrameScope::Main,
                NavigationTransition::Link,
                Some(url),
            )),
            BrowserEventPayload::PageLifecycle(PageLifecycleEvent::new(
                NavigationFrameScope::Main,
                PageLifecycleName::Load,
            )),
            BrowserEventPayload::TargetLifecycle(TargetLifecycleEvent::new(
                TargetLifecycle::Recording,
            )),
            BrowserEventPayload::TargetVisibility(TargetVisibilityEvent::new(
                TargetVisibility::Visible,
            )),
            BrowserEventPayload::DialogOpened(DialogOpenedEvent::new(
                BrowserDialogType::Alert,
                true,
                false,
            )),
            BrowserEventPayload::DialogClosed(DialogClosedEvent::new(
                BrowserDialogType::Alert,
                true,
                false,
            )),
            BrowserEventPayload::CaptureStatusChanged(
                TargetCaptureStatus::new(
                    target_id(),
                    1,
                    CaptureStreamState::Capturing,
                    crate::CaptureStatistics::default(),
                    1,
                    0,
                    None,
                    timing.clone(),
                    timing,
                    crate::EveryNthFrame::default(),
                    None,
                )
                .unwrap(),
            ),
            BrowserEventPayload::CollectionStateChanged(
                BrowserEventCollectionState::new(
                    BrowserEventCollectionStatus::Operational,
                    vec![],
                    0,
                    1,
                )
                .unwrap(),
            ),
            BrowserEventPayload::CollectionGap(
                BrowserEventCollectionGap::new(
                    BrowserEventGapReason::QueueSaturated,
                    Some(BrowserEventClass::Console),
                    SessionRange::new(SessionTime::from_nanos(15), SessionTime::from_nanos(15))
                        .unwrap(),
                    BrowserEventOrdinal::new(15).unwrap(),
                    BrowserEventOrdinal::new(15).unwrap(),
                    NonZeroU64::new(1).unwrap(),
                    false,
                )
                .unwrap(),
            ),
        ]
    }

    fn event(event_number: u128, ordinal: u64, payload: BrowserEventPayload) -> BrowserEvent {
        let severity = payload.expected_severity();
        BrowserEvent::new(
            BrowserEventId::from_uuid(id(event_number)),
            session_id(),
            target_id(),
            1,
            BrowserEventOrdinal::new(ordinal).unwrap(),
            SessionTime::from_nanos(ordinal),
            Some(
                BrowserSourceTimestamp::new(
                    BrowserSourceClock::CdpMonotonic,
                    SourceTime::from_nanos(ordinal as i128),
                    false,
                )
                .unwrap(),
            ),
            ObservedTime::from_nanos(ordinal + 1),
            severity,
            payload,
        )
        .unwrap()
    }

    #[test]
    fn generated_registry_is_complete_unique_and_drives_stable_contracts() {
        assert_eq!(BrowserEventKind::ALL.len(), BROWSER_EVENT_REGISTRY.len());
        let mut names = HashSet::new();
        for kind in BrowserEventKind::ALL {
            let definition = kind.definition();
            assert_eq!(definition.kind, *kind);
            assert_eq!(definition.stable_name, kind.as_str());
            assert!(names.insert(definition.stable_name));
            assert_eq!(
                BrowserEventKind::from_stable_name(kind.as_str()),
                Some(*kind)
            );
            assert_eq!(
                serde_json::from_str::<BrowserEventKind>(&serde_json::to_string(kind).unwrap())
                    .unwrap(),
                *kind
            );
        }
        assert_eq!(BrowserEventKind::from_stable_name("unknown"), None);

        let payloads = all_payloads();
        assert_eq!(payloads.len(), BrowserEventKind::ALL.len());
        for (index, payload) in payloads.into_iter().enumerate() {
            let kind = BrowserEventKind::ALL[index];
            assert_eq!(payload.kind(), kind);
            assert_eq!(payload.class(), kind.definition().class);
            payload.validate().unwrap();
            let value = event(100 + index as u128, index as u64 + 1, payload);
            assert_eq!(value.kind(), kind);
            assert_eq!(value.class(), kind.definition().class);
            assert_eq!(
                serde_json::from_str::<BrowserEvent>(&serde_json::to_string(&value).unwrap())
                    .unwrap(),
                value
            );
        }
    }

    #[test]
    fn event_round_trip_validates_scope_clocks_severity_and_payload_shape() {
        let value = event(10, 1, console_payload(ConsoleLevel::Error));
        assert_eq!(value.kind(), BrowserEventKind::ConsoleMessage);
        assert_eq!(value.class(), BrowserEventClass::Console);
        assert_eq!(value.compact_priority(), 0);
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<BrowserEvent>(&encoded).unwrap(),
            value
        );

        let mut malformed = serde_json::to_value(&value).unwrap();
        malformed["severity"] = serde_json::json!("info");
        assert!(serde_json::from_value::<BrowserEvent>(malformed).is_err());
        let mut malformed = serde_json::to_value(&value).unwrap();
        malformed["attachment_generation"] = serde_json::json!(0);
        assert!(serde_json::from_value::<BrowserEvent>(malformed).is_err());
        let mut malformed = serde_json::to_value(&value).unwrap();
        malformed["session_time"] = serde_json::json!(100);
        assert!(serde_json::from_value::<BrowserEvent>(malformed).is_err());
        let mut malformed = serde_json::to_value(&value).unwrap();
        malformed["raw_params"] = serde_json::json!({"secret": true});
        assert!(serde_json::from_value::<BrowserEvent>(malformed).is_err());
        assert!(
            BrowserSourceTimestamp::new(
                BrowserSourceClock::UnixEpoch,
                SourceTime::from_nanos(-1),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn payload_registry_rejects_unknown_fields_and_invalid_nested_values() {
        let payload = console_payload(ConsoleLevel::Info);
        let mut value = serde_json::to_value(payload).unwrap();
        value["value"]["headers"] = serde_json::json!({"authorization": "secret"});
        assert!(serde_json::from_value::<BrowserEventPayload>(value).is_err());

        let response = BrowserEventPayload::NetworkResponseReceived(
            NetworkResponseReceived::new(
                request_id(),
                None,
                None,
                None,
                HttpStatus::new(500).unwrap(),
                false,
                false,
            )
            .unwrap(),
        );
        assert_eq!(response.expected_severity(), BrowserEventSeverity::Error);
        assert_eq!(response.compact_priority(), 16);
        assert_eq!(
            serde_json::from_str::<BrowserEventPayload>(&serde_json::to_string(&response).unwrap())
                .unwrap(),
            response
        );
        assert!(HttpStatus::new(1000).is_err());
        assert!(NetworkRequestFinished::new(NetworkRequestId::from_uuid(Uuid::nil())).is_err());
        assert!(
            serde_json::from_value::<NetworkRequestFinished>(serde_json::json!({
                "request_id": Uuid::nil()
            }))
            .is_err()
        );

        let mut console = serde_json::to_value(ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            ConsoleLevel::Info,
            ConsoleMethod::Log,
            vec![],
            None,
            vec![],
        ))
        .unwrap();
        console["argument_types"] = serde_json::json!(vec!["string"; 17]);
        assert!(serde_json::from_value::<ConsoleEvent>(console).is_err());

        assert!(
            serde_json::from_value::<BrowserEventCollectionState>(serde_json::json!({
                "status": "degraded",
                "unavailable_classes": ["network", "network"],
                "dropped_count": 1,
                "persisted_count": 0
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<BrowserEventCollectionGap>(serde_json::json!({
                "reason": "queue_saturated",
                "affected_class": "console",
                "range": {"start": 1, "end": 2},
                "first_ordinal": 2,
                "last_ordinal": 1,
                "count": 1,
                "ledger_merged": false
            }))
            .is_err()
        );
    }

    #[test]
    fn privacy_shapes_cover_console_exception_network_stack_and_dialog_boundaries() {
        let sentinel = "private-sentinel";
        let stack = SanitizedStackFrame::sanitize(
            Some("password=private-sentinel"),
            Some("https://user:private-sentinel@example.test/home/alice/app.js?token=x#f"),
            Some(1),
            Some(2),
        )
        .unwrap();
        let payloads = [
            BrowserEventPayload::ConsoleMessage(ConsoleEvent::new(
                ConsoleEventSource::Log,
                ConsoleLevel::Warning,
                ConsoleMethod::Warning,
                vec![ConsoleArgumentType::String; 20],
                Some(EventRedactor.text("authorization=private-sentinel")),
                vec![stack.clone(); 20],
            )),
            BrowserEventPayload::JavascriptException(
                ExceptionEvent::new(
                    Some(EventRedactor.name("secret=private-sentinel")),
                    EventRedactor.text(r"failed at C:\\Users\\alice\\private-sentinel.rs"),
                    vec![stack.clone()],
                )
                .unwrap(),
            ),
            BrowserEventPayload::NetworkRequestStarted(
                NetworkRequestStarted::new(
                    request_id(),
                    HttpMethod::sanitize("X-PRIVATE-SENTINEL").unwrap(),
                    NetworkResourceType::Fetch,
                    SanitizedUrl::sanitize(
                        "https://user:private-sentinel@example.test/private-sentinel.json?q=private-sentinel#private-sentinel",
                    )
                    .unwrap(),
                    NetworkInitiator::new(NetworkInitiatorKind::Script, vec![stack]),
                )
                .unwrap(),
            ),
            BrowserEventPayload::DialogOpened(DialogOpenedEvent::new(
                BrowserDialogType::Prompt,
                true,
                true,
            )),
            BrowserEventPayload::DialogClosed(DialogClosedEvent::new(
                BrowserDialogType::Prompt,
                true,
                true,
            )),
        ];
        for payload in payloads {
            let encoded = serde_json::to_string(&payload).unwrap();
            assert!(!encoded.contains(sentinel));
            for forbidden_field in [
                "headers",
                "cookies",
                "body",
                "raw_params",
                "session_id",
                "dialog_message",
                "prompt_text",
                "upload_path",
                "fill_value",
                "raw_path",
                "basename",
            ] {
                assert!(!encoded.contains(forbidden_field));
            }
            assert_eq!(
                serde_json::from_str::<BrowserEventPayload>(&encoded).unwrap(),
                payload
            );
        }
    }

    #[test]
    fn payload_and_batch_limits_and_ordering_are_enforced() {
        let oversized_stack = (0..MAX_EVENT_STACK_FRAMES)
            .map(|_| {
                SanitizedStackFrame::sanitize(
                    Some(&"f".repeat(super::super::MAX_REDACTED_FUNCTION_BYTES)),
                    Some(&format!("https://{}.test/a.js", "x".repeat(440))),
                    None,
                    None,
                )
                .unwrap()
            })
            .collect();
        let oversized = BrowserEventPayload::ConsoleMessage(ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            ConsoleLevel::Info,
            ConsoleMethod::Log,
            vec![],
            Some(EventRedactor.text(&"x".repeat(super::super::MAX_REDACTED_TEXT_BYTES))),
            oversized_stack,
        ));
        let result = BrowserEvent::new(
            BrowserEventId::from_uuid(id(20)),
            session_id(),
            target_id(),
            1,
            BrowserEventOrdinal::new(1).unwrap(),
            SessionTime::ZERO,
            None,
            ObservedTime::from_nanos(1),
            BrowserEventSeverity::Info,
            oversized,
        );
        assert!(result.is_err());

        let bounded_large_stack = (0..8)
            .map(|_| {
                SanitizedStackFrame::sanitize(
                    Some(&"f".repeat(100)),
                    Some(&format!("https://{}.test/a.js", "x".repeat(200))),
                    None,
                    None,
                )
                .unwrap()
            })
            .collect();
        let bounded_large_payload = BrowserEventPayload::ConsoleMessage(ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            ConsoleLevel::Info,
            ConsoleMethod::Log,
            vec![],
            None,
            bounded_large_stack,
        ));
        let probe = event(20_000, 1, bounded_large_payload.clone());
        let event_bytes = serde_json::to_vec(&probe).unwrap().len();
        let rows_needed = MAX_BROWSER_EVENT_BATCH_BYTES / event_bytes + 1;
        assert!(rows_needed <= MAX_BROWSER_EVENT_BATCH_ROWS);
        let byte_heavy_batch = (0..rows_needed)
            .map(|index| {
                event(
                    20_001 + index as u128,
                    index as u64 + 1,
                    bounded_large_payload.clone(),
                )
            })
            .collect();
        assert!(BrowserEventBatch::new(session_id(), byte_heavy_batch).is_err());

        let first = event(30, 1, console_payload(ConsoleLevel::Info));
        let second = event(31, 2, console_payload(ConsoleLevel::Info));
        let batch =
            BrowserEventBatch::new(session_id(), vec![first.clone(), second.clone()]).unwrap();
        assert_eq!(batch.events().len(), 2);
        assert_eq!(
            serde_json::from_str::<BrowserEventBatch>(&serde_json::to_string(&batch).unwrap())
                .unwrap(),
            batch
        );
        assert!(BrowserEventBatch::new(session_id(), vec![]).is_err());
        assert!(BrowserEventBatch::new(session_id(), vec![first.clone(), first]).is_err());
        assert!(
            BrowserEventBatch::new(
                session_id(),
                vec![second, event(32, 1, console_payload(ConsoleLevel::Info))]
            )
            .is_err()
        );

        let too_many: Vec<_> = (0..=MAX_BROWSER_EVENT_BATCH_ROWS)
            .map(|index| {
                event(
                    1_000 + index as u128,
                    index as u64 + 1,
                    console_payload(ConsoleLevel::Debug),
                )
            })
            .collect();
        assert!(BrowserEventBatch::new(session_id(), too_many).is_err());
        assert!(BrowserEventOrdinal::new(0).is_err());
    }
}

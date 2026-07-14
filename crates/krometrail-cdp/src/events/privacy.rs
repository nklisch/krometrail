use krometrail_core::{
    BrowserDialogType, BrowserSourceClock, BrowserSourceTimestamp, ConsoleArgumentType,
    ConsoleLevel, ConsoleMethod, EventRedactor, HttpMethod, NetworkFailureKind, NetworkInitiator,
    NetworkInitiatorKind, NetworkResourceType, PageLifecycleName, RedactedText,
    SanitizedStackFrame, SanitizedUrl, SourceTime,
};
use serde_json::Value;

use super::normalize::NormalizeError;

pub(super) fn source_seconds(value: Option<&Value>) -> Option<BrowserSourceTimestamp> {
    source_timestamp(value, BrowserSourceClock::CdpMonotonic, 1_000_000_000.0)
}

pub(super) fn source_epoch_millis(value: Option<&Value>) -> Option<BrowserSourceTimestamp> {
    source_timestamp(value, BrowserSourceClock::UnixEpoch, 1_000_000.0)
}

fn source_timestamp(
    value: Option<&Value>,
    clock: BrowserSourceClock,
    multiplier: f64,
) -> Option<BrowserSourceTimestamp> {
    let raw = value?.as_f64()?;
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    let scaled = raw * multiplier;
    let rounded = scaled.round();
    if !rounded.is_finite() || rounded < 0.0 || rounded > i128::MAX as f64 {
        return None;
    }
    BrowserSourceTimestamp::new(
        clock,
        SourceTime::from_nanos(rounded as i128),
        scaled.fract() != 0.0,
    )
    .ok()
}

pub(super) fn sanitized_url(value: Option<&Value>) -> Result<Option<SanitizedUrl>, NormalizeError> {
    let Some(raw) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    SanitizedUrl::sanitize(raw)
        .map(Some)
        .map_err(|_| NormalizeError::InvalidPayload)
}

pub(super) fn stack_frames(value: Option<&Value>, limit: usize) -> Vec<SanitizedStackFrame> {
    value
        .and_then(|value| value.get("callFrames").or(Some(value)))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(|frame| {
            let function_name = frame
                .get("functionName")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|value| EventRedactor.function_name(value));
            let url = frame
                .get("url")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .and_then(|value| SanitizedUrl::sanitize(value).ok());
            let line = bounded_u32(frame.get("lineNumber"));
            let column = bounded_u32(frame.get("columnNumber"));
            SanitizedStackFrame::new(function_name, url, line, column).ok()
        })
        .collect()
}

fn bounded_u32(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

pub(super) fn console_argument_types(arguments: Option<&Value>) -> Vec<ConsoleArgumentType> {
    arguments
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(krometrail_core::MAX_CONSOLE_ARGUMENT_TYPES)
        .map(
            |argument| match argument.get("type").and_then(Value::as_str) {
                Some("undefined") => ConsoleArgumentType::Undefined,
                Some("boolean") => ConsoleArgumentType::Boolean,
                Some("number") => ConsoleArgumentType::Number,
                Some("string") => ConsoleArgumentType::String,
                Some("bigint") => ConsoleArgumentType::BigInt,
                Some("symbol") => ConsoleArgumentType::Symbol,
                Some("function") => ConsoleArgumentType::Function,
                Some("object")
                    if argument.get("subtype").and_then(Value::as_str) == Some("null") =>
                {
                    ConsoleArgumentType::Null
                }
                _ => ConsoleArgumentType::Object,
            },
        )
        .collect()
}

pub(super) fn console_preview(arguments: Option<&Value>) -> Option<RedactedText> {
    let mut preview = String::new();
    for argument in arguments
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(krometrail_core::MAX_CONSOLE_ARGUMENT_TYPES)
    {
        let primitive = match argument.get("type").and_then(Value::as_str) {
            Some("undefined") => Some("undefined".to_owned()),
            Some("object") if argument.get("subtype").and_then(Value::as_str) == Some("null") => {
                Some("null".to_owned())
            }
            Some("string") => argument
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned),
            Some("boolean") => argument
                .get("value")
                .and_then(Value::as_bool)
                .map(|value| value.to_string()),
            Some("number") => argument
                .get("value")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .map(|value| value.to_string()),
            Some("bigint") => argument
                .get("unserializableValue")
                .and_then(Value::as_str)
                .filter(|value| value.len() <= 128)
                .map(str::to_owned),
            _ => None,
        };
        if let Some(primitive) = primitive {
            if !preview.is_empty() {
                preview.push(' ');
            }
            preview.push_str(&primitive);
        }
    }
    (!preview.is_empty()).then(|| EventRedactor.text(&preview))
}

pub(super) fn console_level(value: Option<&Value>) -> ConsoleLevel {
    match value
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("error") | Some("assert") => ConsoleLevel::Error,
        Some("warning") | Some("warn") => ConsoleLevel::Warning,
        Some("debug") | Some("verbose") => ConsoleLevel::Debug,
        _ => ConsoleLevel::Info,
    }
}

pub(super) fn console_method(value: Option<&Value>) -> ConsoleMethod {
    match value
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("log") => ConsoleMethod::Log,
        Some("debug") => ConsoleMethod::Debug,
        Some("info") => ConsoleMethod::Info,
        Some("error") => ConsoleMethod::Error,
        Some("warning") | Some("warn") => ConsoleMethod::Warning,
        Some("dir") => ConsoleMethod::Dir,
        Some("dirxml") => ConsoleMethod::DirXml,
        Some("table") => ConsoleMethod::Table,
        Some("trace") => ConsoleMethod::Trace,
        Some("clear") => ConsoleMethod::Clear,
        Some("startgroup") => ConsoleMethod::StartGroup,
        Some("startgroupcollapsed") => ConsoleMethod::StartGroupCollapsed,
        Some("endgroup") => ConsoleMethod::EndGroup,
        Some("assert") => ConsoleMethod::Assert,
        Some("profile") => ConsoleMethod::Profile,
        Some("profileend") => ConsoleMethod::ProfileEnd,
        Some("count") => ConsoleMethod::Count,
        Some("timeend") => ConsoleMethod::TimeEnd,
        _ => ConsoleMethod::Other,
    }
}

pub(super) fn http_method(value: Option<&Value>) -> Result<Option<HttpMethod>, NormalizeError> {
    value
        .and_then(Value::as_str)
        .map(HttpMethod::sanitize)
        .transpose()
        .map_err(|_| NormalizeError::InvalidPayload)
}

pub(super) fn resource_type(value: Option<&Value>) -> Option<NetworkResourceType> {
    value
        .and_then(Value::as_str)
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "document" => NetworkResourceType::Document,
            "stylesheet" => NetworkResourceType::Stylesheet,
            "image" => NetworkResourceType::Image,
            "media" => NetworkResourceType::Media,
            "font" => NetworkResourceType::Font,
            "script" => NetworkResourceType::Script,
            "texttrack" => NetworkResourceType::TextTrack,
            "xhr" => NetworkResourceType::Xhr,
            "fetch" => NetworkResourceType::Fetch,
            "prefetch" => NetworkResourceType::Prefetch,
            "eventsource" => NetworkResourceType::EventSource,
            "websocket" => NetworkResourceType::WebSocket,
            "manifest" => NetworkResourceType::Manifest,
            "signedexchange" => NetworkResourceType::SignedExchange,
            "ping" => NetworkResourceType::Ping,
            "cspviolationreport" => NetworkResourceType::CspViolationReport,
            "preflight" => NetworkResourceType::Preflight,
            _ => NetworkResourceType::Other,
        })
}

pub(super) fn network_initiator(value: Option<&Value>) -> NetworkInitiator {
    let kind = match value
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("parser") => NetworkInitiatorKind::Parser,
        Some("script") => NetworkInitiatorKind::Script,
        Some("preload") => NetworkInitiatorKind::Preload,
        Some("signedexchange") => NetworkInitiatorKind::SignedExchange,
        Some("preflight") => NetworkInitiatorKind::Preflight,
        _ => NetworkInitiatorKind::Other,
    };
    let stack = stack_frames(
        value.and_then(|value| value.get("stack")),
        krometrail_core::MAX_NETWORK_INITIATOR_STACK_FRAMES,
    );
    NetworkInitiator::new(kind, stack)
}

pub(super) fn failure_kind(params: &Value) -> NetworkFailureKind {
    if params.get("canceled").and_then(Value::as_bool) == Some(true) {
        return NetworkFailureKind::Cancelled;
    }
    if params.get("blockedReason").is_some() {
        return NetworkFailureKind::Blocked;
    }
    let lower = params
        .get("errorText")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower.contains("name_not_resolved") || lower.contains("dns") {
        NetworkFailureKind::Dns
    } else if lower.contains("timed_out") || lower.contains("timeout") {
        NetworkFailureKind::Timeout
    } else if lower.contains("connection") || lower.contains("reset") || lower.contains("refused") {
        NetworkFailureKind::Connection
    } else {
        NetworkFailureKind::Other
    }
}

pub(super) fn lifecycle_name(value: Option<&Value>) -> PageLifecycleName {
    match value
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("init") => PageLifecycleName::Init,
        Some("commit") => PageLifecycleName::Commit,
        Some("domcontentloaded") => PageLifecycleName::DomContentLoaded,
        Some("load") => PageLifecycleName::Load,
        Some("networkalmostidle") => PageLifecycleName::NetworkAlmostIdle,
        Some("networkidle") => PageLifecycleName::NetworkIdle,
        Some("firstpaint") => PageLifecycleName::FirstPaint,
        Some("firstcontentfulpaint") => PageLifecycleName::FirstContentfulPaint,
        _ => PageLifecycleName::Other,
    }
}

pub(super) fn dialog_type(value: Option<&Value>) -> BrowserDialogType {
    match value
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("alert") => BrowserDialogType::Alert,
        Some("confirm") => BrowserDialogType::Confirm,
        Some("prompt") => BrowserDialogType::Prompt,
        Some("beforeunload") => BrowserDialogType::BeforeUnload,
        _ => BrowserDialogType::Other,
    }
}

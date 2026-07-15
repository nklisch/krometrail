use crate::{ContractError, Result};

pub(crate) const MAX_SHORT_TEXT: usize = 256;
pub(crate) const MAX_LONG_TEXT: usize = 2_048;

pub(crate) fn validate_relative_path(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ContractError::new(format!(
            "{label} must be a relative POSIX path"
        )));
    }
    validate_safe_text(value, label, MAX_SHORT_TEXT)
}

pub(crate) fn validate_opaque_id(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value.contains("..")
        || value.chars().count() > MAX_SHORT_TEXT
        || value
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || "._-".contains(character)))
    {
        return Err(ContractError::new(format!(
            "{label} must be a bounded opaque identifier"
        )));
    }
    Ok(())
}

pub(crate) fn validate_trial_id(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.chars().count() > MAX_SHORT_TEXT
        || value.contains("..")
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || "._-/:".contains(character)))
    {
        return Err(ContractError::new(format!(
            "{label} must be a bounded canonical trial identifier"
        )));
    }
    Ok(())
}

pub(crate) fn validate_sha256(value: &str, label: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ContractError::new(format!(
            "{label} must use sha256:<64 hex>"
        )));
    };
    if hex.len() != 64
        || hex.bytes().any(|byte| !byte.is_ascii_hexdigit())
        || hex.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(ContractError::new(format!(
            "{label} must use 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

pub(crate) fn validate_git_revision(value: &str, label: &str) -> Result<()> {
    if value.len() != 40 || value.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
        return Err(ContractError::new(format!(
            "{label} must be a 40-character hexadecimal revision"
        )));
    }
    Ok(())
}

pub(crate) fn validate_safe_text(value: &str, label: &str, max_chars: usize) -> Result<()> {
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(ContractError::new(format!(
            "{label} is empty or exceeds its bounded length"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(ContractError::new(format!(
            "{label} contains control characters"
        )));
    }
    let lower = value.to_ascii_lowercase();
    for forbidden in [
        "http://",
        "https://",
        "ws://",
        "wss://",
        "file://",
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "--user-data-dir",
        "authorization:",
        "set-cookie",
        "password",
        "secret",
        "page body",
        "raw browser",
        "adapter error",
        "websocket",
        "cdp error",
        "protocol error",
        "<html",
        "<body",
        "<!doctype",
        "document.",
        "innerhtml",
        "textcontent",
    ] {
        if lower.contains(forbidden) {
            return Err(ContractError::new(format!(
                "{label} contains disallowed machine or sensitive data"
            )));
        }
    }
    for prefix in [
        "/home/",
        "/users/",
        "/private/",
        "/root/",
        "/workspace/",
        "/build/",
        "/tmp/",
        "/var/tmp/",
        "/run/",
        "tmp/",
        "temp/",
        "c:/",
    ] {
        if lower.starts_with(prefix) {
            return Err(ContractError::new(format!(
                "{label} contains an absolute private path"
            )));
        }
    }
    if value.contains('\\') || value.contains("..") {
        return Err(ContractError::new(format!(
            "{label} contains path traversal or separator data"
        )));
    }
    Ok(())
}

/// Applies the generic redaction pass to a serialized manifest without echoing unsafe content.
pub(crate) fn sanitize_serialized<T: serde::Serialize>(value: &T) -> Result<()> {
    let encoded = serde_json::to_value(value)?;
    walk_value(&encoded, "manifest")
}

fn walk_value(value: &serde_json::Value, path: &str) -> Result<()> {
    match value {
        serde_json::Value::String(text) => {
            if text.chars().any(char::is_control) {
                return Err(ContractError::new(format!(
                    "{path} contains control characters"
                )));
            }
            let lower = text.to_ascii_lowercase();
            for forbidden in [
                "http://",
                "https://",
                "ws://",
                "wss://",
                "file://",
                "--remote-debugging-port",
                "--user-data-dir",
                "authorization:",
                "set-cookie",
                "page body",
                "raw browser",
                "adapter error",
                "websocket",
                "cdp error",
                "protocol error",
                "<html",
                "<body",
                "<!doctype",
                "document.",
                "innerhtml",
                "textcontent",
            ] {
                if lower.contains(forbidden) {
                    return Err(ContractError::new(format!(
                        "{path} contains disallowed endpoint or sensitive data"
                    )));
                }
            }
            if lower.starts_with("/home/")
                || lower.starts_with("/users/")
                || lower.starts_with("/private/")
                || lower.starts_with("/root/")
                || lower.starts_with("/workspace/")
                || lower.starts_with("/build/")
                || lower.starts_with("/tmp/")
                || lower.starts_with("/var/tmp/")
                || lower.starts_with("/run/")
                || lower.starts_with("tmp/")
                || lower.starts_with("temp/")
                || lower.starts_with("c:/")
                || text.contains('\\')
            {
                return Err(ContractError::new(format!(
                    "{path} contains a private machine path"
                )));
            }
            Ok(())
        }
        serde_json::Value::Array(values) => values
            .iter()
            .enumerate()
            .try_for_each(|(index, value)| walk_value(value, &format!("{path}[{index}]"))),
        serde_json::Value::Object(values) => values
            .iter()
            .try_for_each(|(key, value)| walk_value(value, &format!("{path}.{key}"))),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            Ok(())
        }
    }
}

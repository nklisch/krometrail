//! Privacy-safe browser-event values.
//!
//! Raw event text and URLs enter these constructors and leave only bounded,
//! redacted data. The types deliberately expose no raw URL path, basename, query,
//! fragment, credentials, or local filesystem path.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    error::{Result, invalid},
    validation::deserialize_validated,
};

pub const MAX_REDACTED_TEXT_BYTES: usize = 2_048;
pub const MAX_REDACTED_NAME_BYTES: usize = 64;
pub const MAX_REDACTED_FUNCTION_BYTES: usize = 128;
const MAX_URL_INPUT_BYTES: usize = 32 * 1_024;
const MAX_ORIGIN_BYTES: usize = 512;
const REDACTED_VALUE: &str = "[redacted]";
const REDACTED_URL: &str = "[redacted-url]";
const REDACTED_PATH: &str = "[redacted-path]";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RedactedText {
    text: String,
    truncated: bool,
    redaction_count: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RedactedTextWire {
    text: String,
    truncated: bool,
    redaction_count: u16,
}

impl RedactedText {
    /// Constructs an already-redacted value. Obvious secret/path/URL material is
    /// rejected so adapters cannot bypass the canonical redactor accidentally.
    pub fn new(text: impl Into<String>, truncated: bool, redaction_count: u16) -> Result<Self> {
        Self::new_with_limit(
            text.into(),
            truncated,
            redaction_count,
            MAX_REDACTED_TEXT_BYTES,
        )
    }

    fn new_with_limit(
        text: String,
        truncated: bool,
        redaction_count: u16,
        max_bytes: usize,
    ) -> Result<Self> {
        if text.len() > max_bytes {
            return Err(invalid("redacted event text exceeds its byte limit"));
        }
        let (checked, detected) = redact_fragments(&text);
        if detected != 0 || checked != text {
            return Err(invalid("event text has not passed the privacy redactor"));
        }
        Ok(Self {
            text,
            truncated,
            redaction_count,
        })
    }

    fn from_redactor(text: String, truncated: bool, redaction_count: u16) -> Self {
        Self {
            text,
            truncated,
            redaction_count,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    pub const fn redaction_count(&self) -> u16 {
        self.redaction_count
    }

    pub fn byte_len(&self) -> usize {
        self.text.len()
    }
}

impl<'de> Deserialize<'de> for RedactedText {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: RedactedTextWire| {
            Self::new(wire.text, wire.truncated, wire.redaction_count)
        })
    }
}

/// The one text sanitizer for console previews, exception text/names, and stack
/// function names. It intentionally favors privacy over preserving formatting.
#[derive(Clone, Copy, Debug, Default)]
pub struct EventRedactor;

impl EventRedactor {
    pub fn text(self, input: &str) -> RedactedText {
        self.redact(input, MAX_REDACTED_TEXT_BYTES)
    }

    pub fn name(self, input: &str) -> RedactedText {
        self.redact(input, MAX_REDACTED_NAME_BYTES)
    }

    pub fn function_name(self, input: &str) -> RedactedText {
        self.redact(input, MAX_REDACTED_FUNCTION_BYTES)
    }

    fn redact(self, input: &str, max_bytes: usize) -> RedactedText {
        let (mut text, redaction_count) = redact_fragments(input);
        let truncated = text.len() > max_bytes;
        if truncated {
            truncate_utf8(&mut text, max_bytes);
        }
        RedactedText::from_redactor(text, truncated, redaction_count)
    }
}

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn redact_fragments(input: &str) -> (String, u16) {
    let segments: Vec<&str> = input.split_inclusive(char::is_whitespace).collect();
    let mut output = String::with_capacity(input.len().min(MAX_REDACTED_TEXT_BYTES));
    let mut count = 0u16;
    let mut redact_next_value = false;

    for (index, segment) in segments.iter().enumerate() {
        let token_len = segment.trim_end_matches(char::is_whitespace).len();
        let (token, whitespace) = segment.split_at(token_len);
        if token.is_empty() {
            output.push_str(whitespace);
            continue;
        }

        if redact_next_value {
            let pending = trim_token(token).to_ascii_lowercase();
            if is_redacted_placeholder(token) {
                output.push_str(token);
                redact_next_value = false;
            } else if matches!(pending.as_str(), "=" | ":" | "bearer" | "basic") {
                output.push_str(token);
            } else {
                output.push_str(REDACTED_VALUE);
                count = count.saturating_add(1);
                redact_next_value = false;
            }
            output.push_str(whitespace);
            continue;
        }

        let lower = trim_token(token).to_ascii_lowercase();
        if matches!(lower.as_str(), "bearer" | "basic") {
            output.push_str(token);
            redact_next_value = true;
        } else if let Some(separator) = token.find(['=', ':']) {
            let key = normalize_key(&token[..separator]);
            if is_sensitive_key(&key) {
                output.push_str(&token[..=separator]);
                if separator + 1 < token.len() {
                    if is_redacted_placeholder(&token[separator + 1..]) {
                        output.push_str(&token[separator + 1..]);
                    } else {
                        output.push_str(REDACTED_VALUE);
                        count = count.saturating_add(1);
                    }
                } else {
                    redact_next_value = true;
                }
            } else if looks_like_url(token) {
                output.push_str(REDACTED_URL);
                count = count.saturating_add(1);
            } else if looks_like_absolute_path(token) {
                output.push_str(REDACTED_PATH);
                count = count.saturating_add(1);
            } else {
                output.push_str(token);
            }
        } else if looks_like_url(token) {
            output.push_str(REDACTED_URL);
            count = count.saturating_add(1);
        } else if looks_like_absolute_path(token) {
            output.push_str(REDACTED_PATH);
            count = count.saturating_add(1);
        } else {
            let next = segments
                .iter()
                .skip(index + 1)
                .map(|part| trim_token(part.trim()))
                .find(|part| !part.is_empty());
            if is_sensitive_key(&normalize_key(token)) && matches!(next, Some("=" | ":")) {
                redact_next_value = true;
            }
            output.push_str(token);
        }
        output.push_str(whitespace);
    }

    // `split_inclusive` returns no segment for an empty input and otherwise
    // includes the unterminated tail, so no separate tail copy is needed.
    (output, count)
}

fn is_redacted_placeholder(value: &str) -> bool {
    value.strip_prefix(REDACTED_VALUE).is_some_and(|suffix| {
        suffix
            .chars()
            .all(|character| character.is_ascii_punctuation())
    })
}

fn trim_token(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character.is_ascii_punctuation() && !matches!(character, '=' | ':' | '/' | '\\' | '-' | '_')
    })
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_sensitive_key(value: &str) -> bool {
    matches!(
        value,
        "password"
            | "passwd"
            | "token"
            | "secret"
            | "authorization"
            | "apikey"
            | "session"
            | "cookie"
            | "setcookie"
    )
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("://")
        || lower.starts_with("file:")
        || lower.starts_with("data:")
        || lower.starts_with("blob:")
}

fn looks_like_absolute_path(value: &str) -> bool {
    let value = trim_token(value);
    value.starts_with('/')
        || value.starts_with("\\\\")
        || (value.len() >= 3
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'/' | b'\\'))
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SanitizedUrlScheme {
    Http,
    Https,
    Ws,
    Wss,
    File,
    Data,
    Blob,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SanitizedUrl {
    scheme: SanitizedUrlScheme,
    origin: Option<String>,
    non_default_port: Option<u16>,
    path_sha256: Option<[u8; 32]>,
    path_segment_count: u16,
    extension: Option<String>,
    had_credentials: bool,
    had_query: bool,
    had_fragment: bool,
    fully_redacted: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SanitizedUrlWire {
    scheme: SanitizedUrlScheme,
    origin: Option<String>,
    non_default_port: Option<u16>,
    path_sha256: Option<[u8; 32]>,
    path_segment_count: u16,
    extension: Option<String>,
    had_credentials: bool,
    had_query: bool,
    had_fragment: bool,
    fully_redacted: bool,
}

impl SanitizedUrl {
    /// Sanitizes a URL without introducing a URL-parser dependency into core.
    /// Adapters may use a stricter parser before this boundary, but durable URL
    /// identity is always reduced to this allowlisted shape.
    pub fn sanitize(raw: &str) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_URL_INPUT_BYTES {
            return Err(invalid(
                "browser event URL is empty or exceeds its input limit",
            ));
        }
        if raw.chars().any(char::is_control) {
            return Err(invalid("browser event URL contains control characters"));
        }

        let (without_fragment, had_fragment) = split_once_flag(raw, '#');
        let (without_query, had_query) = split_once_flag(without_fragment, '?');
        let Some((raw_scheme, remainder)) = without_query.split_once(':') else {
            return Self::make_fully_redacted(SanitizedUrlScheme::Other, had_query, had_fragment);
        };
        let scheme = match raw_scheme.to_ascii_lowercase().as_str() {
            "http" => SanitizedUrlScheme::Http,
            "https" => SanitizedUrlScheme::Https,
            "ws" => SanitizedUrlScheme::Ws,
            "wss" => SanitizedUrlScheme::Wss,
            "file" => SanitizedUrlScheme::File,
            "data" => SanitizedUrlScheme::Data,
            "blob" => SanitizedUrlScheme::Blob,
            _ => SanitizedUrlScheme::Other,
        };

        match scheme {
            SanitizedUrlScheme::Http
            | SanitizedUrlScheme::Https
            | SanitizedUrlScheme::Ws
            | SanitizedUrlScheme::Wss => {
                Self::sanitize_network(scheme, remainder, had_query, had_fragment)
            }
            SanitizedUrlScheme::File => {
                let path = remainder.strip_prefix("//").unwrap_or(remainder);
                Self::from_parts(
                    scheme,
                    None,
                    None,
                    Some(path_hash(path)),
                    path_segment_count(path),
                    allowlisted_extension(path),
                    false,
                    had_query,
                    had_fragment,
                    false,
                )
            }
            SanitizedUrlScheme::Data | SanitizedUrlScheme::Blob | SanitizedUrlScheme::Other => {
                Self::make_fully_redacted(scheme, had_query, had_fragment)
            }
        }
    }

    fn sanitize_network(
        scheme: SanitizedUrlScheme,
        remainder: &str,
        had_query: bool,
        had_fragment: bool,
    ) -> Result<Self> {
        let Some(remainder) = remainder.strip_prefix("//") else {
            return Err(invalid("network URL does not contain an authority"));
        };
        let authority_end = remainder.find(['/', '\\']).unwrap_or(remainder.len());
        let authority = &remainder[..authority_end];
        let path = if authority_end == remainder.len() {
            "/"
        } else {
            &remainder[authority_end..]
        };
        let (authority, had_credentials) = match authority.rsplit_once('@') {
            Some((_, safe_authority)) => (safe_authority, true),
            None => (authority, false),
        };
        let (host, port) = split_host_port(authority)?;
        let host = host.to_ascii_lowercase();
        if host.is_empty()
            || host.len() > MAX_ORIGIN_BYTES
            || host.chars().any(|character| {
                character.is_whitespace() || matches!(character, '@' | '/' | '\\' | '?' | '#')
            })
        {
            return Err(invalid("network URL has an invalid host"));
        }
        let default_port = match scheme {
            SanitizedUrlScheme::Http | SanitizedUrlScheme::Ws => 80,
            SanitizedUrlScheme::Https | SanitizedUrlScheme::Wss => 443,
            _ => unreachable!("network sanitizer receives a network scheme"),
        };
        let non_default_port = port.filter(|port| *port != default_port);
        let scheme_name = match scheme {
            SanitizedUrlScheme::Http => "http",
            SanitizedUrlScheme::Https => "https",
            SanitizedUrlScheme::Ws => "ws",
            SanitizedUrlScheme::Wss => "wss",
            _ => unreachable!("network sanitizer receives a network scheme"),
        };
        let origin = format!("{scheme_name}://{host}");
        Self::from_parts(
            scheme,
            Some(origin),
            non_default_port,
            Some(path_hash(path)),
            path_segment_count(path),
            allowlisted_extension(path),
            had_credentials,
            had_query,
            had_fragment,
            false,
        )
    }

    fn make_fully_redacted(
        scheme: SanitizedUrlScheme,
        had_query: bool,
        had_fragment: bool,
    ) -> Result<Self> {
        Self::from_parts(
            scheme,
            None,
            None,
            None,
            0,
            None,
            false,
            had_query,
            had_fragment,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        scheme: SanitizedUrlScheme,
        origin: Option<String>,
        non_default_port: Option<u16>,
        path_sha256: Option<[u8; 32]>,
        path_segment_count: u16,
        extension: Option<String>,
        had_credentials: bool,
        had_query: bool,
        had_fragment: bool,
        fully_redacted: bool,
    ) -> Result<Self> {
        if let Some(origin) = &origin {
            let expected_scheme = network_scheme_name(scheme)
                .ok_or_else(|| invalid("only network URLs may retain an origin"))?;
            let prefix = format!("{expected_scheme}://");
            let authority = origin
                .strip_prefix(&prefix)
                .ok_or_else(|| invalid("sanitized URL origin does not agree with its scheme"))?;
            if origin.len() > MAX_ORIGIN_BYTES
                || origin != &origin.to_ascii_lowercase()
                || !is_valid_origin_authority(authority)
            {
                return Err(invalid("sanitized URL origin is invalid"));
            }
        }
        if non_default_port == Some(0)
            || non_default_port.is_some_and(|port| Some(port) == default_port(scheme))
        {
            return Err(invalid(
                "sanitized URL non-default port is invalid for its scheme",
            ));
        }
        if let Some(extension) = &extension
            && (!is_allowlisted_extension(extension)
                || extension != &extension.to_ascii_lowercase())
        {
            return Err(invalid("sanitized URL extension is not allowlisted"));
        }

        let is_network = matches!(
            scheme,
            SanitizedUrlScheme::Http
                | SanitizedUrlScheme::Https
                | SanitizedUrlScheme::Ws
                | SanitizedUrlScheme::Wss
        );
        let shape_is_valid = if is_network {
            origin.is_some() && path_sha256.is_some() && !fully_redacted
        } else if scheme == SanitizedUrlScheme::File {
            origin.is_none()
                && non_default_port.is_none()
                && path_sha256.is_some()
                && !fully_redacted
        } else {
            origin.is_none()
                && non_default_port.is_none()
                && path_sha256.is_none()
                && path_segment_count == 0
                && extension.is_none()
                && fully_redacted
        };
        if !shape_is_valid {
            return Err(invalid("sanitized URL fields do not agree with its scheme"));
        }

        Ok(Self {
            scheme,
            origin,
            non_default_port,
            path_sha256,
            path_segment_count,
            extension,
            had_credentials,
            had_query,
            had_fragment,
            fully_redacted,
        })
    }

    pub const fn scheme(&self) -> SanitizedUrlScheme {
        self.scheme
    }

    pub fn origin(&self) -> Option<&str> {
        self.origin.as_deref()
    }

    pub const fn non_default_port(&self) -> Option<u16> {
        self.non_default_port
    }

    pub const fn path_sha256(&self) -> Option<[u8; 32]> {
        self.path_sha256
    }

    pub const fn path_segment_count(&self) -> u16 {
        self.path_segment_count
    }

    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }

    pub const fn had_credentials(&self) -> bool {
        self.had_credentials
    }

    pub const fn had_query(&self) -> bool {
        self.had_query
    }

    pub const fn had_fragment(&self) -> bool {
        self.had_fragment
    }

    pub const fn fully_redacted(&self) -> bool {
        self.fully_redacted
    }
}

impl<'de> Deserialize<'de> for SanitizedUrl {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: SanitizedUrlWire| {
            Self::from_parts(
                wire.scheme,
                wire.origin,
                wire.non_default_port,
                wire.path_sha256,
                wire.path_segment_count,
                wire.extension,
                wire.had_credentials,
                wire.had_query,
                wire.had_fragment,
                wire.fully_redacted,
            )
        })
    }
}

fn split_once_flag(value: &str, delimiter: char) -> (&str, bool) {
    value
        .split_once(delimiter)
        .map_or((value, false), |(before, _)| (before, true))
}

fn network_scheme_name(scheme: SanitizedUrlScheme) -> Option<&'static str> {
    match scheme {
        SanitizedUrlScheme::Http => Some("http"),
        SanitizedUrlScheme::Https => Some("https"),
        SanitizedUrlScheme::Ws => Some("ws"),
        SanitizedUrlScheme::Wss => Some("wss"),
        _ => None,
    }
}

fn default_port(scheme: SanitizedUrlScheme) -> Option<u16> {
    match scheme {
        SanitizedUrlScheme::Http | SanitizedUrlScheme::Ws => Some(80),
        SanitizedUrlScheme::Https | SanitizedUrlScheme::Wss => Some(443),
        _ => None,
    }
}

fn is_valid_origin_authority(authority: &str) -> bool {
    if authority.is_empty()
        || authority.contains(['@', '/', '\\', '?', '#'])
        || authority.chars().any(char::is_whitespace)
    {
        return false;
    }
    if authority.starts_with('[') {
        return authority.ends_with(']')
            && authority.len() > 2
            && !authority[1..authority.len() - 1].contains(['[', ']']);
    }
    !authority.contains(':')
}

fn split_host_port(authority: &str) -> Result<(&str, Option<u16>)> {
    if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| invalid("network URL has an invalid IPv6 host"))?;
        let host = &authority[..=end];
        let suffix = &authority[end + 1..];
        if suffix.is_empty() {
            return Ok((host, None));
        }
        let port = suffix
            .strip_prefix(':')
            .ok_or_else(|| invalid("network URL authority is invalid"))?
            .parse::<u16>()
            .map_err(|_| invalid("network URL port is invalid"))?;
        return Ok((host, Some(port)));
    }
    if authority.matches(':').count() > 1 {
        return Err(invalid("network URL IPv6 hosts must be bracketed"));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => Ok((
            host,
            Some(
                port.parse::<u16>()
                    .map_err(|_| invalid("network URL port is invalid"))?,
            ),
        )),
        None => Ok((authority, None)),
    }
}

fn path_hash(path: &str) -> [u8; 32] {
    Sha256::digest(path.as_bytes()).into()
}

fn path_segment_count(path: &str) -> u16 {
    path.split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
        .count()
        .min(u16::MAX as usize) as u16
}

fn allowlisted_extension(path: &str) -> Option<String> {
    let basename = path.rsplit(['/', '\\']).next().unwrap_or_default();
    let extension = basename.rsplit_once('.')?.1.to_ascii_lowercase();
    is_allowlisted_extension(&extension).then_some(extension)
}

fn is_allowlisted_extension(extension: &str) -> bool {
    matches!(
        extension,
        "html"
            | "htm"
            | "css"
            | "js"
            | "mjs"
            | "cjs"
            | "json"
            | "xml"
            | "txt"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "svg"
            | "ico"
            | "avif"
            | "wasm"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "mp3"
            | "mp4"
            | "webm"
            | "wav"
            | "pdf"
            | "map"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_corpus_removes_secrets_paths_and_urls_with_utf8_safe_bounds() {
        let cases = [
            ("password=hunter2", "hunter2"),
            ("Authorization: Bearer abc.def", "abc.def"),
            ("cookie=sessionid=secret-cookie", "secret-cookie"),
            ("token = private-token", "private-token"),
            ("at /home/alice/project/secret.rs:12", "/home/alice"),
            (r"at C:\\Users\\alice\\project\\secret.rs:12", "Users"),
            ("file:///Users/alice/private.txt", "alice"),
            (
                "https://user:pass@example.test/private?q=token#fragment",
                "private",
            ),
        ];
        for (input, forbidden) in cases {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains(forbidden),
                "input was not redacted"
            );
            assert!(redacted.redaction_count() > 0);
            assert!(
                serde_json::from_str::<RedactedText>(&serde_json::to_string(&redacted).unwrap())
                    .is_ok()
            );
        }

        let input = "é".repeat(MAX_REDACTED_TEXT_BYTES);
        let redacted = EventRedactor.text(&input);
        assert!(redacted.truncated());
        assert!(redacted.text().len() <= MAX_REDACTED_TEXT_BYTES);
        assert!(redacted.text().is_char_boundary(redacted.text().len()));
    }

    #[test]
    fn sanitized_url_corpus_retains_only_allowlisted_components() {
        let cases = [
            "https://user:pass@Example.TEST:8443/private/account.json?token=abc#details",
            "file:///Users/alice/project/private.txt?ignored=yes#fragment",
            r"file:C:\\Users\\alice\\project\\private.js",
            "data:text/plain,private-value",
            "blob:https://example.test/private-id",
        ];
        for raw in cases {
            let sanitized = SanitizedUrl::sanitize(raw).unwrap();
            let encoded = serde_json::to_string(&sanitized).unwrap();
            for forbidden in [
                "user", "pass", "private", "account", "token", "alice", "project", "details",
            ] {
                assert!(
                    !encoded.contains(forbidden),
                    "leaked {forbidden} from {raw}"
                );
            }
            assert_eq!(
                serde_json::from_str::<SanitizedUrl>(&encoded).unwrap(),
                sanitized
            );
        }

        let network = SanitizedUrl::sanitize(
            "https://user:pass@Example.TEST:8443/a/b/report.JSON?q=1#fragment",
        )
        .unwrap();
        assert_eq!(network.origin(), Some("https://example.test"));
        assert_eq!(network.non_default_port(), Some(8443));
        assert_eq!(network.path_segment_count(), 3);
        assert_eq!(network.extension(), Some("json"));
        assert!(network.had_credentials());
        assert!(network.had_query());
        assert!(network.had_fragment());
        assert!(!network.fully_redacted());
    }

    #[test]
    fn validated_privacy_values_reject_unknown_fields_and_bypasses() {
        assert!(RedactedText::new("password=secret", false, 0).is_err());
        let redacted = EventRedactor.text("safe text");
        let mut value = serde_json::to_value(redacted).unwrap();
        value["raw"] = serde_json::json!("secret");
        assert!(serde_json::from_value::<RedactedText>(value).is_err());

        let mut url =
            serde_json::to_value(SanitizedUrl::sanitize("https://example.test/a").unwrap())
                .unwrap();
        url["raw_path"] = serde_json::json!("/a");
        assert!(serde_json::from_value::<SanitizedUrl>(url).is_err());
    }
}

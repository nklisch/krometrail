//! Privacy-safe browser-event values.
//!
//! Raw event text and URLs enter these constructors and leave only bounded,
//! redacted data. The types deliberately expose no raw URL path, basename, query,
//! fragment, credentials, or local filesystem path.

use crate::{
    Sha256Digest,
    error::{Result, invalid},
    validation::deserialize_validated,
};
use serde::{Deserialize, Serialize};

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
            truncate_to_redaction_stable(&mut text, max_bytes);
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

/// Truncates to the longest prefix within `max_bytes` that the redactor leaves
/// alone.
///
/// Truncation runs after redaction, so a plain byte cut can split a token the
/// redactor judged as a whole and leave behind a fragment a later pass judges
/// differently — `Bearer [` is the short case: the placeholder's opening bracket
/// survives the cut and the next pass reads it as an unredacted value. The
/// emitted text has to satisfy `RedactedText::new`, because that is what
/// `Deserialize` calls when the persisted event is read back, so a cut that
/// leaves the text unstable would make retained evidence unreadable. Shrinking
/// one character at a time terminates: the empty string is always stable.
fn truncate_to_redaction_stable(value: &mut String, max_bytes: usize) {
    truncate_utf8(value, max_bytes);
    while !value.is_empty() {
        let (again, count) = redact_fragments(value);
        if count == 0 && again == *value {
            return;
        }
        let mut end = value.len() - 1;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
}

fn redact_fragments(input: &str) -> (String, u16) {
    let segments: Vec<&str> = input.split_inclusive(char::is_whitespace).collect();
    let mut output = String::with_capacity(input.len().min(MAX_REDACTED_TEXT_BYTES));
    let mut count = 0u16;
    let mut redact_next_value = false;
    // Set when a structured token ended inside an unterminated quoted secret,
    // carrying the quote character that must close it. The replacement has
    // already been emitted, so every following token is dropped until that quote
    // - otherwise a value containing spaces ({"token":"secret value with
    // spaces"}) leaks everything after the first whitespace token. Single quotes
    // count: a console log is not obliged to be JSON.
    let mut inside_quoted_secret: Option<char> = None;
    // Escape state for the scan above, carried across tokens because a quoted
    // secret spans the whitespace between them.
    let mut continuation_escaped = false;

    for (index, segment) in segments.iter().enumerate() {
        let token_len = segment.trim_end_matches(char::is_whitespace).len();
        let (token, whitespace) = segment.split_at(token_len);
        if token.is_empty() {
            if inside_quoted_secret.is_none() {
                output.push_str(whitespace);
            }
            continue;
        }

        if let Some(quote) = inside_quoted_secret {
            if let Some(closing) = find_unescaped_quote(token, quote, &mut continuation_escaped) {
                // The closing quote is consumed with the value it closes. Its
                // opening partner was swallowed by the replacement, so emitting
                // it would leave an orphaned quote that a later pass reads as
                // trailing content and strips — which would make this pass's
                // output an unstable, unreadable value.
                output.push_str(&token[closing + quote.len_utf8()..]);
                output.push_str(whitespace);
                inside_quoted_secret = None;
            }
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
        } else if let Some(structured) = redact_structured_token(token) {
            // A compact structured fragment such as {"outer":{"token":"secret"}}
            // is a single whitespace token, so the single-separator scan below
            // would only inspect the outermost key and emit the nested secret
            // verbatim. This branch is taken only when a nested sensitive key
            // was actually redacted, so URL and path handling below still apply
            // to structured tokens that carry no nested secret.
            output.push_str(&structured.text);
            count = count.saturating_add(structured.count);
            if let Some(quote) = structured.continuation {
                inside_quoted_secret = Some(quote);
                continuation_escaped = false;
                continue;
            }
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

/// Every placeholder the redactor emits.
///
/// The recogniser below is the only place this set is consulted, so a new
/// placeholder cannot be honoured in one code path and unknown in another —
/// which is exactly how `[redacted-url]` and `[redacted-path]` came to be
/// treated as ordinary text by the structured scanner while `[redacted]` was
/// recognised.
const REDACTION_PLACEHOLDERS: [&str; 3] = [REDACTED_VALUE, REDACTED_URL, REDACTED_PATH];

/// Byte length of the placeholder run starting at the front of `value`, or
/// `None` when `value` does not start with one.
///
/// Adjacent placeholders are consumed as a single run. Matching only one would
/// leave the next placeholder as an unrecognised remainder, and because every
/// placeholder opens with `[`, that remainder reads as a fresh nested value —
/// so `[redacted][redacted]secret` would slip its tail past the scanner.
fn redacted_placeholder_run(value: &str) -> Option<usize> {
    let mut length = 0;
    while let Some(matched) = REDACTION_PLACEHOLDERS
        .iter()
        .find(|placeholder| value[length..].starts_with(**placeholder))
    {
        length += matched.len();
    }
    (length > 0).then_some(length)
}

fn is_redacted_placeholder(value: &str) -> bool {
    redacted_placeholder_run(value).is_some_and(|length| {
        value[length..]
            .chars()
            .all(|character| character.is_ascii_punctuation())
    })
}

/// Structural delimiters that separate key/value pairs inside a compact
/// structured fragment carrying no whitespace.
const STRUCTURAL_DELIMITERS: [char; 5] = ['{', '}', '[', ']', ','];

/// Outcome of rewriting one structured token.
struct StructuredRedaction {
    text: String,
    count: u16,
    /// Set when the token ends inside an unterminated sensitive value, carrying
    /// the quote the remainder must be dropped up to.
    continuation: Option<char>,
}

/// Redact sensitive values nested inside a single structured token. Returns
/// `None` when nothing was redacted, so the caller's existing URL and
/// absolute-path handling still applies unchanged to structured tokens that
/// carry nothing sensitive.
fn redact_structured_token(token: &str) -> Option<StructuredRedaction> {
    if !token.contains(STRUCTURAL_DELIMITERS) || !token.contains(['=', ':']) {
        return None;
    }

    let mut text = String::with_capacity(token.len());
    let mut count = 0u16;
    let mut fragment_start = 0;
    let mut continuation = None;
    let mut depth = 0_usize;
    // Set while consuming a nested value that has already been replaced whole,
    // holding the nesting depth it opened at. Nothing inside it is emitted.
    let mut redacted_value_depth: Option<usize> = None;
    // Byte index the scan resumes at after text that has already been emitted
    // verbatim.
    let mut resume_at = 0_usize;

    for (index, character) in token.char_indices() {
        if index < resume_at {
            continue;
        }
        if let Some(opened_at) = redacted_value_depth {
            match character {
                '{' | '[' => depth = depth.saturating_add(1),
                '}' | ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == opened_at {
                        // This delimiter closes the structure that was replaced,
                        // so it is consumed with it and the emitted brackets stay
                        // balanced.
                        redacted_value_depth = None;
                        fragment_start = index + character.len_utf8();
                    }
                }
                _ => {}
            }
            continue;
        }
        if !STRUCTURAL_DELIMITERS.contains(&character) {
            continue;
        }
        let fragment_text = &token[fragment_start..index];
        // A sensitive key whose value is the structure opening here: the whole
        // nested value is the secret. Descending into it instead would judge
        // `{"outer":{"token":{"inner":"secret"}}}` on the inner key alone - which
        // is not sensitive - and emit the secret verbatim.
        if matches!(character, '{' | '[') && opens_sensitive_value(fragment_text) {
            // Re-running the redactor over its own output must be a no-op, and
            // every placeholder opens with a structural delimiter like any array
            // would. Without this branch the placeholder would be treated as a
            // fresh nested value and redacted again on every pass — reporting a
            // redaction while changing nothing, which is exactly what
            // `RedactedText::new` refuses. Worse, the generic nested-value branch
            // matches brackets, so the placeholder's own closing `]` would end
            // the value it thinks it is consuming and release everything after it.
            if let Some(run) = redacted_placeholder_run(&token[index..]) {
                let rest = &token[index + run..];
                text.push_str(fragment_text);
                text.push_str(REDACTED_VALUE);
                // Anything between the placeholder run and the next structural
                // delimiter is not part of it. It sits where the value's own
                // bytes would sit, so it is dropped and counted rather than
                // emitted — otherwise text carrying a literal placeholder prefix
                // could smuggle a secret straight through.
                let trailing = rest.find(STRUCTURAL_DELIMITERS).unwrap_or(rest.len());
                if trailing > 0 {
                    count = count.saturating_add(1);
                }
                resume_at = index + run + trailing;
                fragment_start = resume_at;
                continue;
            }
            text.push_str(fragment_text);
            text.push_str(REDACTED_VALUE);
            count = count.saturating_add(1);
            redacted_value_depth = Some(depth);
            depth = depth.saturating_add(1);
            continue;
        }
        let fragment = redact_key_value_fragment(fragment_text);
        text.push_str(&fragment.text);
        count = count.saturating_add(fragment.count);
        text.push(character);
        fragment_start = index + character.len_utf8();
        match character {
            '{' | '[' => depth = depth.saturating_add(1),
            '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if redacted_value_depth.is_some() {
        // The token ended inside a nested value that was replaced whole, so the
        // remainder runs on into the following tokens and must be dropped there.
        continuation = Some('"');
    } else {
        let fragment = redact_key_value_fragment(&token[fragment_start..]);
        text.push_str(&fragment.text);
        count = count.saturating_add(fragment.count);
        continuation = continuation.or(fragment.continuation);
    }

    (count > 0).then_some(StructuredRedaction {
        text,
        count,
        continuation,
    })
}

/// True when this fragment is a sensitive key whose value has not started yet,
/// so whatever structure follows *is* the value.
fn opens_sensitive_value(fragment: &str) -> bool {
    let Some(separator) = fragment.find(['=', ':']) else {
        return false;
    };
    fragment[separator + 1..].trim().is_empty()
        && is_sensitive_key(&normalize_key(&fragment[..separator]))
}

/// Byte index of the first quote that actually closes a quoted value.
///
/// Escape state is threaded through the caller because a quoted value spans the
/// whitespace between tokens: `{"token":"one \" two"}` splits into three tokens,
/// and treating the escaped quote in the middle one as the terminator would
/// release the rest of the secret.
fn find_unescaped_quote(token: &str, quote: char, escaped: &mut bool) -> Option<usize> {
    for (index, character) in token.char_indices() {
        if *escaped {
            *escaped = false;
            continue;
        }
        if character == '\\' {
            *escaped = true;
        } else if character == quote {
            return Some(index);
        }
    }
    None
}

/// True when a quoted value closes inside the text given.
fn quoted_value_is_terminated(value: &str, quote: char) -> bool {
    let mut escaped = false;
    find_unescaped_quote(&value[quote.len_utf8()..], quote, &mut escaped).is_some()
}

/// Outcome of rewriting one `key:value` fragment.
struct FragmentRedaction {
    text: String,
    count: u16,
    continuation: Option<char>,
}

impl FragmentRedaction {
    fn unchanged(fragment: &str) -> Self {
        Self {
            text: fragment.to_owned(),
            count: 0,
            continuation: None,
        }
    }
}

/// Redact one `key:value` or `key=value` fragment. A sensitive key redacts its
/// value outright; any other key still has its value checked for a URL or
/// absolute path, so nested locations are covered as well as nested secrets.
fn redact_key_value_fragment(fragment: &str) -> FragmentRedaction {
    let Some(separator) = fragment.find(['=', ':']) else {
        return FragmentRedaction::unchanged(fragment);
    };
    let value = &fragment[separator + 1..];
    if value.is_empty() || is_redacted_placeholder(value) {
        return FragmentRedaction::unchanged(fragment);
    }

    let sensitive_key = is_sensitive_key(&normalize_key(&fragment[..separator]));
    let replacement = if sensitive_key {
        REDACTED_VALUE
    } else if looks_like_url(value) {
        REDACTED_URL
    } else if looks_like_absolute_path(value) {
        REDACTED_PATH
    } else {
        return FragmentRedaction::unchanged(fragment);
    };

    let mut text = String::with_capacity(separator + 1 + replacement.len());
    text.push_str(&fragment[..=separator]);
    text.push_str(replacement);
    FragmentRedaction {
        text,
        count: 1,
        // A quoted value with no closing quote runs past this whitespace token,
        // so the remainder must also be dropped by the caller. Either quote
        // style opens one, and only an *unescaped* quote closes it.
        continuation: sensitive_key
            .then(|| value.chars().next())
            .flatten()
            .filter(|quote| matches!(quote, '"' | '\''))
            .filter(|quote| !quoted_value_is_terminated(value, *quote)),
    }
}

fn trim_token(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character.is_ascii_punctuation() && !matches!(character, '=' | ':' | '/' | '\\' | '-' | '_')
    })
}

/// Reduces a key to its comparable letters and digits.
///
/// Escape sequences are decoded first. A JSON producer is free to write
/// `"token"`, and a normalizer that only filtered characters would compare
/// `toku0065n` against the sensitive-key set and find no match - so the secret
/// would be emitted verbatim purely because of how its key was spelled.
fn normalize_key(value: &str) -> String {
    let mut decoded = value.to_owned();
    // Decode to a fixed point rather than exactly once. A producer that escapes
    // its own escapes writes `tok\\u0065n`; one pass turns that into the literal
    // text `e`, whose letters then compare as `toku0065n` and miss the
    // sensitive set entirely. The loop is bounded, and the only error it can
    // introduce is over-normalizing a key — which redacts a value that did not
    // need it. That is the safe direction for a redactor, which is why looping
    // is preferred here over rejecting keys that still carry escape syntax.
    for _ in 0..MAX_KEY_DECODE_PASSES {
        let next = decode_escapes(&decoded);
        if next == decoded {
            break;
        }
        decoded = next;
    }
    decoded
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Decoding passes `normalize_key` will run before giving up. Every real key
/// settles in one or two; the bound only exists so pathological input cannot
/// spin.
const MAX_KEY_DECODE_PASSES: usize = 4;

/// Decodes every escape sequence in `value` once.
fn decode_escapes(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(escaped) = decode_escape(&mut characters) {
                decoded.push(escaped);
            }
        } else {
            decoded.push(character);
        }
    }
    decoded
}

/// Decodes one escape sequence, its leading backslash already consumed.
///
/// `None` means the escape contributes nothing to a key name. The named
/// single-character escapes are listed explicitly so that `\n` yields a newline
/// (dropped) rather than a literal `n` that could invent a key. `\\` is the
/// exception: it denotes a real backslash, and yielding it is what lets a
/// second decoding pass see the escape that was hiding behind it.
fn decode_escape(characters: &mut std::str::Chars<'_>) -> Option<char> {
    match characters.next()? {
        'u' => {
            let mut code = 0_u32;
            for _ in 0..4 {
                code = code * 16 + characters.next()?.to_digit(16)?;
            }
            char::from_u32(code)
        }
        '\\' => Some('\\'),
        'n' | 't' | 'r' | 'b' | 'f' | '"' | '\'' | '/' => None,
        other => Some(other),
    }
}

fn is_sensitive_key(value: &str) -> bool {
    // `normalize_key` has already stripped non-alphanumerics, so `access_token`,
    // `access-token`, and `"accessToken"` all arrive here as `accesstoken`.
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
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "authtoken"
            | "bearertoken"
            | "clientsecret"
            | "apisecret"
            | "privatekey"
            | "secretkey"
            | "sessionid"
            | "sessiontoken"
            | "credentials"
            | "passphrase"
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
    // A single leading backslash covers both UNC (\\server\share) and the
    // rooted drive-relative form (\Users\alice\file) that Chrome can emit.
    if value.starts_with('/') || value.starts_with('\\') {
        return true;
    }
    // A Windows drive designator is `X:`, but the single-letter test alone
    // redacts ordinary prose such as `A:todo`. The remainder has to look like a
    // location in its own right: a path separator (C:\dir, C:/dir, C:foo\bar), a
    // filename extension (C:secret.txt), or nothing at all (C:).
    let bytes = value.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return false;
    }
    let remainder = &value[2..];
    remainder.is_empty()
        || remainder.contains(['/', '\\'])
        || ends_with_filename_extension(remainder)
        || is_dotfile_name(remainder)
}

/// True when the text ends in something shaped like a filename extension: a
/// non-empty stem, a dot, then a short suffix carrying at least one letter.
///
/// This is the discriminator that keeps both directions honest. `C:secret.txt`
/// is a real drive-relative path and must be redacted; `A:todo` and `v:1.0` are
/// prose and a version, and redacting them would train readers to ignore
/// `[redacted-path]`.
fn ends_with_filename_extension(value: &str) -> bool {
    let Some((stem, extension)) = value.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=5).contains(&extension.len())
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        && extension
            .chars()
            .any(|character| character.is_ascii_alphabetic())
}

/// True when the text is a leading-dot filename such as `.bashrc` or `.config`.
///
/// `ends_with_filename_extension` requires a non-empty stem, so a dotfile has no
/// extension by that test and `C:.bashrc` — a real drive-relative path to a
/// user's shell configuration — was emitted verbatim. Requiring at least one
/// letter after the dot keeps `v:1.0`-shaped prose out: that has no leading dot
/// at all, and `C:.5` would be a number, not a name.
fn is_dotfile_name(value: &str) -> bool {
    let Some(name) = value.strip_prefix('.') else {
        return false;
    };
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
        && name
            .chars()
            .any(|character| character.is_ascii_alphabetic())
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
    path_sha256: Option<Sha256Digest>,
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
    path_sha256: Option<Sha256Digest>,
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
        path_sha256: Option<Sha256Digest>,
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

    pub const fn path_sha256(&self) -> Option<Sha256Digest> {
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

fn path_hash(path: &str) -> Sha256Digest {
    Sha256Digest::digest(path.as_bytes())
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

    /// Every input the redactor is known to be exercised with, in one place.
    ///
    /// This is the corpus the idempotence property runs over. Entries are added
    /// here rather than only inside the test that motivated them so a new escape
    /// route is automatically checked for stability as well as for leakage.
    const REDACTION_CORPUS: &[&str] = &[
        // Flat key/value, credential, and location shapes.
        "password=hunter2",
        "Authorization: Bearer abc.def",
        "cookie=sessionid=secret-cookie",
        "token = private-token",
        "at /home/alice/project/secret.rs:12",
        r"at C:\\Users\\alice\\project\\secret.rs:12",
        "file:///Users/alice/private.txt",
        "https://user:pass@example.test/private?q=token#fragment",
        "access-token=abc123",
        "safe text",
        "",
        "   ",
        // Compact structured fragments.
        r#"{"outer":{"token":"secret-value"}}"#,
        r#"{"a":{"b":{"password":"deep-secret"}}}"#,
        r#"[{"apikey":"leaked-key"}]"#,
        r#"{"ok":1,"secret":"tail-secret"}"#,
        r#"{"cookie":"sid=abc123"}"#,
        r#"{"authorization":"Bearer xyz.789"}"#,
        r#"{"token":"secret value with spaces"}"#,
        r#"{"password":"a b c"} trailing"#,
        r#"{"access_token":"abc123"}"#,
        r#"{"refresh_token":"abc123"}"#,
        r#"{"client_secret":"abc123"}"#,
        r#"{"privateKey":"abc123"}"#,
        r#"{"url":"https://user:pass@example.test/private"}"#,
        r#"{"path":"/home/alice/private.txt"}"#,
        r#"{"outer":{"token":{"inner":"LEAK"}}}"#,
        r#"{"token":"LEAK"}"#,
        r"{'token':'secret LEAK'}",
        r#"{"token":"one \" LEAK"}"#,
        // Windows locations and the colon-separated prose they must not swallow.
        r"C:foo\bar\secret.txt",
        r"\Users\alice\file.txt",
        r"D:private\notes.md",
        r"\\server\share\alice\secret.txt",
        r"C:\Users\alice\file.txt",
        "C:secret.txt",
        r"C:foo\bar",
        r"D:notes\private.md",
        "C:.bashrc",
        r"C:.config\x",
        "C:.config/x",
        "C:.5",
        r#"{"tok\\u0065n":"LEAK"}"#,
        "A:todo",
        "note:something",
        "status:ok",
        "v:1.0",
        "level:info",
        "B:note",
        // Text that already carries the redactor's own placeholders. These are
        // the shapes that broke idempotence: a placeholder followed by ordinary
        // characters used to be reported as a redaction without changing the
        // text, so the value failed `RedactedText::new` on the way back in.
        "token:[redacted]-ok",
        "token: [redacted]-ok",
        r#"{"token":[redacted],"a":"ok"}"#,
        r#"{"token":[redacted]}"#,
        r#"{"a":1,"token":[redacted]MYSECRET}"#,
        "token:[redacted]",
        "[redacted]",
        "[redacted-url]",
        "[redacted-path]",
        r#"{"url":[redacted-url]}"#,
        // The typed placeholders and adjacent repeats. Each of these is a
        // separate way to write "a placeholder sits here", and the structured
        // scanner used to recognise only the shortest one — so the rest read as
        // fresh nested values whose bracket matching ended at the placeholder's
        // own `]`, releasing everything after it.
        r#"{"a":1,"token":[redacted-url]MYSECRET}"#,
        r#"{"a":1,"token":[redacted-path]MYSECRET}"#,
        r#"{"a":1,"token":[redacted][redacted]MYSECRET}"#,
        r#"{"a":1,"token":[redacted][redacted-url]MYSECRET}"#,
        r#"{"a":1,"token":[redacted-url][redacted-path][redacted]MYSECRET}"#,
        r#"{"token":[redacted][redacted]}"#,
        "token:[redacted][redacted]",
        "token:[redacted-url]",
        "token: [redacted-path]",
        "[redacted][redacted-url][redacted-path]",
    ];

    /// The redactor must be a projection: redacting already-redacted text is a
    /// no-op that reports nothing.
    ///
    /// This is not a stylistic property. `EventRedactor` builds `RedactedText`
    /// through `from_redactor`, which skips validation, but the `Deserialize`
    /// impl goes through `RedactedText::new` — and `new` rejects any text the
    /// redactor would still change or still count. A non-idempotent redactor
    /// therefore writes browser events that can never be read back, so retained
    /// evidence becomes permanently unreadable. Idempotence is what keeps the
    /// persisted form legible.
    #[test]
    fn redaction_is_idempotent_so_persisted_evidence_stays_readable() {
        for input in REDACTION_CORPUS {
            let (once, first_count) = redact_fragments(input);
            let (twice, second_count) = redact_fragments(&once);
            assert_eq!(
                twice, once,
                "second redaction pass changed the text: {input:?} -> {once:?} -> {twice:?}"
            );
            assert_eq!(
                second_count, 0,
                "second redaction pass reported {second_count} redaction(s) without changing \
                 {once:?} (from {input:?})"
            );
            assert!(
                RedactedText::new(once.clone(), false, first_count).is_ok(),
                "redactor output is rejected by its own validator: {input:?} -> {once:?}"
            );
        }
    }

    /// Truncation happens after redaction, so it can split a token the redactor
    /// judged whole and leave a fragment a later pass would judge differently.
    /// The emitted text still has to satisfy `RedactedText::new`, for the same
    /// deserialization reason as the property above.
    #[test]
    fn truncated_redactor_output_is_still_accepted_by_its_own_validator() {
        for input in REDACTION_CORPUS {
            let padded = format!("{}{input}", "x".repeat(64));
            for limit in 1..padded.len().min(96) {
                let redacted = EventRedactor.redact(&padded, limit);
                assert!(
                    RedactedText::new(
                        redacted.text().to_owned(),
                        redacted.truncated(),
                        redacted.redaction_count(),
                    )
                    .is_ok(),
                    "truncating to {limit} produced text its own validator rejects: \
                     {input:?} -> {:?}",
                    redacted.text()
                );
            }
        }
    }

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
    fn nested_structured_secrets_are_redacted_without_whitespace_separation() {
        // A compact structured fragment is one whitespace token, so the
        // single-separator scan only ever inspected the outermost key.
        let cases = [
            (r#"{"outer":{"token":"secret-value"}}"#, "secret-value"),
            (r#"{"a":{"b":{"password":"deep-secret"}}}"#, "deep-secret"),
            (r#"[{"apikey":"leaked-key"}]"#, "leaked-key"),
            (r#"{"ok":1,"secret":"tail-secret"}"#, "tail-secret"),
            (r#"{"cookie":"sid=abc123"}"#, "abc123"),
            (r#"{"authorization":"Bearer xyz.789"}"#, "xyz.789"),
        ];
        for (input, forbidden) in cases {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains(forbidden),
                "nested secret survived redaction: {input} -> {}",
                redacted.text()
            );
            assert!(
                redacted.redaction_count() > 0,
                "no redaction counted for {input}"
            );
        }
    }

    #[test]
    fn nested_secret_values_containing_whitespace_are_fully_redacted() {
        // Continuation used to redact exactly one following token, leaking the
        // remainder of a quoted value that contained spaces.
        for (input, forbidden) in [
            (
                r#"{"token":"secret value with spaces"}"#,
                ["secret", "value", "spaces"],
            ),
            (r#"{"password":"a b c"} trailing"#, ["a b c", "a b", "b c"]),
        ] {
            let redacted = EventRedactor.text(input);
            for needle in forbidden {
                assert!(
                    !redacted.text().contains(needle),
                    "{needle} leaked from {input} -> {}",
                    redacted.text()
                );
            }
            assert!(redacted.redaction_count() > 0);
        }
        // Text following the closing quote must survive.
        let redacted = EventRedactor.text(r#"{"password":"a b c"} trailing"#);
        assert!(
            redacted.text().contains("trailing"),
            "content after the secret was swallowed: {}",
            redacted.text()
        );
    }

    #[test]
    fn common_secret_key_aliases_are_recognized() {
        for input in [
            r#"{"access_token":"abc123"}"#,
            r#"{"refresh_token":"abc123"}"#,
            r#"{"client_secret":"abc123"}"#,
            r#"{"privateKey":"abc123"}"#,
            "access-token=abc123",
        ] {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains("abc123"),
                "alias key not recognized: {input} -> {}",
                redacted.text()
            );
            assert!(redacted.redaction_count() > 0, "no redaction for {input}");
        }
    }

    #[test]
    fn structured_tokens_without_nested_secrets_keep_url_and_path_redaction() {
        // The structured branch must not shadow the existing URL/path handling
        // when it has nothing of its own to redact.
        for input in [
            r#"{"url":"https://user:pass@example.test/private"}"#,
            r#"{"path":"/home/alice/private.txt"}"#,
        ] {
            let redacted = EventRedactor.text(input);
            assert!(redacted.redaction_count() > 0, "not redacted: {input}");
            for forbidden in ["private", "alice", "pass"] {
                assert!(
                    !redacted.text().contains(forbidden),
                    "{forbidden} survived in {input} -> {}",
                    redacted.text()
                );
            }
        }
    }

    #[test]
    fn windows_drive_relative_and_rooted_paths_are_redacted() {
        let cases = [
            (r"C:foo\bar\secret.txt", "secret"),
            (r"\Users\alice\file.txt", "alice"),
            (r"D:private\notes.md", "private"),
            (r"\\server\share\alice\secret.txt", "alice"),
            (r"C:\Users\alice\file.txt", "alice"),
        ];
        for (input, forbidden) in cases {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains(forbidden),
                "windows path leaked: {input} -> {}",
                redacted.text()
            );
            assert!(redacted.redaction_count() > 0, "no redaction for {input}");
        }
    }

    /// Five ways a secret used to survive the redactor. Each entry is a distinct
    /// escape route, so each is asserted independently rather than as a corpus:
    /// a nested value, an escaped key, a single-quoted value with spaces, and an
    /// escaped quote inside a value.
    #[test]
    fn secrets_do_not_escape_through_nesting_escaping_or_quoting() {
        let cases = [
            // A sensitive key whose value is an object: the key governs the whole
            // nested value, not just a scalar.
            (
                r#"{"outer":{"token":{"inner":"LEAK"}}}"#,
                "a nested object under a sensitive key",
            ),
            // The key is spelled with a unicode escape. Normalization has to
            // decode before it compares.
            (
                r#"{"tok\u0065n":"LEAK"}"#,
                "a sensitive key spelled with a unicode escape",
            ),
            // The escape is itself escaped. One decoding pass leaves a residual
            // escape that reads as literal letters, so normalization has to
            // decode to a fixed point rather than exactly once.
            (
                r#"{"tok\\u0065n":"LEAK"}"#,
                "a sensitive key spelled with a doubly escaped unicode escape",
            ),
            // Single quotes are legal in console output, and the value contains a
            // space, so the continuation scan must recognise them.
            (r"{'token':'secret LEAK'}", "a single-quoted spaced value"),
            // The escaped quote is not the terminator; treating it as one
            // releases the rest of the value.
            (r#"{"token":"one \" LEAK"}"#, "an escaped quote in a value"),
        ];
        for (input, description) in cases {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains("LEAK"),
                "{description} leaked: {input} -> {}",
                redacted.text()
            );
            assert!(redacted.redaction_count() > 0, "no redaction for {input}");
        }
    }

    /// A literal placeholder written into the input must not buy the text after
    /// it a free pass.
    ///
    /// The structured scanner has to leave already-redacted text alone, so it
    /// recognises a placeholder where a nested value would start. Recognising
    /// only `[redacted]` made the other two typed placeholders — and any
    /// adjacent repeat — read as ordinary nested values instead, and because
    /// that branch matches brackets, the placeholder's own `]` closed the value
    /// it thought it was consuming and released the rest verbatim.
    #[test]
    fn typed_and_repeated_placeholders_cannot_smuggle_a_secret_through() {
        for input in [
            r#"{"a":1,"token":[redacted-url]MYSECRET}"#,
            r#"{"a":1,"token":[redacted-path]MYSECRET}"#,
            r#"{"a":1,"token":[redacted][redacted]MYSECRET}"#,
            r#"{"a":1,"token":[redacted][redacted-url]MYSECRET}"#,
            r#"{"a":1,"token":[redacted-url][redacted-path][redacted]MYSECRET}"#,
            r#"{"a":1,"token":[redacted]MYSECRET}"#,
        ] {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains("MYSECRET"),
                "a placeholder prefix smuggled the value after it through: {input} -> {}",
                redacted.text()
            );
            assert!(
                redacted.redaction_count() > 0,
                "the smuggled value was dropped without being counted: {input}"
            );
        }
    }

    /// Windows drive-relative paths and ordinary colon-separated prose share a
    /// prefix shape, so the discriminator has to be checked in both directions.
    /// Redacting `A:todo` is not a safe default: it teaches readers that
    /// `[redacted-path]` means nothing.
    #[test]
    fn windows_drive_paths_redact_without_swallowing_colon_separated_prose() {
        for (input, forbidden) in [
            ("C:secret.txt", "secret"),
            (r"C:foo\bar", "foo"),
            (r"D:notes\private.md", "private"),
            // Dotfiles have no stem, so the filename-extension discriminator
            // alone missed them and a drive-relative path to a user's shell
            // configuration was emitted verbatim.
            ("C:.bashrc", "bashrc"),
            (r"C:.config\x", "config"),
            ("C:.config/x", "config"),
        ] {
            let redacted = EventRedactor.text(input);
            assert!(
                !redacted.text().contains(forbidden),
                "windows path leaked: {input} -> {}",
                redacted.text()
            );
            assert!(redacted.redaction_count() > 0, "no redaction for {input}");
        }
        for input in ["A:todo", "note:something", "status:ok", "v:1.0", "C:.5"] {
            let redacted = EventRedactor.text(input);
            assert_eq!(
                redacted.text(),
                input,
                "ordinary text was redacted as a path: {input}"
            );
            assert_eq!(redacted.redaction_count(), 0);
        }
    }

    #[test]
    fn ordinary_key_value_text_is_not_mistaken_for_a_windows_path() {
        // The single-letter drive constraint keeps ordinary prose out of the
        // path branch; only `X:` prefixes are treated as drive designators.
        // `A:todo` is prose, not a drive-relative path: a bare `X:` prefix only
        // counts when a path separator follows or nothing does.
        for input in [
            "note:something",
            "status:ok",
            "level:info",
            "A:todo",
            "B:note",
        ] {
            let redacted = EventRedactor.text(input);
            assert_eq!(
                redacted.text(),
                input,
                "ordinary text was redacted as a path: {input}"
            );
            assert_eq!(redacted.redaction_count(), 0);
        }
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
        let encoded = serde_json::to_value(&network).unwrap();
        let digest = encoded["path_sha256"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
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

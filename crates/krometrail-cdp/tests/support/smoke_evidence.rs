#![allow(dead_code)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use krometrail_cdp::CaptureConfig;

pub const SCHEMA_VERSION: u32 = 1;
pub const KIND: &str = "cross_platform_capture_smoke";
pub const CDPKIT_VERSION: &str = "0.4.0";
pub const FIXTURE_NAME: &str = "cdp-transport-gate";
pub const FIXTURE_RELATIVE_PATH: &str = "tests/fixtures/browser/cdp-transport-gate";

/// Schema-valid, canonical sample evidence used by the deterministic round-trip test.
///
/// All values are fixed; no host-derived ordering or temp paths leak into the committed sample.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossPlatformSmokeEvidence {
    pub schema_version: u32,
    pub kind: String,
    pub provenance: Provenance,
    pub sessions: Vec<Session>,
    pub shutdown: Shutdown,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub krometrail_revision: String,
    pub rust_version: String,
    pub cdpkit_version: String,
    pub platform: String,
    pub architecture: String,
    pub configuration_name: String,
    pub browser_installation: BrowserInstallationEvidence,
    pub runtime_version: RuntimeVersion,
    pub launch: Launch,
    pub capture_config: CaptureConfigSnapshot,
    pub fixture: FixtureEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserInstallationEvidence {
    pub executable_source: String,
    pub product: String,
    pub discovered_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeVersion {
    pub product: String,
    pub product_version: String,
    pub revision: String,
    pub protocol_version: String,
    pub user_agent: String,
    pub js_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    pub ownership: String,
    pub profile_kind: String,
    pub endpoint: String,
    pub wrapper_variant: String,
    pub force_device_scale_factor: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfigSnapshot {
    pub format: String,
    pub jpeg_quality: u8,
    pub max_dimensions: Option<()>,
    pub max_active_streams: usize,
    pub queue_capacity: usize,
    pub max_base64_payload_bytes: usize,
    pub gap_ledger_capacity: usize,
    pub ack_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
}

impl From<&CaptureConfig> for CaptureConfigSnapshot {
    fn from(config: &CaptureConfig) -> Self {
        Self {
            format: "jpeg".into(),
            jpeg_quality: config.jpeg_quality.expect("JPEG quality is present"),
            max_dimensions: None,
            max_active_streams: config.max_active_streams.get(),
            queue_capacity: config.queue_capacity.get(),
            max_base64_payload_bytes: config.max_base64_payload_bytes.get(),
            gap_ledger_capacity: config.gap_ledger_capacity.get(),
            ack_timeout_ms: config.ack_timeout.as_millis() as u64,
            shutdown_timeout_ms: config.shutdown_timeout.as_millis() as u64,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureEvidence {
    pub name: String,
    pub path: String,
    pub index_html_sha256: String,
    pub animation_js_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub name: String,
    pub capture_config: CaptureConfigSnapshot,
    pub frame_count: u64,
    pub source_time_samples: u64,
    pub image_dimensions: Dimensions,
    pub viewport: Dimensions,
    pub device_scale_factor: f64,
    pub capture_ordinal_range: OrdinalRange,
    pub observed_clock_span_nanos: u64,
    pub session_clock_span_nanos: u64,
    pub ack_latency_nanos: TimingSummary,
    pub frame_cadence_nanos: TimingSummary,
    pub declared_gaps: Vec<DeclaredGap>,
    pub visibility_events: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinalRange {
    pub min: u64,
    pub max: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingSummary {
    pub samples: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredGap {
    pub reason: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shutdown {
    pub outcome: String,
    pub flush_count: u64,
    pub process_references_after: Vec<String>,
    pub profile_references_after: Vec<String>,
}

impl CrossPlatformSmokeEvidence {
    /// A hand-valid, canonical sample whose bytes are pinned by the round-trip test.
    pub fn sample() -> Self {
        let capture_config = CaptureConfigSnapshot::from(&CaptureConfig::default());
        let fidelity_session = Session {
            name: "fidelity".into(),
            capture_config: capture_config.clone(),
            frame_count: 30,
            source_time_samples: 30,
            image_dimensions: Dimensions {
                width: 780,
                height: 437,
            },
            viewport: Dimensions {
                width: 780,
                height: 437,
            },
            device_scale_factor: 1.0,
            capture_ordinal_range: OrdinalRange { min: 1, max: 30 },
            observed_clock_span_nanos: 1_234_567_890,
            session_clock_span_nanos: 1_234_567_000,
            ack_latency_nanos: TimingSummary {
                samples: 30,
                p50: Some(1_500_000),
                p95: Some(3_000_000),
                p99: Some(5_000_000),
                max: Some(4_500_000),
            },
            frame_cadence_nanos: TimingSummary {
                samples: 29,
                p50: Some(33_000_000),
                p95: Some(34_000_000),
                p99: Some(35_000_000),
                max: Some(36_000_000),
            },
            declared_gaps: Vec::new(),
            visibility_events: 0,
        };
        let mut loss_config = capture_config.clone();
        loss_config.queue_capacity = 1;
        let loss_session = Session {
            name: "loss_reporting".into(),
            capture_config: loss_config,
            frame_count: 12,
            source_time_samples: 12,
            image_dimensions: Dimensions {
                width: 780,
                height: 437,
            },
            viewport: Dimensions {
                width: 780,
                height: 437,
            },
            device_scale_factor: 1.0,
            capture_ordinal_range: OrdinalRange { min: 1, max: 12 },
            observed_clock_span_nanos: 500_000_000,
            session_clock_span_nanos: 499_000_000,
            ack_latency_nanos: TimingSummary {
                samples: 12,
                p50: Some(1_000_000),
                p95: Some(2_000_000),
                p99: Some(3_000_000),
                max: Some(2_500_000),
            },
            frame_cadence_nanos: TimingSummary {
                samples: 11,
                p50: Some(33_000_000),
                p95: Some(34_000_000),
                p99: Some(35_000_000),
                max: Some(36_000_000),
            },
            declared_gaps: vec![DeclaredGap {
                reason: "ingestion_queue_saturated".into(),
                count: 1,
            }],
            visibility_events: 0,
        };
        Self {
            schema_version: SCHEMA_VERSION,
            kind: KIND.into(),
            provenance: Provenance {
                krometrail_revision: "0000000000000000000000000000000000000000".into(),
                rust_version: "1.85.0".into(),
                cdpkit_version: CDPKIT_VERSION.into(),
                platform: "linux".into(),
                architecture: "x86_64".into(),
                configuration_name: "linux-chrome".into(),
                browser_installation: BrowserInstallationEvidence {
                    executable_source: "platform_default".into(),
                    product: "chrome".into(),
                    discovered_version: "Google Chrome 128.0.0.0".into(),
                },
                runtime_version: RuntimeVersion {
                    product: "chrome".into(),
                    product_version: "128.0.0.0".into(),
                    revision: "@abcdef1234567890abcdef1234567890abcdef12".into(),
                    protocol_version: "1.3".into(),
                    user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36".into(),
                    js_version: "12.8".into(),
                },
                launch: Launch {
                    ownership: "managed".into(),
                    profile_kind: "temporary".into(),
                    endpoint: "loopback".into(),
                    wrapper_variant: "default_dpi".into(),
                    force_device_scale_factor: 1.0,
                },
                capture_config: capture_config.clone(),
                fixture: FixtureEvidence {
                    name: FIXTURE_NAME.into(),
                    path: FIXTURE_RELATIVE_PATH.into(),
                    index_html_sha256:
                        "sha256:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68"
                            .into(),
                    animation_js_sha256:
                        "sha256:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13"
                            .into(),
                },
            },
            sessions: vec![fidelity_session, loss_session],
            shutdown: Shutdown {
                outcome: "managed_browser_closed".into(),
                flush_count: 1,
                process_references_after: Vec::new(),
                profile_references_after: Vec::new(),
            },
            non_claims: vec![
                "no transport requalification (final5 owns cdpkit selection)".into(),
                "no host-speed percentile threshold (ack/cadence are diagnostics)".into(),
                "no product-thesis capture-probability threshold".into(),
                "no duration sweep, defect corpus, artifact comparison, or storage validation"
                    .into(),
                "no chrome-acknowledgement-token continuity claim".into(),
            ],
        }
    }

    /// Serialize to canonical evidence bytes: struct fields in schema order, `BTreeMap`-sorted maps,
    /// canonical session order, recursive key sort, and pretty JSON.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let value = serde_json::to_value(self)?;
        let canonical = canonicalize_value(value);
        serde_json::to_vec_pretty(&canonical)
    }

    /// Validate that the evidence satisfies the schema invariants enforced by the serializer.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("schema_version must be 1".into());
        }
        if self.kind != KIND {
            return Err("kind must be cross_platform_capture_smoke".into());
        }
        if self.non_claims.is_empty() {
            return Err("non_claims must not be empty".into());
        }
        if self.provenance.cdpkit_version != CDPKIT_VERSION {
            return Err("cdpkit_version must be 0.4.0".into());
        }
        if !matches!(self.provenance.platform.as_str(), "linux" | "macos") {
            return Err("platform must be linux or macos".into());
        }
        if !matches!(self.provenance.architecture.as_str(), "x86_64" | "aarch64") {
            return Err("architecture must be x86_64 or aarch64".into());
        }
        if !matches!(
            self.provenance.configuration_name.as_str(),
            "linux-chrome"
                | "linux-chromium"
                | "macos-chrome-default-dpi"
                | "macos-chrome-high-dpi"
        ) {
            return Err("configuration_name is not recognized".into());
        }
        let expected_force = match self.provenance.launch.wrapper_variant.as_str() {
            "default_dpi" => 1.0,
            "high_dpi" => 2.0,
            _ => return Err("wrapper_variant must be default_dpi or high_dpi".into()),
        };
        if (self.provenance.launch.force_device_scale_factor - expected_force).abs() > f64::EPSILON
        {
            return Err("force_device_scale_factor does not match wrapper_variant".into());
        }
        self.validate_capture_config(&self.provenance.capture_config, 4)?;
        for (index, session) in self.sessions.iter().enumerate() {
            if !matches!(session.name.as_str(), "fidelity" | "loss_reporting") {
                return Err(format!("session {index} has an unrecognized name"));
            }
            self.validate_capture_config(&session.capture_config, session.queue_capacity_hint())?;
            self.validate_timing_summary(&session.ack_latency_nanos)?;
            self.validate_timing_summary(&session.frame_cadence_nanos)?;
            if session.capture_ordinal_range.min > session.capture_ordinal_range.max {
                return Err(format!("session {index} ordinal range min exceeds max"));
            }
        }
        if self
            .sessions
            .iter()
            .filter(|session| session.name == "fidelity")
            .count()
            != 1
        {
            return Err("exactly one fidelity session is required".into());
        }
        let has_loss = self
            .sessions
            .iter()
            .any(|session| session.name == "loss_reporting");
        if !has_loss {
            return Err("at least one loss_reporting session is required".into());
        }
        if self.shutdown.outcome.is_empty() {
            return Err("shutdown.outcome must not be empty".into());
        }
        sanitize(self)?;
        Ok(())
    }

    fn validate_capture_config(
        &self,
        config: &CaptureConfigSnapshot,
        expected_queue_capacity: usize,
    ) -> Result<(), String> {
        if config.format != "jpeg" {
            return Err("capture_config.format must be jpeg".into());
        }
        if config.jpeg_quality != 80 {
            return Err("capture_config.jpeg_quality must be 80".into());
        }
        if config.max_dimensions.is_some() {
            return Err("capture_config.max_dimensions must be null".into());
        }
        if config.max_active_streams != 8 {
            return Err("capture_config.max_active_streams must be 8".into());
        }
        if config.queue_capacity != expected_queue_capacity {
            return Err(format!(
                "capture_config.queue_capacity must be {expected_queue_capacity}"
            ));
        }
        if config.max_base64_payload_bytes != 8_388_608 {
            return Err("capture_config.max_base64_payload_bytes must be 8388608".into());
        }
        if config.gap_ledger_capacity != 64 {
            return Err("capture_config.gap_ledger_capacity must be 64".into());
        }
        if config.ack_timeout_ms != 250 {
            return Err("capture_config.ack_timeout_ms must be 250".into());
        }
        if config.shutdown_timeout_ms != 5_000 {
            return Err("capture_config.shutdown_timeout_ms must be 5000".into());
        }
        Ok(())
    }

    fn validate_timing_summary(&self, summary: &TimingSummary) -> Result<(), String> {
        let percentiles = [summary.p50, summary.p95, summary.p99, summary.max];
        let all_null = percentiles.iter().all(|value| value.is_none());
        let all_present = percentiles.iter().all(|value| value.is_some());
        if summary.samples == 0 {
            if !all_null {
                return Err("percentiles must be null when samples is 0".into());
            }
        } else if !all_present {
            return Err("percentiles must be present when samples is non-zero".into());
        }
        if let (Some(p50), Some(p95), Some(p99)) = (summary.p50, summary.p95, summary.p99) {
            if p50 > p95 || p95 > p99 {
                return Err("percentiles must satisfy p50 <= p95 <= p99".into());
            }
        }
        Ok(())
    }
}

impl Session {
    fn queue_capacity_hint(&self) -> usize {
        match self.name.as_str() {
            "fidelity" => 4,
            "loss_reporting" => 1,
            _ => 4,
        }
    }
}

/// Recursively sort object keys so evidence bytes are host-independent.
fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(values) => {
            let mut sorted = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize_value(value)))
                .collect::<Vec<_>>();
            sorted.sort_by(|left, right| left.0.cmp(&right.0));
            let mut object = serde_json::Map::new();
            for (key, value) in sorted {
                object.insert(key, value);
            }
            Value::Object(object)
        }
        other => other,
    }
}

/// Reject host-derived material that must never be committed.
pub fn sanitize(evidence: &CrossPlatformSmokeEvidence) -> Result<(), String> {
    let encoded = serde_json::to_value(evidence)
        .map_err(|error| format!("cannot encode evidence for sanitization: {error}"))?;
    if walk(&encoded) {
        return Err("evidence contains a host path, endpoint, frame payload, profile path, or raw adapter error".into());
    }
    Ok(())
}

fn walk(value: &Value) -> bool {
    match value {
        Value::String(text) => contains_machine_detail(text),
        Value::Array(values) => values.iter().any(walk),
        Value::Object(values) => values.iter().any(|(_key, value)| walk(value)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn contains_machine_detail(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "http://",
        "https://",
        "ws://",
        "wss://",
        "file://",
        "--remote-debugging-port",
        "--user-data-dir",
        "/tmp/krometrail-real-",
        "/var/folders/",
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || lower.starts_with("/home/")
        || lower.starts_with("/users/")
        || lower.starts_with("/private/")
        || lower.starts_with("/root/")
        || lower.starts_with("/workspace/")
        || lower.starts_with("/build/")
        || lower.contains("\\")
        || lower.contains("c:\\")
        || lower.contains("c:/")
}

pub fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/cross-platform-smoke/v1/schema.json")
}

pub fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/cross-platform-smoke/v1/sample.json")
}

pub fn load_schema() -> Value {
    let bytes = std::fs::read(schema_path()).expect("schema.json exists");
    serde_json::from_slice(&bytes).expect("schema.json is valid JSON")
}

pub fn load_sample() -> CrossPlatformSmokeEvidence {
    let bytes = std::fs::read(sample_path()).expect("sample.json exists");
    serde_json::from_slice(&bytes).expect("sample.json is valid evidence")
}

/// A minimal Draft 2020-12 validator that understands exactly the keywords used by the smoke
/// evidence schema. Keeping it local avoids adding a fragile runtime dependency.
pub fn validate_against_schema(value: &Value, schema: &Value) -> Result<(), String> {
    validate_node(value, schema, "")
}

fn validate_node(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    if let Some(types) = schema.get("type") {
        if !matches_type(value, types) {
            return Err(format!(
                "{path}: expected type {types}, got {}",
                value_type(value)
            ));
        }
    }

    if let Some(constant) = schema.get("const") {
        if value != constant {
            return Err(format!("{path}: expected const {constant}"));
        }
    }

    if let Some(enumeration) = schema.get("enum") {
        let candidates = enumeration.as_array().ok_or("enum must be an array")?;
        if !candidates.contains(value) {
            return Err(format!("{path}: value {value} is not in enum"));
        }
    }

    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
        let number = value.as_f64().ok_or(format!("{path}: expected a number"))?;
        if number < minimum {
            return Err(format!("{path}: {number} is below minimum {minimum}"));
        }
    }

    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
        let number = value.as_f64().ok_or(format!("{path}: expected a number"))?;
        if number > maximum {
            return Err(format!("{path}: {number} is above maximum {maximum}"));
        }
    }

    if let Some(min_length) = schema.get("minLength").and_then(Value::as_u64) {
        let text = value.as_str().ok_or(format!("{path}: expected a string"))?;
        if (text.len() as u64) < min_length {
            return Err(format!("{path}: string is shorter than {min_length}"));
        }
    }

    if let Some(max_length) = schema.get("maxLength").and_then(Value::as_u64) {
        let text = value.as_str().ok_or(format!("{path}: expected a string"))?;
        if (text.len() as u64) > max_length {
            return Err(format!("{path}: string is longer than {max_length}"));
        }
    }

    if let Some(min_items) = schema.get("minItems").and_then(Value::as_u64) {
        let items = value
            .as_array()
            .ok_or(format!("{path}: expected an array"))?;
        if (items.len() as u64) < min_items {
            return Err(format!("{path}: array has fewer than {min_items} items"));
        }
    }

    if let Some(max_items) = schema.get("maxItems").and_then(Value::as_u64) {
        let items = value
            .as_array()
            .ok_or(format!("{path}: expected an array"))?;
        if (items.len() as u64) > max_items {
            return Err(format!("{path}: array has more than {max_items} items"));
        }
    }

    if let Some(properties) = schema.get("properties") {
        let object = value
            .as_object()
            .ok_or(format!("{path}: expected an object"))?;
        let props = properties
            .as_object()
            .ok_or("properties must be an object")?;
        for (key, subschema) in props {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            if let Some(child) = object.get(key) {
                validate_node(child, subschema, &child_path)?;
            } else if schema
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.contains(&Value::String(key.clone())))
            {
                return Err(format!("{child_path}: required property is missing"));
            }
        }

        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            let allowed: std::collections::HashSet<_> = props.keys().collect();
            for key in object.keys() {
                if !allowed.contains(key) {
                    return Err(format!(
                        "{path}: additional property '{key}' is not allowed"
                    ));
                }
            }
        }
    }

    if let Some(items_schema) = schema.get("items") {
        let items = value
            .as_array()
            .ok_or(format!("{path}: expected an array"))?;
        for (index, item) in items.iter().enumerate() {
            let item_path = format!("{path}[{index}]");
            validate_node(item, items_schema, &item_path)?;
        }
    }

    Ok(())
}

fn matches_type(value: &Value, types: &Value) -> bool {
    let expected = match types {
        Value::String(name) => vec![name.as_str()],
        Value::Array(names) => names
            .iter()
            .filter_map(|name| name.as_str())
            .collect::<Vec<_>>(),
        _ => return true,
    };
    expected.iter().any(|expected| match *expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_u64().is_some() || value.as_i64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    })
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_validates_against_schema() {
        let schema = load_schema();
        let sample = serde_json::to_value(CrossPlatformSmokeEvidence::sample()).unwrap();
        validate_against_schema(&sample, &schema).expect("sample validates against schema");
    }
}

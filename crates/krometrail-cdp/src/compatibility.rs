//! Capability-based compatibility probing for Chrome-family renderer targets.
//!
//! `RENDERER_CAPABILITY_PROBES` is the single registry for the required capability id, scope,
//! command, and response decoder. The result, logging, and missing-capability validation all
//! derive from this registry. Product branding is status metadata, never an acceptance shortcut.

use std::collections::HashMap;

use krometrail_core::{
    BrowserCompatibility, BrowserProduct, BrowserProductVersion, BrowserVersion, CapabilitySupport,
    NonEmptyText, RendererCapability,
};
use serde_json::Value;
use thiserror::Error;

use crate::transport::{CdpTransport, CommandScope, TransportError, TransportSessionId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointKind {
    Chrome,
    Chromium,
    ElectronRenderer,
    NodeInspector,
    Other,
}

impl EndpointKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::ElectronRenderer => "electron_renderer",
            Self::NodeInspector => "node_inspector",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeScope {
    Browser,
    Renderer,
}

pub struct RendererCapabilityProbe {
    pub id: &'static str,
    pub capability: RendererCapability,
    pub required: bool,
    pub scope: ProbeScope,
    pub command: &'static str,
    pub decode: fn(&Value) -> bool,
}

pub const RENDERER_CAPABILITY_PROBES: &[RendererCapabilityProbe] = &[
    RendererCapabilityProbe {
        id: "browser_identity",
        capability: RendererCapability::BrowserIdentity,
        required: true,
        scope: ProbeScope::Browser,
        command: "Browser.getVersion",
        decode: object_response,
    },
    RendererCapabilityProbe {
        id: "target_discovery",
        capability: RendererCapability::TargetDiscovery,
        required: true,
        scope: ProbeScope::Browser,
        command: "Target.getTargets",
        decode: target_list_response,
    },
    RendererCapabilityProbe {
        id: "flat_target_sessions",
        capability: RendererCapability::FlatTargetSessions,
        required: true,
        scope: ProbeScope::Browser,
        command: "Target.attachToTarget",
        decode: session_response,
    },
    RendererCapabilityProbe {
        id: "page",
        capability: RendererCapability::Page,
        required: true,
        scope: ProbeScope::Renderer,
        command: "Page.enable",
        decode: any_response,
    },
    RendererCapabilityProbe {
        id: "runtime",
        capability: RendererCapability::Runtime,
        required: true,
        scope: ProbeScope::Renderer,
        command: "Runtime.evaluate",
        decode: object_response,
    },
    RendererCapabilityProbe {
        id: "accessibility",
        capability: RendererCapability::Accessibility,
        required: true,
        scope: ProbeScope::Renderer,
        command: "Accessibility.getFullAXTree",
        decode: any_response,
    },
    RendererCapabilityProbe {
        id: "input",
        capability: RendererCapability::Input,
        required: true,
        scope: ProbeScope::Renderer,
        command: "Input.dispatchMouseEvent",
        decode: any_response,
    },
    RendererCapabilityProbe {
        id: "screencast",
        capability: RendererCapability::Screencast,
        required: true,
        scope: ProbeScope::Browser,
        command: "Schema.getDomains",
        decode: screencast_schema_response,
    },
];

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum CompatibilityProbeError {
    #[error("browser compatibility probe lost its transport")]
    Transport(TransportError),
    #[error("browser compatibility requires a recordable renderer page")]
    NoRecordablePage,
    #[error("Electron Node inspector is not a renderer endpoint")]
    NodeInspectorOnly,
    #[error("browser identity is incomplete")]
    InvalidIdentity,
    #[error("required renderer capabilities are unavailable")]
    MissingCapabilities(Vec<RendererCapability>),
}

impl CompatibilityProbeError {
    pub fn missing(&self) -> &[RendererCapability] {
        match self {
            Self::MissingCapabilities(values) => values,
            _ => &[],
        }
    }

    pub fn to_core_error(&self) -> krometrail_core::KrometrailError {
        krometrail_core::KrometrailError::from_browser_failure(
            krometrail_core::ErrorCode::BrowserCompatibilityFailed,
            NonEmptyText::new("browser does not provide the required renderer capabilities")
                .expect("static compatibility message is non-empty"),
        )
    }
}

/// Probe one browser connection. The probe never starts a screencast; its schema query only
/// verifies that the Page domain advertises the required capture command/event surface.
pub async fn probe_compatibility(
    transport: &dyn CdpTransport,
) -> Result<BrowserCompatibility, CompatibilityProbeError> {
    let version_value = command(
        transport,
        &CommandScope::Browser,
        "Browser.getVersion",
        Value::Object(Default::default()),
    )
    .await?;
    let identity =
        parse_identity(&version_value).ok_or(CompatibilityProbeError::InvalidIdentity)?;
    let endpoint_kind = identity.endpoint_kind;
    let mut availability = HashMap::new();
    availability.insert(RendererCapability::BrowserIdentity, true);
    if endpoint_kind == EndpointKind::NodeInspector {
        trace_probe(
            &identity,
            endpoint_kind,
            &availability,
            &[RendererCapability::BrowserIdentity],
        );
        return Err(CompatibilityProbeError::NodeInspectorOnly);
    }

    // cdpkit subscriptions are installed before discovery is enabled. They expose named params,
    // not wildcard envelopes, which is the honest boundary this probe relies on.
    let subscriptions = ["Target.targetCreated", "Target.attachedToTarget"];
    let subscriptions_ok = futures_util::future::try_join_all(
        subscriptions
            .iter()
            .map(|name| transport.subscribe_named(&CommandScope::Browser, name)),
    )
    .await
    .is_ok();
    if !subscriptions_ok {
        availability.insert(RendererCapability::TargetDiscovery, false);
    }

    let discovery = command(
        transport,
        &CommandScope::Browser,
        "Target.setDiscoverTargets",
        serde_json::json!({"discover": true}),
    )
    .await;
    let auto_attach = command(
        transport,
        &CommandScope::Browser,
        "Target.setAutoAttach",
        serde_json::json!({"autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true}),
    )
    .await;
    let targets = command(
        transport,
        &CommandScope::Browser,
        "Target.getTargets",
        Value::Object(Default::default()),
    )
    .await;
    let target_value = targets
        .as_ref()
        .ok()
        .filter(|value| target_list_response(value));
    availability.insert(
        RendererCapability::TargetDiscovery,
        subscriptions_ok && discovery.is_ok() && target_value.is_some(),
    );
    availability.insert(
        RendererCapability::FlatTargetSessions,
        subscriptions_ok && auto_attach.is_ok(),
    );

    let page_target = target_value.and_then(|value| {
        value
            .get("targetInfos")
            .and_then(Value::as_array)
            .and_then(|targets| {
                targets.iter().find(|target| {
                    target.get("type").and_then(Value::as_str) == Some("page")
                        && target
                            .get("url")
                            .and_then(Value::as_str)
                            .is_some_and(|url| !url.is_empty())
                        && !target
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .starts_with("devtools://")
                })
            })
            .and_then(|target| target.get("targetId").and_then(Value::as_str))
    });
    let session_id = if let Some(target_id) = page_target {
        match command(
            transport,
            &CommandScope::Browser,
            "Target.attachToTarget",
            serde_json::json!({"targetId": target_id, "flatten": true}),
        )
        .await
        {
            Ok(value) => value
                .get("sessionId")
                .and_then(Value::as_str)
                .and_then(|value| TransportSessionId::new(value.to_owned()).ok()),
            Err(_) => None,
        }
    } else {
        None
    };
    availability.insert(
        RendererCapability::FlatTargetSessions,
        availability[&RendererCapability::FlatTargetSessions] && session_id.is_some(),
    );

    let session = session_id
        .as_ref()
        .map(|session| CommandScope::Session(session.clone()));
    if let Some(session) = session {
        availability.insert(
            RendererCapability::Page,
            probe_command(
                transport,
                &session,
                "Page.enable",
                Value::Object(Default::default()),
                any_response,
            )
            .await,
        );
        availability.insert(
            RendererCapability::Runtime,
            probe_command(
                transport,
                &session,
                "Runtime.evaluate",
                serde_json::json!({"expression": "1 + 1", "returnByValue": true}),
                object_response,
            )
            .await,
        );
        availability.insert(
            RendererCapability::Accessibility,
            probe_command(
                transport,
                &session,
                "Accessibility.enable",
                Value::Object(Default::default()),
                any_response,
            )
            .await
                && probe_command(
                    transport,
                    &session,
                    "Accessibility.getFullAXTree",
                    Value::Object(Default::default()),
                    any_response,
                )
                .await,
        );
        availability.insert(
            RendererCapability::Input,
            probe_command(
                transport,
                &session,
                "Input.dispatchMouseEvent",
                serde_json::json!({"type": "mouseMoved", "x": 1, "y": 1}),
                any_response,
            )
            .await,
        );
    } else {
        for capability in [
            RendererCapability::Page,
            RendererCapability::Runtime,
            RendererCapability::Accessibility,
            RendererCapability::Input,
        ] {
            availability.insert(capability, false);
        }
    }
    let schema = command(
        transport,
        &CommandScope::Browser,
        "Schema.getDomains",
        Value::Object(Default::default()),
    )
    .await;
    availability.insert(
        RendererCapability::Screencast,
        schema.as_ref().is_ok_and(screencast_schema_response),
    );

    let missing = RENDERER_CAPABILITY_PROBES
        .iter()
        .filter(|probe| {
            probe.required
                && !availability
                    .get(&probe.capability)
                    .copied()
                    .unwrap_or(false)
        })
        .map(|probe| probe.capability)
        .collect::<Vec<_>>();
    trace_probe(&identity, endpoint_kind, &availability, &missing);
    if !missing.is_empty() {
        if page_target.is_none() {
            return Err(CompatibilityProbeError::NoRecordablePage);
        }
        return Err(CompatibilityProbeError::MissingCapabilities(missing));
    }
    let capabilities = RENDERER_CAPABILITY_PROBES
        .iter()
        .map(|probe| {
            let available = availability[&probe.capability];
            CapabilitySupport::new(
                probe.capability,
                available,
                probe.required,
                (!available)
                    .then(|| NonEmptyText::new("probe unavailable").expect("static detail")),
            )
            .expect("registry capability values are valid")
        })
        .collect();
    BrowserCompatibility::new(identity.version, capabilities)
        .map_err(|_| CompatibilityProbeError::InvalidIdentity)
}

struct ParsedIdentity {
    version: BrowserVersion,
    endpoint_kind: EndpointKind,
}

fn parse_identity(value: &Value) -> Option<ParsedIdentity> {
    let product = value.get("product")?.as_str()?;
    let product_version = nonempty(product.split_once('/')?.1)?;
    let revision = nonempty(value.get("revision")?.as_str()?)?;
    let protocol_version = nonempty(value.get("protocolVersion")?.as_str()?)?;
    let user_agent = nonempty(value.get("userAgent")?.as_str()?)?;
    let js_version = nonempty(value.get("jsVersion")?.as_str()?)?;
    let product_kind = classify_product(product, &user_agent);
    let endpoint_kind = classify_endpoint(product, &user_agent);
    let version = BrowserVersion::new(
        product_kind,
        BrowserProductVersion::new(product_version).ok()?,
        revision,
        protocol_version,
        user_agent,
        js_version,
    )
    .ok()?;
    Some(ParsedIdentity {
        version,
        endpoint_kind,
    })
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn classify_product(product: &str, user_agent: &str) -> BrowserProduct {
    if product.starts_with("Electron") || user_agent.contains("Electron/") {
        BrowserProduct::ElectronRenderer
    } else if product.starts_with("Chrome/") {
        BrowserProduct::Chrome
    } else if product.starts_with("Chromium/") {
        BrowserProduct::Chromium
    } else {
        BrowserProduct::OtherChromium
    }
}

fn classify_endpoint(product: &str, user_agent: &str) -> EndpointKind {
    if product.starts_with("Node.js") || product.starts_with("node/") {
        EndpointKind::NodeInspector
    } else {
        match classify_product(product, user_agent) {
            BrowserProduct::Chrome => EndpointKind::Chrome,
            BrowserProduct::Chromium => EndpointKind::Chromium,
            BrowserProduct::ElectronRenderer => EndpointKind::ElectronRenderer,
            BrowserProduct::OtherChromium => EndpointKind::Other,
        }
    }
}

async fn command(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    method: &str,
    params: Value,
) -> Result<Value, CompatibilityProbeError> {
    transport
        .send_raw(scope, method, params)
        .await
        .map_err(|error| {
            if error.is_retryable() {
                CompatibilityProbeError::Transport(error)
            } else {
                CompatibilityProbeError::MissingCapabilities(Vec::new())
            }
        })
}

async fn probe_command(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    method: &str,
    params: Value,
    decoder: fn(&Value) -> bool,
) -> bool {
    match transport.send_raw(scope, method, params).await {
        Ok(value) => decoder(&value),
        Err(error) if error.is_retryable() => false,
        Err(_) => false,
    }
}

fn object_response(value: &Value) -> bool {
    value.is_object()
}

fn any_response(_value: &Value) -> bool {
    true
}

fn target_list_response(value: &Value) -> bool {
    value.get("targetInfos").is_some_and(Value::is_array)
}

fn session_response(value: &Value) -> bool {
    value.get("sessionId").is_some_and(Value::is_string)
}

fn screencast_schema_response(value: &Value) -> bool {
    value
        .get("domains")
        .and_then(Value::as_array)
        .is_some_and(|domains| {
            domains.iter().any(|domain| {
                domain.get("name").and_then(Value::as_str) == Some("Page")
                    && domain
                        .get("commands")
                        .and_then(Value::as_array)
                        .is_some_and(|commands| {
                            commands.iter().any(|command| {
                                command.get("name").and_then(Value::as_str)
                                    == Some("startScreencast")
                            })
                        })
            })
        })
}

fn trace_probe(
    identity: &ParsedIdentity,
    endpoint_kind: EndpointKind,
    availability: &HashMap<RendererCapability, bool>,
    missing: &[RendererCapability],
) {
    tracing::info!(
        product = ?identity.version.product,
        browser_version = %identity.version.product_version().as_str(),
        protocol_version = %identity.version.protocol_version(),
        endpoint_kind = endpoint_kind.as_str(),
        required_capabilities_ok = missing.is_empty(),
        missing_capabilities = ?missing,
        probed_capability_count = availability.len(),
        "browser.compatibility.probed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_the_complete_required_capability_set() {
        assert_eq!(
            RENDERER_CAPABILITY_PROBES.len(),
            RendererCapability::ALL.len()
        );
        for capability in RendererCapability::ALL {
            assert!(
                RENDERER_CAPABILITY_PROBES
                    .iter()
                    .any(|probe| probe.capability == *capability)
            );
        }
    }

    #[test]
    fn branding_does_not_make_node_inspector_a_renderer() {
        assert_eq!(
            classify_endpoint("Node.js/22", "Node.js/22"),
            EndpointKind::NodeInspector
        );
        assert_eq!(
            classify_endpoint("Electron/35.0.0", "Chrome/135 Electron/35"),
            EndpointKind::ElectronRenderer
        );
    }
}

//! Disposable adapter for the published cdpkit 0.4.0 crate.
//! No reconnect, target lifecycle, or production capture policy belongs here.

use std::{collections::HashMap, sync::Arc};

use cdpkit::{CDP, Sender, page, target};
use futures_util::StreamExt;
use tokio::sync::Mutex;

use super::{
    contract::{
        EventStream, ScreencastFrame, SpikeFuture, SpikeTransport, SpikeTransportFactory,
        TransportScope, TypedProbeEvidence,
    },
    error::{SpikeError, SpikeErrorCode},
    evidence::CandidateIdentity,
};

type PageFrames = cdpkit::EventStream<serde_json::Value>;

/// The exact crates.io candidate. The checksum is copied from Cargo.lock and is intentionally
/// reported as evidence so a locally patched checkout cannot masquerade as the candidate.
#[derive(Clone, Debug, Default)]
pub struct CdpkitTransportFactory {
    scripted_endpoint: Option<String>,
}

impl CdpkitTransportFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_scripted_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            scripted_endpoint: Some(endpoint.into()),
        }
    }
}

impl SpikeTransportFactory for CdpkitTransportFactory {
    fn candidate(&self) -> CandidateIdentity {
        CandidateIdentity {
            name: "cdpkit".into(),
            version: "0.4.0".into(),
            checksum: "c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa".into(),
        }
    }

    fn connect<'a>(
        &'a self,
        browser_ws_url: &'a str,
    ) -> SpikeFuture<'a, Result<Box<dyn SpikeTransport>, SpikeError>> {
        let endpoint = self
            .scripted_endpoint
            .as_deref()
            .filter(|_| browser_ws_url == "scripted-peer")
            .unwrap_or(browser_ws_url)
            .to_owned();
        Box::pin(async move {
            let cdp = CDP::connect_ws(&endpoint)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Connect, error))?;
            cdp.set_command_timeout(std::time::Duration::from_secs(5));
            Ok(Box::new(CdpkitTransport {
                cdp,
                frames: Arc::new(Mutex::new(HashMap::new())),
            }) as Box<dyn SpikeTransport>)
        })
    }
}

pub struct CdpkitTransport {
    cdp: CDP,
    frames: Arc<Mutex<HashMap<String, PageFrames>>>,
}

impl CdpkitTransport {
    fn sender(&self, scope: &TransportScope) -> SenderRef<'_> {
        match scope {
            TransportScope::Browser => SenderRef::Browser(&self.cdp),
            TransportScope::Session { session_id } => {
                SenderRef::Session(self.cdp.owned_session(session_id.clone()))
            }
        }
    }
}

// A small enum keeps the session handle alive for the duration of each command without leaking
// cdpkit types through the spike contract.
enum SenderRef<'a> {
    Browser(&'a CDP),
    Session(cdpkit::OwnedSession),
}

impl SenderRef<'_> {
    async fn raw(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, SpikeError> {
        match self {
            Self::Browser(sender) => sender
                .send_raw(method, params)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Command, error)),
            Self::Session(sender) => sender
                .send_raw(method, params)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Command, error)),
        }
    }
}

impl SpikeTransport for CdpkitTransport {
    fn send_raw<'a>(
        &'a self,
        scope: &'a TransportScope,
        method: &'a str,
        params: serde_json::Value,
    ) -> SpikeFuture<'a, Result<serde_json::Value, SpikeError>> {
        Box::pin(async move { self.sender(scope).raw(method, params).await })
    }

    fn subscribe_named<'a>(
        &'a self,
        scope: &'a TransportScope,
        method: &'a str,
    ) -> SpikeFuture<'a, Result<EventStream, SpikeError>> {
        Box::pin(async move {
            let name = method.to_owned();
            let event_scope = scope.clone();
            let stream = match scope {
                TransportScope::Browser => self.cdp.event_stream::<serde_json::Value>(method),
                TransportScope::Session { session_id } => self
                    .cdp
                    .owned_session(session_id.clone())
                    .event_stream::<serde_json::Value>(method),
            };
            if method == "Protocol.unknownEvent"
                || method == "Runtime.additiveField"
                || method == "Runtime.unknownEnum"
            {
                // The scripted peer releases each fixture only after this named subscription is
                // installed. Chrome treats the additive marker as an ignored Browser.getVersion
                // parameter, so the same candidate path remains usable for both probes.
                let _ = self
                    .cdp
                    .send_raw(
                        "Browser.getVersion",
                        serde_json::json!({"scripted_drift": method}),
                    )
                    .await;
            }
            Ok(Box::pin(stream.map(move |params| {
                Ok(super::contract::NamedEventParams {
                    method: name.clone(),
                    scope: event_scope.clone(),
                    params,
                })
            })) as EventStream)
        })
    }

    fn run_typed_probe<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<TypedProbeEvidence, SpikeError>> {
        Box::pin(async move {
            let sender = match session {
                TransportScope::Session { session_id } => {
                    self.cdp.owned_session(session_id.clone())
                }
                TransportScope::Browser => {
                    return Err(SpikeError::new(
                        SpikeErrorCode::Routing,
                        "typed probe requires a page session",
                    ));
                }
            };
            page::methods::Enable::new()
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Protocol, error))?;
            cdpkit::runtime::methods::Evaluate::new("1 + 1")
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Protocol, error))?;
            cdpkit::accessibility::methods::Enable::new()
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Protocol, error))?;
            cdpkit::accessibility::methods::GetFullAxTree::new()
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Protocol, error))?;
            cdpkit::input::methods::DispatchMouseEvent::new("mouseMoved", 1.0, 1.0)
                .with_button(cdpkit::input::types::MouseButton::None)
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Protocol, error))?;
            let browser_version = cdpkit::browser::methods::GetVersion::new()
                .send(&self.cdp)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Protocol, error))?;
            if browser_version.product.is_empty() || browser_version.protocol_version.is_empty() {
                return Err(SpikeError::new(
                    SpikeErrorCode::Protocol,
                    "Browser.getVersion returned incomplete identity",
                ));
            }
            Ok(TypedProbeEvidence {
                browser_version_observed: true,
                page_enable_observed: true,
                runtime_evaluate_observed: true,
                accessibility_observed: true,
                input_observed: true,
            })
        })
    }

    fn attach_flat_page<'a>(
        &'a self,
        target_id: &'a str,
    ) -> SpikeFuture<'a, Result<TransportScope, SpikeError>> {
        Box::pin(async move {
            let attached = target::methods::AttachToTarget::new(target_id.to_owned())
                .with_flatten(true)
                .send(&self.cdp)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Routing, error))?;
            Ok(TransportScope::session(attached.session_id))
        })
    }

    fn start_screencast<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<(), SpikeError>> {
        Box::pin(async move {
            let session_id = session.session_id().ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::Routing,
                    "screencast requires a page session",
                )
            })?;
            let sender = self.cdp.owned_session(session_id.to_owned());
            let stream = sender.event_stream::<serde_json::Value>("Page.screencastFrame");
            self.frames
                .lock()
                .await
                .insert(session_id.to_owned(), stream);
            page::methods::StartScreencast::new()
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Command, error))?;
            Ok(())
        })
    }

    fn next_screencast_frame<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<ScreencastFrame, SpikeError>> {
        Box::pin(async move {
            let session_id = session.session_id().ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::Routing,
                    "screencast requires a page session",
                )
            })?;
            let mut frames = self.frames.lock().await;
            let stream = frames.get_mut(session_id).ok_or_else(|| {
                SpikeError::new(SpikeErrorCode::Invariant, "screencast was not started")
            })?;
            let frame = stream.next().await.ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::SubscriptionClosed,
                    "screencast stream closed",
                )
            })?;
            let sequence = frame
                .get("sessionId")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| {
                    SpikeError::new(
                        SpikeErrorCode::Protocol,
                        "screencast frame lacked sessionId",
                    )
                })?;
            let data = frame
                .get("data")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    SpikeError::new(SpikeErrorCode::Protocol, "screencast frame lacked data")
                })?
                .to_owned();
            let metadata = frame
                .get("metadata")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            Ok(ScreencastFrame {
                scope: session.clone(),
                sequence,
                data,
                metadata,
            })
        })
    }

    fn ack_screencast<'a>(
        &'a self,
        session: &'a TransportScope,
        sequence: i64,
    ) -> SpikeFuture<'a, Result<(), SpikeError>> {
        Box::pin(async move {
            let session_id = session.session_id().ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::Routing,
                    "screencast requires a page session",
                )
            })?;
            let sender = self.cdp.owned_session(session_id.to_owned());
            page::methods::ScreencastFrameAck::new(sequence)
                .send(&sender)
                .await
                .map_err(|error| map_error(SpikeErrorCode::Command, error))
        })
    }

    fn close_reason(&self) -> Option<super::contract::DisconnectEvidence> {
        self.cdp
            .close_reason()
            .map(|reason| super::contract::DisconnectEvidence {
                reason: format!("{reason:?}"),
                pending_calls_closed: self.cdp.is_closed(),
                subscriptions_closed: self.cdp.is_closed(),
            })
    }
}

fn map_error(code: SpikeErrorCode, error: cdpkit::CdpError) -> SpikeError {
    let retryable = error.is_connection_failed()
        || matches!(
            error,
            cdpkit::CdpError::ConnectionClosed | cdpkit::CdpError::ChannelClosed
        );
    let code = if error.is_connection_failed()
        || matches!(
            error,
            cdpkit::CdpError::ConnectionClosed | cdpkit::CdpError::ChannelClosed
        ) {
        SpikeErrorCode::Disconnected
    } else if error.is_protocol_error() {
        SpikeErrorCode::Protocol
    } else {
        code
    };
    SpikeError::new(code, error.to_string()).retryable_if(retryable)
}

trait RetryableIf {
    fn retryable_if(self, retryable: bool) -> Self;
}

impl RetryableIf for SpikeError {
    fn retryable_if(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

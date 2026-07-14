use std::{collections::HashSet, future::Future, time::Duration};

use krometrail_core::{
    DocumentReadiness, ElementLocator, ElementState, ErrorCode, EvaluationValue, KrometrailError,
    ObservationContext, Result, RetryAdvice, SessionTime, UrlMatch, WaitCondition, WaitOutcome,
    WaitPresence, WaitProbe, WaitRequest, WaitResult, WaitTextMatch,
};
use serde_json::{Value, json};

use super::{
    BoundTarget, PageControl, bind_target, evaluation::decode_evaluation,
    navigation::OperationCancellation, operation_error, transport_error,
};
use crate::{
    SupervisorState,
    transport::{CdpTransport, CommandScope, NamedEvent, TransportError, TransportEvents},
};

impl PageControl {
    pub(crate) async fn execute_wait(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: WaitRequest,
        cancel: &OperationCancellation,
        parent_deadline: Option<tokio::time::Instant>,
    ) -> Result<WaitResult> {
        let bound = bind_target(state, request.target)?;
        let started_at = self.session_time()?;
        let own_deadline = tokio::time::Instant::now() + request.timeout;
        let deadline = parent_deadline.map_or(own_deadline, |parent| parent.min(own_deadline));
        let generation = state.connection_generation;

        if let WaitCondition::Elapsed { duration } = request.condition {
            return self
                .wait_elapsed(
                    &bound,
                    started_at,
                    duration,
                    request.condition,
                    deadline,
                    generation,
                    cancel,
                )
                .await;
        }
        if let WaitCondition::NetworkQuiet { quiet_for } = request.condition {
            return self
                .wait_network_quiet(
                    transport,
                    &bound,
                    started_at,
                    quiet_for,
                    request.condition,
                    request.poll_interval,
                    deadline,
                    generation,
                    cancel,
                )
                .await;
        }

        let mut lifecycle_events = if matches!(request.condition, WaitCondition::Navigation { .. })
        {
            match controlled(cancel, generation, bound.target_id, deadline, async {
                transport
                    .subscribe_named(
                        &CommandScope::Session(bound.transport_session.clone()),
                        "Page.lifecycleEvent",
                    )
                    .await
                    .map_err(|error| {
                        transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
                    })
            })
            .await?
            {
                Controlled::Value(events) => Some(events),
                Controlled::TimedOut => {
                    return self.timed_out(&bound, started_at, request.condition, None, started_at);
                }
            }
        } else {
            None
        };

        let mut last_probe = None;
        let mut last_probe_at = started_at;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return self.timed_out(
                    &bound,
                    started_at,
                    request.condition,
                    last_probe,
                    last_probe_at,
                );
            }
            let probe = controlled(
                cancel,
                generation,
                bound.target_id,
                deadline,
                self.probe_condition(transport, &bound, &request.condition),
            )
            .await?;
            let probe = match probe {
                Controlled::Value(probe) => probe,
                Controlled::TimedOut => {
                    return self.timed_out(
                        &bound,
                        started_at,
                        request.condition,
                        last_probe,
                        last_probe_at,
                    );
                }
            };
            let probe_at = self.session_time()?;
            if probe.matched() {
                return self.satisfied(&bound, started_at, request.condition, probe, probe_at);
            }
            last_probe = Some(probe);
            last_probe_at = probe_at;

            let wake_at = (tokio::time::Instant::now() + request.poll_interval).min(deadline);
            if let Some(events) = lifecycle_events.as_deref_mut() {
                tokio::select! {
                    biased;
                    error = cancel.wait(generation, bound.target_id) => return Err(error),
                    _ = tokio::time::sleep_until(deadline) => {
                        return self.timed_out(&bound, started_at, request.condition, last_probe, last_probe_at);
                    }
                    event = events.next() => {
                        if !matches!(event, Ok(Some(_))) {
                            lifecycle_events = None;
                        }
                    }
                    _ = tokio::time::sleep_until(wake_at) => {}
                }
            } else {
                match controlled(cancel, generation, bound.target_id, deadline, async {
                    tokio::time::sleep_until(wake_at).await;
                    Ok(())
                })
                .await?
                {
                    Controlled::Value(()) => {}
                    Controlled::TimedOut => {
                        return self.timed_out(
                            &bound,
                            started_at,
                            request.condition,
                            last_probe,
                            last_probe_at,
                        );
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn wait_elapsed(
        &self,
        bound: &BoundTarget,
        started_at: SessionTime,
        duration: Duration,
        condition: WaitCondition,
        deadline: tokio::time::Instant,
        generation: u64,
        cancel: &OperationCancellation,
    ) -> Result<WaitResult> {
        let started = tokio::time::Instant::now();
        let wake = started + duration;
        match controlled(cancel, generation, bound.target_id, deadline, async {
            tokio::time::sleep_until(wake).await;
            Ok(())
        })
        .await?
        {
            Controlled::Value(()) => {
                let at = self.session_time()?;
                self.satisfied(
                    bound,
                    started_at,
                    condition,
                    WaitProbe::Elapsed {
                        matched: true,
                        elapsed_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
                    },
                    at,
                )
            }
            Controlled::TimedOut => {
                let elapsed = tokio::time::Instant::now().saturating_duration_since(started);
                self.timed_out(
                    bound,
                    started_at,
                    condition,
                    Some(WaitProbe::Elapsed {
                        matched: false,
                        elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                    }),
                    self.session_time()?,
                )
            }
        }
    }

    async fn probe_condition(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        condition: &WaitCondition,
    ) -> Result<WaitProbe> {
        match condition {
            WaitCondition::Text {
                locator,
                text,
                match_mode,
                presence,
                case_sensitive,
            } => {
                self.probe_text(
                    transport,
                    bound,
                    locator.as_ref(),
                    text.as_str(),
                    *match_mode,
                    *presence,
                    *case_sensitive,
                )
                .await
            }
            WaitCondition::Element { locator, state } => {
                self.probe_element(transport, bound, locator, *state).await
            }
            WaitCondition::Navigation { readiness, url } => {
                self.probe_navigation(transport, bound, *readiness, url.as_ref())
                    .await
            }
            WaitCondition::Page { expression } => {
                self.probe_page(transport, bound, expression.as_str()).await
            }
            WaitCondition::Elapsed { .. } | WaitCondition::NetworkQuiet { .. } => {
                unreachable!("elapsed and network quiet use dedicated wait strategies")
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn probe_text(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        locator: Option<&ElementLocator>,
        needle: &str,
        match_mode: WaitTextMatch,
        presence: WaitPresence,
        case_sensitive: bool,
    ) -> Result<WaitProbe> {
        let object_id = match locator {
            Some(locator) => {
                self.snapshots
                    .resolve_wait_object(transport, bound, locator)
                    .await?
            }
            None => None,
        };
        if locator.is_some() && object_id.is_none() {
            return Ok(WaitProbe::Text {
                matched: presence == WaitPresence::Absent,
                observed_length: None,
            });
        }
        let function = "function(needle,mode,presence,caseSensitive){if(!this.isConnected)return {attached:false,matched:presence==='absent',length:null};let value=this.innerText??this.textContent??'';const length=Math.min(value.length,4294967295);if(!caseSensitive){value=value.toLocaleLowerCase();needle=needle.toLocaleLowerCase();}const found=mode==='exact'?value===needle:value.includes(needle);return {attached:true,matched:presence==='present'?found:!found,length};}";
        let response = if let Some(object_id) = object_id {
            transport
                .send_raw(
                    &CommandScope::Session(bound.transport_session.clone()),
                    "Runtime.callFunctionOn",
                    json!({
                        "objectId": object_id,
                        "functionDeclaration": function,
                        "arguments": [
                            {"value":needle},
                            {"value":match match_mode { WaitTextMatch::Contains => "contains", WaitTextMatch::Exact => "exact" }},
                            {"value":match presence { WaitPresence::Present => "present", WaitPresence::Absent => "absent" }},
                            {"value":case_sensitive}
                        ],
                        "returnByValue":true,
                        "throwOnSideEffect":true,
                        "silent":true
                    }),
                )
                .await
        } else {
            let encoded = serde_json::to_string(needle).expect("text predicate serializes");
            let mode = match match_mode {
                WaitTextMatch::Contains => "contains",
                WaitTextMatch::Exact => "exact",
            };
            let presence = match presence {
                WaitPresence::Present => "present",
                WaitPresence::Absent => "absent",
            };
            let expression = format!(
                "(()=>{{const needle={encoded},mode='{mode}',presence='{presence}',caseSensitive={case_sensitive};let value=document.body?.innerText??document.documentElement?.innerText??'';const length=Math.min(value.length,4294967295);let n=needle;if(!caseSensitive){{value=value.toLocaleLowerCase();n=n.toLocaleLowerCase();}}const found=mode==='exact'?value===n:value.includes(n);return{{attached:true,matched:presence==='present'?found:!found,length}};}})()"
            );
            transport
                .send_raw(
                    &CommandScope::Session(bound.transport_session.clone()),
                    "Runtime.evaluate",
                    json!({"expression":expression,"returnByValue":true,"throwOnSideEffect":true,"silent":true}),
                )
                .await
        }
        .map_err(|error| transport_error(error, ErrorCode::PageObservationFailed, bound.target_id))?;
        let value = by_value(&response).ok_or_else(|| wait_probe_error(bound.target_id))?;
        if matches!(locator, Some(ElementLocator::Reference(_)))
            && value.get("attached").and_then(Value::as_bool) != Some(true)
        {
            return Err(operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "wait reference is no longer attached to the document",
            ));
        }
        Ok(WaitProbe::Text {
            matched: value
                .get("matched")
                .and_then(Value::as_bool)
                .ok_or_else(|| wait_probe_error(bound.target_id))?,
            observed_length: value
                .get("length")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok()),
        })
    }

    async fn probe_element(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        locator: &ElementLocator,
        requested: ElementState,
    ) -> Result<WaitProbe> {
        let object_id = self
            .snapshots
            .resolve_wait_object(transport, bound, locator)
            .await?;
        let Some(object_id) = object_id else {
            return Ok(WaitProbe::Element {
                matched: requested == ElementState::Hidden,
                attached: false,
                visible: None,
                enabled: None,
                editable: None,
                checked: None,
            });
        };
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Runtime.callFunctionOn",
                json!({
                    "objectId":object_id,
                    "functionDeclaration":"function(){if(!this.isConnected)return {attached:false};const s=getComputedStyle(this),r=this.getBoundingClientRect();let n=this,inert=false;while(n&&!inert){inert=n.inert===true;n=n.parentElement;}const visible=!this.hidden&&s.display!=='none'&&s.visibility!=='hidden'&&s.visibility!=='collapse'&&s.contentVisibility!=='hidden'&&r.width>0&&r.height>0;const enabled=!inert&&!this.disabled&&this.getAttribute('aria-disabled')!=='true';const tag=this.tagName,type=tag==='INPUT'?(this.type||'text').toLowerCase():null;const editable=enabled&&!this.readOnly&&(this.isContentEditable||(tag==='INPUT'&&/^(text|search|url|email|tel|password|number)$/.test(type))||tag==='TEXTAREA');const checked=typeof this.checked==='boolean'?this.checked:null;return {attached:true,visible,enabled,editable,checked};}",
                    "returnByValue":true,"throwOnSideEffect":true,"silent":true
                }),
            )
            .await
            .map_err(|error| transport_error(error, ErrorCode::PageObservationFailed, bound.target_id))?;
        let value = by_value(&response).ok_or_else(|| wait_probe_error(bound.target_id))?;
        let attached = value.get("attached").and_then(Value::as_bool) == Some(true);
        if !attached && matches!(locator, ElementLocator::Reference(_)) {
            return Err(operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "wait reference is no longer attached to the document",
            ));
        }
        let visible = value.get("visible").and_then(Value::as_bool);
        let enabled = value.get("enabled").and_then(Value::as_bool);
        let editable = value.get("editable").and_then(Value::as_bool);
        let checked = value.get("checked").and_then(Value::as_bool);
        let matched = match requested {
            ElementState::Attached => attached,
            ElementState::Visible => visible == Some(true),
            ElementState::Hidden => !attached || visible == Some(false),
            ElementState::Enabled => enabled == Some(true),
            ElementState::Disabled => enabled == Some(false),
            ElementState::Editable => editable == Some(true),
            ElementState::Checked => checked == Some(true),
            ElementState::Unchecked => checked == Some(false),
        };
        Ok(WaitProbe::Element {
            matched,
            attached,
            visible,
            enabled,
            editable,
            checked,
        })
    }

    async fn probe_navigation(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        requested: DocumentReadiness,
        url: Option<&(UrlMatch, krometrail_core::NonEmptyText)>,
    ) -> Result<WaitProbe> {
        let (mode, expected) = url.map_or(("none", ""), |(mode, value)| {
            (
                match mode {
                    UrlMatch::Exact => "exact",
                    UrlMatch::Prefix => "prefix",
                },
                value.as_str(),
            )
        });
        let expected = serde_json::to_string(expected).expect("URL predicate serializes");
        let expression = format!(
            "(()=>{{const expected={expected},mode='{mode}',readiness=document.readyState;const rank=readiness==='complete'?2:readiness==='interactive'?1:0;const required={};const urlMatched=mode==='none'?null:(mode==='exact'?location.href===expected:location.href.startsWith(expected));return{{readiness,urlMatched,matched:rank>=required&&(urlMatched??true)}};}})()",
            readiness_rank(requested)
        );
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true,"throwOnSideEffect":true,"silent":true}),
            )
            .await
            .map_err(|error| transport_error(error, ErrorCode::NavigationFailed, bound.target_id))?;
        let value = by_value(&response).ok_or_else(|| wait_probe_error(bound.target_id))?;
        let readiness = match value.get("readiness").and_then(Value::as_str) {
            Some("loading") => DocumentReadiness::Loading,
            Some("interactive") => DocumentReadiness::Interactive,
            Some("complete") => DocumentReadiness::Complete,
            _ => return Err(wait_probe_error(bound.target_id)),
        };
        Ok(WaitProbe::Navigation {
            matched: value
                .get("matched")
                .and_then(Value::as_bool)
                .ok_or_else(|| wait_probe_error(bound.target_id))?,
            readiness,
            url_matched: value.get("urlMatched").and_then(Value::as_bool),
        })
    }

    async fn probe_page(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        expression: &str,
    ) -> Result<WaitProbe> {
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Runtime.evaluate",
                json!({
                    "expression":expression,
                    "returnByValue":true,
                    "awaitPromise":false,
                    "throwOnSideEffect":true,
                    "silent":true,
                    "timeout":u64::try_from(self.config.evaluation_timeout.as_millis()).unwrap_or(u64::MAX)
                }),
            )
            .await
            .map_err(|error| transport_error(error, ErrorCode::EvaluationFailed, bound.target_id))?;
        let value = decode_evaluation(&response, bound.target_id)?;
        let EvaluationValue::Json(Value::Bool(matched)) = value else {
            return Err(operation_error(
                ErrorCode::EvaluationFailed,
                bound.target_id,
                "wait page condition must return a JSON boolean",
            ));
        };
        Ok(WaitProbe::Page { matched })
    }

    #[allow(clippy::too_many_arguments)]
    async fn wait_network_quiet(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        started_at: SessionTime,
        quiet_for: Duration,
        condition: WaitCondition,
        poll_interval: Duration,
        deadline: tokio::time::Instant,
        generation: u64,
        cancel: &OperationCancellation,
    ) -> Result<WaitResult> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let started = match self
            .subscribe_network(
                transport,
                &scope,
                "Network.requestWillBeSent",
                bound,
                deadline,
                generation,
                cancel,
            )
            .await?
        {
            Controlled::Value(events) => events,
            Controlled::TimedOut => {
                return self.timed_out(bound, started_at, condition, None, started_at);
            }
        };
        let finished = match self
            .subscribe_network(
                transport,
                &scope,
                "Network.loadingFinished",
                bound,
                deadline,
                generation,
                cancel,
            )
            .await?
        {
            Controlled::Value(events) => events,
            Controlled::TimedOut => {
                return self.timed_out(bound, started_at, condition, None, started_at);
            }
        };
        let failed = match self
            .subscribe_network(
                transport,
                &scope,
                "Network.loadingFailed",
                bound,
                deadline,
                generation,
                cancel,
            )
            .await?
        {
            Controlled::Value(events) => events,
            Controlled::TimedOut => {
                return self.timed_out(bound, started_at, condition, None, started_at);
            }
        };
        match controlled(cancel, generation, bound.target_id, deadline, async {
            transport
                .send_raw(&scope, "Network.enable", json!({}))
                .await
                .map_err(|error| network_setup_error(error, bound.target_id))
        })
        .await?
        {
            Controlled::Value(_) => {}
            Controlled::TimedOut => {
                return self.timed_out(bound, started_at, condition, None, started_at);
            }
        }

        let (mut network_events, _event_pumps) = pump_network_events([started, finished, failed]);
        let mut in_flight = HashSet::<String>::new();
        let mut completed_before_start = HashSet::<String>::new();
        let mut quiet_since = Some(tokio::time::Instant::now());
        loop {
            let now = tokio::time::Instant::now();
            let quiet_elapsed =
                quiet_since.map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
            let matched = in_flight.is_empty() && quiet_elapsed >= quiet_for;
            let probe = WaitProbe::NetworkQuiet {
                matched,
                in_flight: u32::try_from(in_flight.len()).unwrap_or(u32::MAX),
                quiet_for_elapsed_ms: u64::try_from(quiet_elapsed.as_millis()).unwrap_or(u64::MAX),
                tracks_from_subscription: true,
                excludes_long_lived_connections: true,
            };
            let probe_at = self.session_time()?;
            if matched {
                return self.satisfied(bound, started_at, condition, probe, probe_at);
            }
            if now >= deadline {
                return self.timed_out(bound, started_at, condition, Some(probe), probe_at);
            }
            let quiet_wake = quiet_since
                .map(|since| since + quiet_for)
                .unwrap_or(deadline);
            let wake_at = (now + poll_interval).min(quiet_wake).min(deadline);
            let wake = tokio::select! {
                biased;
                error = cancel.wait(generation, bound.target_id) => return Err(error),
                _ = tokio::time::sleep_until(deadline) => {
                    return self.timed_out(bound, started_at, condition, Some(probe), probe_at);
                }
                event = network_events.recv() => NetworkWake::Event(event),
                _ = tokio::time::sleep_until(wake_at) => NetworkWake::Timer,
            };
            let NetworkWake::Event(event) = wake else {
                continue;
            };
            let event = event
                .ok_or_else(|| {
                    operation_error(
                        ErrorCode::BrowserDisconnected,
                        bound.target_id,
                        "network event subscriptions ended during explicit wait",
                    )
                })?
                .map_err(|error| {
                    transport_error(error, ErrorCode::BrowserDisconnected, bound.target_id)
                })?;
            update_network_state(
                event,
                &mut in_flight,
                &mut completed_before_start,
                &mut quiet_since,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn subscribe_network(
        &self,
        transport: &dyn CdpTransport,
        scope: &CommandScope,
        method: &str,
        bound: &BoundTarget,
        deadline: tokio::time::Instant,
        generation: u64,
        cancel: &OperationCancellation,
    ) -> Result<Controlled<Box<dyn TransportEvents>>> {
        controlled(cancel, generation, bound.target_id, deadline, async {
            transport
                .subscribe_named(scope, method)
                .await
                .map_err(|error| network_setup_error(error, bound.target_id))
        })
        .await
    }

    fn satisfied(
        &self,
        bound: &BoundTarget,
        started_at: SessionTime,
        condition: WaitCondition,
        probe: WaitProbe,
        matched_at: SessionTime,
    ) -> Result<WaitResult> {
        let completed_at = self.session_time()?;
        WaitResult::new(
            ObservationContext::new(
                self.session_id,
                bound.target_id,
                bound.attachment_generation,
                started_at,
                completed_at,
            )?,
            condition,
            WaitOutcome::Satisfied { matched_at },
            Some(probe),
        )
    }

    fn timed_out(
        &self,
        bound: &BoundTarget,
        started_at: SessionTime,
        condition: WaitCondition,
        last_probe: Option<WaitProbe>,
        last_probe_at: SessionTime,
    ) -> Result<WaitResult> {
        let completed_at = self.session_time()?;
        WaitResult::new(
            ObservationContext::new(
                self.session_id,
                bound.target_id,
                bound.attachment_generation,
                started_at,
                completed_at,
            )?,
            condition,
            WaitOutcome::TimedOut { last_probe_at },
            last_probe,
        )
    }
}

#[derive(Debug)]
enum Controlled<T> {
    Value(T),
    TimedOut,
}

async fn controlled<F, T>(
    cancel: &OperationCancellation,
    generation: u64,
    target_id: krometrail_core::TargetId,
    deadline: tokio::time::Instant,
    future: F,
) -> Result<Controlled<T>>
where
    F: Future<Output = Result<T>>,
{
    tokio::select! {
        biased;
        error = cancel.wait(generation, target_id) => Err(error),
        _ = tokio::time::sleep_until(deadline) => Ok(Controlled::TimedOut),
        result = future => result.map(Controlled::Value),
    }
}

enum NetworkWake {
    Timer,
    Event(Option<std::result::Result<NamedEvent, TransportError>>),
}

struct NetworkEventPumps(Vec<tokio::task::JoinHandle<()>>);

impl Drop for NetworkEventPumps {
    fn drop(&mut self) {
        for pump in self.0.drain(..) {
            pump.abort();
        }
    }
}

fn pump_network_events(
    events: [Box<dyn TransportEvents>; 3],
) -> (
    tokio::sync::mpsc::Receiver<std::result::Result<NamedEvent, TransportError>>,
    NetworkEventPumps,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    let pumps = events
        .into_iter()
        .map(|mut events| {
            let sender = sender.clone();
            tokio::spawn(async move {
                loop {
                    match events.next().await {
                        Ok(Some(event)) => {
                            if sender.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        Ok(None) => {
                            let _ = sender.send(Err(TransportError::SubscriptionClosed)).await;
                            return;
                        }
                        Err(error) => {
                            let _ = sender.send(Err(error)).await;
                            return;
                        }
                    }
                }
            })
        })
        .collect();
    drop(sender);
    (receiver, NetworkEventPumps(pumps))
}

fn update_network_state(
    event: NamedEvent,
    in_flight: &mut HashSet<String>,
    completed_before_start: &mut HashSet<String>,
    quiet_since: &mut Option<tokio::time::Instant>,
) {
    match event.method.as_str() {
        "Network.requestWillBeSent" => {
            let long_lived = event
                .params
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    kind.eq_ignore_ascii_case("websocket")
                        || kind.eq_ignore_ascii_case("eventsource")
                });
            if !long_lived
                && let Some(request_id) = event.params.get("requestId").and_then(Value::as_str)
                && !completed_before_start.remove(request_id)
            {
                in_flight.insert(request_id.to_owned());
                *quiet_since = None;
            }
        }
        "Network.loadingFinished" | "Network.loadingFailed" => {
            if let Some(request_id) = event.params.get("requestId").and_then(Value::as_str) {
                if !in_flight.remove(request_id) {
                    completed_before_start.insert(request_id.to_owned());
                }
                if in_flight.is_empty() {
                    *quiet_since = Some(tokio::time::Instant::now());
                }
            }
        }
        _ => {}
    }
}

fn by_value(response: &Value) -> Option<&Value> {
    response
        .pointer("/result/value")
        .or_else(|| response.pointer("/result/result/value"))
}

const fn readiness_rank(readiness: DocumentReadiness) -> u8 {
    match readiness {
        DocumentReadiness::Loading => 0,
        DocumentReadiness::Interactive => 1,
        DocumentReadiness::Complete => 2,
    }
}

fn wait_probe_error(target_id: krometrail_core::TargetId) -> KrometrailError {
    operation_error(
        ErrorCode::PageObservationFailed,
        target_id,
        "wait probe returned a malformed bounded projection",
    )
}

fn network_setup_error(
    error: TransportError,
    target_id: krometrail_core::TargetId,
) -> KrometrailError {
    if matches!(
        error,
        TransportError::Disconnected | TransportError::Closed | TransportError::SubscriptionClosed
    ) {
        transport_error(error, ErrorCode::BrowserDisconnected, target_id)
    } else {
        operation_error(
            ErrorCode::Unsupported,
            target_id,
            "browser transport cannot provide explicit network-quiet tracking",
        )
        .with_retry(RetryAdvice::Never)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_tracking_resets_quiet_only_for_finite_requests() {
        let mut requests = HashSet::new();
        let mut completed = HashSet::new();
        let mut quiet = Some(tokio::time::Instant::now());
        update_network_state(
            NamedEvent {
                method: "Network.requestWillBeSent".into(),
                params: json!({"requestId":"finite","type":"Fetch"}),
            },
            &mut requests,
            &mut completed,
            &mut quiet,
        );
        assert!(requests.contains("finite"));
        assert!(quiet.is_none());
        update_network_state(
            NamedEvent {
                method: "Network.requestWillBeSent".into(),
                params: json!({"requestId":"socket","type":"WebSocket"}),
            },
            &mut requests,
            &mut completed,
            &mut quiet,
        );
        assert!(!requests.contains("socket"));
        update_network_state(
            NamedEvent {
                method: "Network.loadingFailed".into(),
                params: json!({"requestId":"finite"}),
            },
            &mut requests,
            &mut completed,
            &mut quiet,
        );
        assert!(requests.is_empty());
        assert!(quiet.is_some());
    }

    #[test]
    fn readiness_is_satisfied_monotonically() {
        assert_eq!(readiness_rank(DocumentReadiness::Loading), 0);
        assert_eq!(readiness_rank(DocumentReadiness::Interactive), 1);
        assert_eq!(readiness_rank(DocumentReadiness::Complete), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn absolute_deadline_uses_the_runtime_monotonic_clock_without_wall_time() {
        let cancellation = OperationCancellation::default();
        let target_id = krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1));
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_secs(5);
        let outcome = controlled(
            &cancellation,
            0,
            target_id,
            deadline,
            std::future::pending::<Result<()>>(),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, Controlled::TimedOut));
        assert_eq!(tokio::time::Instant::now(), deadline);
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_wins_when_deadline_and_work_are_also_ready() {
        let cancellation = OperationCancellation::default();
        cancellation.stop();
        let target_id = krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1));
        let error = controlled(
            &cancellation,
            0,
            target_id,
            tokio::time::Instant::now(),
            std::future::ready(Ok(())),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
    }
}

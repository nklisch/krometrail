use krometrail_core::{
    ActionDefinition, ActionabilityRequirement, BROWSER_OPERATION_REGISTRY, BrowserActionRequest,
    BrowserOperationKind, BrowserOperationRequest, BrowserOperationResult, CompletionKind,
    CoordinateSpace, CssPoint, ErrorCode, InteractionId, InteractionLocator, InteractionOutcome,
    InteractionPostcondition, InteractionRecord, InteractionResult, LiveObservationRequest,
    LocatorSummary, NonEmptyText, ObservationContext, PageSelection, Result, SanitizedParameters,
    SideChannelSignals, TargetId, TargetVisibility,
};
use serde_json::{Value, json};

use super::{
    BoundTarget, InteractionDispatchBaselines, PageControl, bind_target,
    navigation::OperationCancellation,
    operation_error, post_action_observation_error,
    snapshot::{ReferenceRequirement, ResolvedNode, quad_bounds},
    transport_error,
};
use crate::{
    SupervisorState,
    events::{
        EventTargetBinding, PageSignalKind, PageSignalReceiveError, PageSignalReceiver,
        PageSignalSetupError, SessionDomainAuthority,
    },
    transport::{CdpTransport, CommandScope},
};

const NAVIGATION_AWARE_WINDOW: std::time::Duration = std::time::Duration::from_millis(750);
const INTERACTION_PHASE_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);
/// Ceiling for the silent post-action state probe. It runs concurrently with
/// the compositor rendezvous and live observation, so this bounds the
/// concurrent read without adding serial latency; a stalled renderer degrades
/// the facts instead of delaying a proven dispatch.
const POSTCONDITION_PROBE_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);
/// Ceiling for the serial pre-dispatch URL read. It sits ahead of dispatch on
/// every interaction, so it stays a strictly best-effort fact with a tight
/// budget; any timeout degrades `url_changed` to unobserved.
const PRE_URL_PROBE_WINDOW: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Clone, Debug)]
pub(super) enum ResolvedTarget {
    Element {
        node: ResolvedNode,
        viewport_point: CssPoint,
    },
    Coordinate {
        viewport_point: CssPoint,
    },
    TargetWide,
}

impl ResolvedTarget {
    pub(super) fn point(&self, target_id: TargetId) -> Result<CssPoint> {
        match self {
            Self::Element { viewport_point, .. } | Self::Coordinate { viewport_point } => {
                Ok(*viewport_point)
            }
            Self::TargetWide => Err(interaction_error(
                target_id,
                "interaction requires a pointer target",
            )),
        }
    }
    pub(super) fn node(&self, target_id: TargetId) -> Result<&ResolvedNode> {
        match self {
            Self::Element { node, .. } => Ok(node),
            _ => Err(interaction_error(
                target_id,
                "interaction requires an element target",
            )),
        }
    }
}

struct InteractionPlan {
    kind: BrowserOperationKind,
    action: &'static ActionDefinition,
    target: PageSelection,
    locator: Option<InteractionLocator>,
    sanitized: SanitizedParameters,
    navigation_aware: bool,
}

impl PageControl {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_interaction_request(
        &mut self,
        transport: &dyn CdpTransport,
        browser_events: &SessionDomainAuthority,
        state: &SupervisorState,
        request: BrowserOperationRequest,
        cancel: &OperationCancellation,
        parent_batch: Option<InteractionId>,
        interaction_id: InteractionId,
        dispatch_baselines: &(dyn Fn() -> InteractionDispatchBaselines + Sync),
    ) -> Result<(
        BrowserOperationResult,
        Option<TargetVisibility>,
        InteractionDispatchBaselines,
    )> {
        self.execute_interaction_request_inner(
            transport,
            browser_events,
            state,
            request,
            cancel,
            parent_batch,
            interaction_id,
            dispatch_baselines,
        )
        .await
        .map_err(|mut error| {
            error.context.session_id = Some(self.session_id);
            error.context.interaction_id = Some(interaction_id);
            error
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_interaction_request_inner(
        &mut self,
        transport: &dyn CdpTransport,
        browser_events: &SessionDomainAuthority,
        state: &SupervisorState,
        request: BrowserOperationRequest,
        cancel: &OperationCancellation,
        parent_batch: Option<InteractionId>,
        interaction_id: InteractionId,
        dispatch_baselines: &(dyn Fn() -> InteractionDispatchBaselines + Sync),
    ) -> Result<(
        BrowserOperationResult,
        Option<TargetVisibility>,
        InteractionDispatchBaselines,
    )> {
        let plan = interaction_plan(&request)?;
        let bound = bind_target(state, plan.target)?;
        let generation = state.connection_generation;
        let prepared_visibility = if matches!(
            plan.action.category,
            krometrail_core::ActionCategory::Pointer
                | krometrail_core::ActionCategory::DragDrop
                | krometrail_core::ActionCategory::Scroll
        ) {
            self.prepare_pointer_target(transport, &bound, self.focus(), cancel, generation)
                .await?
        } else {
            None
        };
        let started_at = self.session_time()?;
        let event_binding = EventTargetBinding {
            target_id: bound.target_id,
            connection_generation: generation,
            attachment_generation: bound.attachment_generation,
            transport_session: bound.transport_session.clone(),
        };
        let resolved = self
            .resolve_interaction_target(
                transport,
                &bound,
                plan.locator.as_ref(),
                requirement(plan.action.actionability),
                matches!(
                    plan.action.category,
                    krometrail_core::ActionCategory::Pointer
                        | krometrail_core::ActionCategory::DragDrop
                ),
                cancel,
                generation,
            )
            .await?;
        let pre_facts = match &resolved {
            ResolvedTarget::Element { node, .. } => Some(node.facts),
            ResolvedTarget::Coordinate { .. } | ResolvedTarget::TargetWide => None,
        };
        // Bounded pre-dispatch page-identity read for the postcondition URL
        // fact. HandleDialog skips it: an open modal blocks the renderer's
        // evaluation loop, and this silent read must never delay handling the
        // dialog it precedes. Any failure degrades the fact to unobserved.
        let pre_url = if plan.kind == BrowserOperationKind::HandleDialog {
            None
        } else {
            self.read_page_url(transport, &bound, cancel, generation)
                .await
        };
        let navigation_events = if plan.navigation_aware {
            match browser_events.page_signal(&event_binding, PageSignalKind::Lifecycle) {
                Ok(events) => Some(events),
                // Navigation awareness is an optional completion refinement. Browsers without
                // lifecycle support still dispatch the interaction and use bounded settling.
                Err(PageSignalSetupError::Unsupported) => None,
                Err(PageSignalSetupError::StaleGeneration) => {
                    return Err(operation_error(
                        ErrorCode::BrowserDisconnected,
                        bound.target_id,
                        "page event authority became stale before interaction dispatch",
                    ));
                }
            }
        } else {
            None
        };
        // Passive signal observation for the postcondition: always subscribed
        // before dispatch, never awaited, and drained at the observation
        // point. Setup failure degrades the affected fact instead of failing
        // dispatch; the navigation-aware wait above stays gated on
        // `wait_for_navigation` exactly as before.
        let mut lifecycle_observation = browser_events
            .page_signal(&event_binding, PageSignalKind::Lifecycle)
            .ok();
        let mut window_open_signals = browser_events
            .page_signal(&event_binding, PageSignalKind::WindowOpen)
            .ok();
        let mut download_request_signals = browser_events
            .page_signal(&event_binding, PageSignalKind::DownloadRequested)
            .ok();
        let mut navigation_committed_signals = browser_events
            .page_signal(&event_binding, PageSignalKind::NavigationCommitted)
            .ok();
        // Subscribe before input dispatch. A JavaScript modal can block both the command response
        // and every subsequent observation command, so command-error classification alone cannot
        // provide a non-deadlocking interaction boundary.
        let mut dialog_events = if plan.kind == BrowserOperationKind::HandleDialog {
            None
        } else {
            Some(
                browser_events
                    .page_signal(&event_binding, PageSignalKind::DialogOpening)
                    .map_err(|error| page_signal_setup_error(error, bound.target_id))?,
            )
        };
        let dispatch_baselines = dispatch_baselines();
        // Attribution fence for the passive signal drains: signals delivered
        // before this point belong to earlier activity (a late event from the
        // previous interaction queued between subscription and dispatch) and
        // must not be attributed to this interaction.
        let signal_floor = browser_events.observed_now();
        let dispatch_time = self.session_time()?;
        let dispatch = async {
            if let Some(events) = dialog_events.as_mut() {
                tokio::select! {
                    result = self.dispatch_action(transport, &bound, &request, &resolved, cancel, generation) => {
                        result?;
                        Ok(false)
                    }
                    event = cancel.race(generation, bound.target_id, events.recv()) => {
                        dialog_event_opened(event, bound.target_id)
                    }
                }
            } else {
                self.dispatch_action(transport, &bound, &request, &resolved, cancel, generation)
                    .await?;
                Ok(false)
            }
        };
        let mut observation_blocked = if plan.kind == BrowserOperationKind::HandleDialog {
            dispatch.await?
        } else {
            match tokio::time::timeout(INTERACTION_PHASE_WINDOW, dispatch).await {
                Ok(result) => result?,
                // A modal can block the in-flight input response before cdpkit yields its named
                // event. Preserve that blocked state for a following HandleDialog operation.
                Err(_) => true,
            }
        };
        let mut completion_degraded = None;
        if !observation_blocked {
            let completion = self.complete_interaction(
                transport,
                &bound,
                plan.action.completion,
                navigation_events,
                dialog_events.take(),
                cancel,
                generation,
            );
            observation_blocked = if plan.kind == BrowserOperationKind::HandleDialog {
                match completion.await {
                    Ok(blocked) => blocked,
                    Err(_) => {
                        completion_degraded = Some(operation_error(
                            ErrorCode::PageObservationFailed,
                            bound.target_id,
                            "interaction was dispatched but completion evidence is unavailable",
                        ));
                        false
                    }
                }
            } else {
                match tokio::time::timeout(INTERACTION_PHASE_WINDOW, completion).await {
                    Ok(Ok(blocked)) => blocked,
                    Ok(Err(_)) => {
                        completion_degraded = Some(operation_error(
                            ErrorCode::PageObservationFailed,
                            bound.target_id,
                            "interaction was dispatched but completion evidence is unavailable",
                        ));
                        false
                    }
                    Err(_) => true,
                }
            };
        }

        let observation_started = self.session_time()?;
        // Post-action target-state probe. Only the healthy observation path
        // probes: a blocked or degraded renderer cannot answer, and claiming
        // node detachment there would be a false fact — those paths leave the
        // target not evaluated. Probe failure on the healthy path maps to a
        // detached-or-replaced backing node with unobserved after-facts.
        let mut post_facts: Option<krometrail_core::NodeStateFacts> = None;
        let mut target_evaluated = false;
        let observation = if let Some(error) = completion_degraded {
            Box::new(self.unavailable_observation(&bound, started_at, error)?)
        } else if observation_blocked {
            Box::new(self.blocked_observation(&bound, started_at)?)
        } else {
            target_evaluated = matches!(&resolved, ResolvedTarget::Element { .. });
            // The state probe and the observation pipeline are independent
            // CDP commands; running them concurrently keeps the probe's
            // bounded window from adding serial latency ahead of the
            // observation a batch deadline could otherwise swallow.
            let (probed, observed) = {
                let probe = async {
                    let ResolvedTarget::Element { node, .. } = &resolved else {
                        return None;
                    };
                    let scope = CommandScope::Session(bound.transport_session.clone());
                    tokio::time::timeout(
                        POSTCONDITION_PROBE_WINDOW,
                        cancel.race(
                            generation,
                            bound.target_id,
                            super::snapshot::probe_backend_node_facts(
                                transport,
                                &scope,
                                bound.target_id,
                                node.backend_node_id,
                            ),
                        ),
                    )
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                };
                let observe = async {
                    let compositor_marker = self
                        .await_compositor_ready(transport, &bound, cancel, generation)
                        .await;
                    let observed = self
                        .observe_live(
                            transport,
                            &bound,
                            LiveObservationRequest {
                                target: plan.target,
                            },
                            observation_started,
                            plan.kind == BrowserOperationKind::Scroll,
                            Some((cancel, generation)),
                        )
                        .await;
                    (compositor_marker, observed)
                };
                tokio::pin!(probe);
                tokio::pin!(observe);
                tokio::select! {
                    biased;
                    observed = &mut observe => {
                        // Observation completion is the result boundary. A probe
                        // that is not already ready is optional evidence and must
                        // never extend the interaction beyond that boundary.
                        let probed = std::future::poll_fn(|context| {
                            match probe.as_mut().poll(context) {
                                std::task::Poll::Ready(result) => std::task::Poll::Ready(result),
                                std::task::Poll::Pending => std::task::Poll::Ready(None),
                            }
                        })
                        .await;
                        (probed, observed)
                    }
                    probed = &mut probe => (probed, observe.await),
                }
            };
            post_facts = probed;
            match observed {
                (
                    compositor_marker,
                    Ok((BrowserOperationResult::ObserveLive(mut observation), _)),
                ) => {
                    if let Some(warning) = compositor_marker {
                        observation.attach_screenshot_warning(warning);
                    }
                    observation
                }
                (_, Ok(_)) => unreachable!("live observation returns its associated result"),
                (_, Err(error)) => Box::new(self.unavailable_observation(
                    &bound,
                    started_at,
                    post_action_observation_error(error, bound.target_id),
                )?),
            }
        };
        let live_observation_time = observation.context.completed_at;
        // Post URL comes from the observation's page state so the comparison
        // is source-consistent with `inspect`; only the boolean flows inward.
        let post_url = match &observation.page {
            krometrail_core::ObservationPart::Available(page) => Some(page.url.as_str()),
            krometrail_core::ObservationPart::Unavailable(_) => None,
        };
        let url_changed = pre_url
            .as_deref()
            .zip(post_url)
            .map(|(pre, post)| pre != post);
        // All passive signal drains share one attribution interval:
        // dispatch fence to this observation-complete ceiling.
        let signal_ceiling = browser_events.observed_now();
        let navigation_lifecycle_observed = lifecycle_observation
            .as_mut()
            .is_some_and(|receiver| receiver.signal_observed_between(signal_floor, signal_ceiling));
        let main_frame_navigation_observed = navigation_committed_signals
            .as_mut()
            .map(|receiver| receiver.signal_observed_between(signal_floor, signal_ceiling));
        let signals = SideChannelSignals {
            window_open_attempts: window_open_signals
                .as_mut()
                .map(|receiver| receiver.observed_count_between(signal_floor, signal_ceiling)),
            download_requests: download_request_signals
                .as_mut()
                .map(|receiver| receiver.observed_count_between(signal_floor, signal_ceiling)),
        };
        let postcondition = InteractionPostcondition::from_facts(
            if target_evaluated {
                pre_facts.as_ref()
            } else {
                None
            },
            post_facts.as_ref(),
            url_changed,
            navigation_lifecycle_observed,
            main_frame_navigation_observed,
            signals,
        );
        let context = ObservationContext::new(
            self.session_id,
            bound.target_id,
            bound.attachment_generation,
            started_at,
            live_observation_time,
        )?;
        let record = InteractionRecord::new(
            interaction_id,
            context,
            dispatch_time,
            live_observation_time,
            plan.kind,
            plan.sanitized,
            LocatorSummary::from_locator(plan.locator.as_ref()),
            InteractionOutcome::Dispatched,
            postcondition,
            parent_batch,
        )?;
        Ok((
            wrap_interaction_result(
                plan.kind,
                InteractionResult {
                    record,
                    observation: *observation,
                },
            ),
            prepared_visibility,
            dispatch_baselines,
        ))
    }

    /// Silent bounded `location.href` read for the postcondition URL fact.
    /// Every failure — timeout, cancellation, transport error, malformed
    /// response — degrades to `None`; the URL itself never leaves this layer.
    async fn read_page_url(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Option<String> {
        let read = send_cdp_unmapped(
            transport,
            bound,
            "Runtime.evaluate",
            json!({
                "expression": "location.href",
                "returnByValue": true,
                "silent": true,
                "throwOnSideEffect": true,
            }),
            cancel,
            generation,
        );
        let response = tokio::time::timeout(PRE_URL_PROBE_WINDOW, read)
            .await
            .ok()?
            .ok()?
            .ok()?;
        response
            .pointer("/result/value")
            .or_else(|| response.pointer("/result/result/value"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    pub(super) async fn prepare_pointer_target(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        focus: krometrail_core::BrowserFocusPolicy,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<Option<TargetVisibility>> {
        if bound.visibility == krometrail_core::TargetVisibility::Visible {
            return Ok(None);
        }
        if focus == krometrail_core::BrowserFocusPolicy::Preserve {
            return Err(target_hidden_error(bound.target_id, focus));
        }
        self.activate_target(transport, bound, cancel, generation)
            .await
            .map(Some)
    }

    pub(crate) async fn activate_target(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<TargetVisibility> {
        let activation_error = |error| {
            super::transport_error_for_surface(
                error,
                ErrorCode::InteractionFailed,
                bound.target_id,
                "activation",
            )
        };
        let activation = async {
            cancel
                .race(
                    generation,
                    bound.target_id,
                    transport.send_raw(
                        &CommandScope::Browser,
                        "Target.activateTarget",
                        json!({"targetId": bound.browser_target_key}),
                    ),
                )
                .await?
                .map_err(activation_error)?;
            send_cdp_unmapped(
                transport,
                bound,
                "Page.bringToFront",
                json!({}),
                cancel,
                generation,
            )
            .await?
            .map_err(activation_error)?;
            loop {
                let response = send_cdp_unmapped(
                    transport,
                    bound,
                    "Runtime.evaluate",
                    json!({"expression":"document.visibilityState","returnByValue":true,"silent":true}),
                    cancel,
                    generation,
                )
                .await?
                .map_err(activation_error)?;
                let visible = response
                    .pointer("/result/value")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        response
                            .pointer("/result/result/value")
                            .and_then(Value::as_str)
                    });
                if visible == Some("visible") {
                    break Ok::<TargetVisibility, krometrail_core::KrometrailError>(
                        TargetVisibility::Visible,
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            }
        };
        match tokio::time::timeout(self.config.evaluation_timeout, activation).await {
            Ok(result) => result,
            Err(_) => Err(target_hidden_error(
                bound.target_id,
                krometrail_core::BrowserFocusPolicy::Foreground,
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn dispatch_action(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: &BrowserOperationRequest,
        resolved: &ResolvedTarget,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<()> {
        match request {
            BrowserOperationRequest::Click(request) => {
                super::pointer::click(transport, bound, request, resolved, cancel, generation).await
            }
            BrowserOperationRequest::Fill(request) => {
                super::keyboard::fill(transport, bound, request, resolved, cancel, generation).await
            }
            BrowserOperationRequest::PressKeys(request) => {
                super::keyboard::press_keys(transport, bound, request, resolved, cancel, generation)
                    .await
            }
            BrowserOperationRequest::SelectOption(request) => {
                super::form::select_option(transport, bound, request, resolved, cancel, generation)
                    .await
            }
            BrowserOperationRequest::Hover(request) => {
                super::pointer::hover(transport, bound, request, resolved, cancel, generation).await
            }
            BrowserOperationRequest::Drag(request) => {
                let destination = self
                    .resolve_interaction_target(
                        transport,
                        bound,
                        Some(&request.destination),
                        ReferenceRequirement::Actionable,
                        true,
                        cancel,
                        generation,
                    )
                    .await?;
                super::pointer::drag(
                    transport,
                    bound,
                    request,
                    resolved,
                    &destination,
                    cancel,
                    generation,
                )
                .await
            }
            BrowserOperationRequest::Scroll(request) => {
                super::pointer::scroll(transport, bound, request, resolved, cancel, generation)
                    .await
            }
            BrowserOperationRequest::UploadFiles(request) => {
                super::upload::upload_files(transport, bound, request, resolved, cancel, generation)
                    .await
            }
            BrowserOperationRequest::HandleDialog(request) => {
                super::dialog::handle_dialog(transport, bound, request, cancel, generation).await
            }
            _ => Err(interaction_error(
                bound.target_id,
                "operation is not an interaction",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_interaction_target(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        locator: Option<&InteractionLocator>,
        requirement: ReferenceRequirement,
        require_viewport_point: bool,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<ResolvedTarget> {
        let Some(locator) = locator else {
            return Ok(ResolvedTarget::TargetWide);
        };
        match locator {
            InteractionLocator::Element(locator) => {
                let mut resolved = match locator {
                    krometrail_core::ElementLocator::Reference(reference) => {
                        cancel
                            .race(
                                generation,
                                bound.target_id,
                                self.snapshots
                                    .resolve(transport, bound, *reference, requirement),
                            )
                            .await??
                    }
                    krometrail_core::ElementLocator::CssSelector(selector) => {
                        cancel
                            .race(
                                generation,
                                bound.target_id,
                                self.snapshots.resolve_selector(
                                    transport,
                                    bound,
                                    selector.as_str(),
                                    requirement,
                                ),
                            )
                            .await??
                    }
                };
                if require_viewport_point {
                    let original_backend_node_id = resolved.backend_node_id;
                    send_cdp(
                        transport,
                        bound,
                        "DOM.scrollIntoViewIfNeeded",
                        json!({"backendNodeId": resolved.backend_node_id}),
                        cancel,
                        generation,
                    )
                    .await?;
                    // Scrolling can trigger layout, virtualization, or replacement. Resolve the
                    // declared identity again and use only its post-scroll actionability/geometry.
                    resolved = match locator {
                        krometrail_core::ElementLocator::Reference(reference) => {
                            cancel
                                .race(
                                    generation,
                                    bound.target_id,
                                    self.snapshots.resolve(
                                        transport,
                                        bound,
                                        *reference,
                                        requirement,
                                    ),
                                )
                                .await??
                        }
                        krometrail_core::ElementLocator::CssSelector(selector) => {
                            cancel
                                .race(
                                    generation,
                                    bound.target_id,
                                    self.snapshots.resolve_selector(
                                        transport,
                                        bound,
                                        selector.as_str(),
                                        requirement,
                                    ),
                                )
                                .await??
                        }
                    };
                    if matches!(locator, krometrail_core::ElementLocator::CssSelector(_))
                        && resolved.backend_node_id != original_backend_node_id
                    {
                        return Err(operation_error(
                            ErrorCode::ReferenceNotActionable,
                            bound.target_id,
                            "selector resolved to a different element while preparing pointer input",
                        ));
                    }
                }
                let (min_x, max_x, min_y, max_y) = quad_bounds(&resolved.document_quad);
                let document_point = CssPoint::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)?;
                // DOM.getBoxModel quads and Input coordinates share main-frame viewport CSS
                // space. Document-space offsets are only applied to explicitly declared document
                // coordinates; subtracting scroll here mis-aims elements after page movement.
                let viewport = document_point;
                if require_viewport_point {
                    let (_, _, width, height) = self
                        .visual_viewport(transport, bound, cancel, generation)
                        .await?;
                    ensure_viewport_point(width, height, viewport, bound.target_id)?;
                }
                Ok(ResolvedTarget::Element {
                    node: resolved,
                    viewport_point: viewport,
                })
            }
            InteractionLocator::Coordinate { point, space } => {
                let (page_x, page_y, width, height) = self
                    .visual_viewport(transport, bound, cancel, generation)
                    .await?;
                let viewport_point = match space {
                    CoordinateSpace::ViewportCss => *point,
                    CoordinateSpace::DocumentCss => {
                        CssPoint::new(point.x - page_x, point.y - page_y)?
                    }
                };
                let _ = (width, height); // The hit-test is the authority for declared coordinates, including outside-viewport no-hit.
                self.hit_test_coordinate(transport, bound, viewport_point, cancel, generation)
                    .await?;
                Ok(ResolvedTarget::Coordinate { viewport_point })
            }
        }
    }

    async fn visual_viewport(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<(f64, f64, f64, f64)> {
        let layout = send_cdp(
            transport,
            bound,
            "Page.getLayoutMetrics",
            json!({}),
            cancel,
            generation,
        )
        .await?;
        let root = layout
            .get("result")
            .filter(|value| value.get("cssVisualViewport").is_some())
            .unwrap_or(&layout);
        let viewport = root
            .get("cssVisualViewport")
            .or_else(|| root.get("cssLayoutViewport"))
            .ok_or_else(|| interaction_error(bound.target_id, "visual viewport is unavailable"))?;
        Ok((
            protocol_number(viewport, "pageX", bound.target_id)?,
            protocol_number(viewport, "pageY", bound.target_id)?,
            protocol_number(viewport, "clientWidth", bound.target_id)?,
            protocol_number(viewport, "clientHeight", bound.target_id)?,
        ))
    }

    async fn hit_test_coordinate(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        point: CssPoint,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<()> {
        let expression = format!(
            "(()=>{{const e=document.elementFromPoint({:?},{:?});if(!e)return null;const r=e.getBoundingClientRect();return{{tagName:e.tagName,x:r.left,y:r.top,width:r.width,height:r.height}};}})()",
            point.x, point.y
        );
        let response = send_cdp(
            transport,
            bound,
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true,"silent":true}),
            cancel,
            generation,
        )
        .await?;
        let value = response
            .pointer("/result/value")
            .or_else(|| response.pointer("/result/result/value"));
        if value.is_none() || value == Some(&Value::Null) {
            return Err(interaction_error(
                bound.target_id,
                "no_hit_target: coordinate does not hit a page element",
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_interaction(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        completion: CompletionKind,
        navigation_events: Option<PageSignalReceiver>,
        dialog_events: Option<PageSignalReceiver>,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<bool> {
        match completion {
            CompletionKind::InputAcknowledged => Ok(false),
            CompletionKind::Settled | CompletionKind::NavigationAware => {
                let settle = send_cdp(
                    transport,
                    bound,
                    "Runtime.evaluate",
                    json!({"expression":"Promise.resolve(true)","awaitPromise":true,"returnByValue":true,"silent":true}),
                    cancel,
                    generation,
                );
                if let Some(mut events) = dialog_events {
                    tokio::select! {
                        result = settle => { result?; }
                        event = cancel.race(generation, bound.target_id, events.recv()) => {
                            if dialog_event_opened(event, bound.target_id)? {
                                return Ok(true);
                            }
                        }
                    }
                } else {
                    settle.await?;
                }
                if let Some(mut events) = navigation_events {
                    let next = tokio::time::timeout(NAVIGATION_AWARE_WINDOW, events.recv());
                    // Navigation awareness is an optional completion refinement. A bounded timeout
                    // means no lifecycle event was observed, not that the already-dispatched input
                    // failed. Cancellation and disconnect still win explicitly.
                    if let Ok(result) = cancel.race(generation, bound.target_id, next).await? {
                        result
                            .map_err(|error| page_signal_receive_error(error, bound.target_id))?;
                    }
                }
                Ok(false)
            }
        }
    }

    fn blocked_observation(
        &self,
        bound: &BoundTarget,
        started_at: krometrail_core::SessionTime,
    ) -> Result<krometrail_core::LiveObservation> {
        let error = operation_error(
            ErrorCode::PageObservationFailed,
            bound.target_id,
            "interaction_completion_blocked: handle any open dialog or retry after the renderer responds",
        );
        self.unavailable_observation(bound, started_at, error)
    }

    fn unavailable_observation(
        &self,
        bound: &BoundTarget,
        started_at: krometrail_core::SessionTime,
        error: krometrail_core::KrometrailError,
    ) -> Result<krometrail_core::LiveObservation> {
        let completed_at = self.session_time()?;
        let context = ObservationContext::new(
            self.session_id,
            bound.target_id,
            bound.attachment_generation,
            started_at,
            completed_at,
        )?;
        Ok(krometrail_core::LiveObservation {
            context,
            page: krometrail_core::ObservationPart::Unavailable(error.clone()),
            snapshot: krometrail_core::ObservationPart::Unavailable(error.clone()),
            screenshot: krometrail_core::ObservationPart::Unavailable(error),
        })
    }
}

fn dialog_event_opened(
    event: Result<std::result::Result<(), PageSignalReceiveError>>,
    target_id: TargetId,
) -> Result<bool> {
    match event? {
        Ok(()) => Ok(true),
        Err(error) => Err(page_signal_receive_error(error, target_id)),
    }
}

fn page_signal_setup_error(
    error: PageSignalSetupError,
    target_id: TargetId,
) -> krometrail_core::KrometrailError {
    let (code, message) = match error {
        PageSignalSetupError::StaleGeneration => (
            ErrorCode::BrowserDisconnected,
            "page event authority became stale before interaction dispatch",
        ),
        PageSignalSetupError::Unsupported => (
            ErrorCode::InteractionFailed,
            "browser cannot provide dialog safety events for interaction dispatch",
        ),
    };
    operation_error(code, target_id, message)
}

fn page_signal_receive_error(
    error: PageSignalReceiveError,
    target_id: TargetId,
) -> krometrail_core::KrometrailError {
    let message = match error {
        PageSignalReceiveError::Lagged => "page event authority lagged during interaction dispatch",
        PageSignalReceiveError::Closed => "page event authority closed during interaction dispatch",
    };
    operation_error(ErrorCode::InteractionFailed, target_id, message)
}

fn interaction_plan(request: &BrowserOperationRequest) -> Result<InteractionPlan> {
    let definition = BROWSER_OPERATION_REGISTRY
        .iter()
        .find(|definition| definition.kind == request.kind())
        .and_then(|definition| definition.action)
        .ok_or_else(|| {
            operation_error(
                ErrorCode::InvalidInput,
                target_hint(request),
                "operation is not an interaction",
            )
        })?;
    let (target, locator, sanitized, navigation_aware) = match request {
        BrowserOperationRequest::Click(value) => (
            value.target,
            Some(value.locator.clone()),
            value.sanitize(),
            value.wait_for_navigation,
        ),
        BrowserOperationRequest::Fill(value) => (
            value.target,
            Some(value.locator.clone()),
            value.sanitize(),
            value.wait_for_navigation,
        ),
        BrowserOperationRequest::PressKeys(value) => (
            value.target,
            value.locator.clone(),
            value.sanitize(),
            value.wait_for_navigation,
        ),
        BrowserOperationRequest::SelectOption(value) => (
            value.target,
            Some(value.locator.clone()),
            value.sanitize(),
            false,
        ),
        BrowserOperationRequest::Hover(value) => (
            value.target,
            Some(value.locator.clone()),
            value.sanitize(),
            false,
        ),
        BrowserOperationRequest::Drag(value) => (
            value.target,
            Some(value.source.clone()),
            value.sanitize(),
            false,
        ),
        BrowserOperationRequest::Scroll(value) => {
            let locator = match &value.delta {
                krometrail_core::ScrollDelta::ByOffset { .. } => None,
                krometrail_core::ScrollDelta::ToElement(locator) => {
                    Some(InteractionLocator::Element(locator.clone()))
                }
            };
            (value.target, locator, value.sanitize(), false)
        }
        BrowserOperationRequest::UploadFiles(value) => (
            value.target,
            Some(value.locator.clone()),
            value.sanitize(),
            false,
        ),
        BrowserOperationRequest::HandleDialog(value) => {
            (value.target, None, value.sanitize(), false)
        }
        _ => {
            return Err(operation_error(
                ErrorCode::InvalidInput,
                target_hint(request),
                "operation is not an interaction",
            ));
        }
    };
    Ok(InteractionPlan {
        kind: request.kind(),
        action: definition,
        target,
        locator,
        sanitized,
        navigation_aware,
    })
}

fn target_hint(request: &BrowserOperationRequest) -> TargetId {
    match request.scope() {
        krometrail_core::BrowserOperationScope::Page(PageSelection::Target(target)) => target,
        _ => TargetId::from_uuid(uuid::Uuid::nil()),
    }
}

fn requirement(value: ActionabilityRequirement) -> ReferenceRequirement {
    match value {
        ActionabilityRequirement::VisibleGeometry => ReferenceRequirement::VisibleGeometry,
        ActionabilityRequirement::Editable => ReferenceRequirement::Editable,
        ActionabilityRequirement::Selectable => ReferenceRequirement::Selectable,
        ActionabilityRequirement::FileInput => ReferenceRequirement::FileInput,
        ActionabilityRequirement::Actionable | ActionabilityRequirement::None => {
            ReferenceRequirement::Actionable
        }
    }
}

pub(super) async fn send_cdp(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    method: &str,
    params: Value,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<Value> {
    send_cdp_unmapped(transport, bound, method, params, cancel, generation)
        .await?
        .map_err(|error| transport_error(error, ErrorCode::InteractionFailed, bound.target_id))
}

/// Like [`send_cdp`], but preserves the raw transport outcome so gesture
/// dispatch can distinguish a lost command acknowledgement from a rejected
/// command. Cancellation and stale-generation failures stay hard errors.
pub(super) async fn send_cdp_unmapped(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    method: &str,
    params: Value,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<std::result::Result<Value, crate::transport::TransportError>> {
    cancel
        .race(
            generation,
            bound.target_id,
            transport.send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                method,
                params,
            ),
        )
        .await
}

fn protocol_number(value: &Value, field: &str, target_id: TargetId) -> Result<f64> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| interaction_error(target_id, "viewport geometry is malformed"))
}
fn ensure_viewport_point(
    width: f64,
    height: f64,
    point: CssPoint,
    target_id: TargetId,
) -> Result<()> {
    if point.x < 0.0 || point.y < 0.0 || point.x > width || point.y > height {
        return Err(interaction_error(
            target_id,
            "interaction coordinate lies outside the current viewport",
        ));
    }
    Ok(())
}

pub(super) fn interaction_error(
    target_id: TargetId,
    message: &'static str,
) -> krometrail_core::KrometrailError {
    operation_error(ErrorCode::InteractionFailed, target_id, message)
}

fn target_hidden_error(
    target_id: TargetId,
    focus: krometrail_core::BrowserFocusPolicy,
) -> krometrail_core::KrometrailError {
    let (message, recovery) = match focus {
        krometrail_core::BrowserFocusPolicy::Preserve => (
            "browser page is hidden and preserve focus policy did not activate it",
            "call activate_page for the selected or explicit target, then retry the pointer operation",
        ),
        krometrail_core::BrowserFocusPolicy::Foreground => (
            "browser page remained hidden after bounded foreground activation",
            "check that Chrome can foreground the managed target, then retry the pointer operation",
        ),
    };
    operation_error(ErrorCode::TargetHidden, target_id, message)
        .with_recovery(NonEmptyText::new(recovery).expect("target-hidden recovery is non-empty"))
}

fn wrap_interaction_result(
    kind: BrowserOperationKind,
    result: InteractionResult,
) -> BrowserOperationResult {
    match kind {
        BrowserOperationKind::Click => BrowserOperationResult::Click(Box::new(result)),
        BrowserOperationKind::Fill => BrowserOperationResult::Fill(Box::new(result)),
        BrowserOperationKind::PressKeys => BrowserOperationResult::PressKeys(Box::new(result)),
        BrowserOperationKind::SelectOption => {
            BrowserOperationResult::SelectOption(Box::new(result))
        }
        BrowserOperationKind::Hover => BrowserOperationResult::Hover(Box::new(result)),
        BrowserOperationKind::Drag => BrowserOperationResult::Drag(Box::new(result)),
        BrowserOperationKind::Scroll => BrowserOperationResult::Scroll(Box::new(result)),
        BrowserOperationKind::UploadFiles => BrowserOperationResult::UploadFiles(Box::new(result)),
        BrowserOperationKind::HandleDialog => {
            BrowserOperationResult::HandleDialog(Box::new(result))
        }
        _ => unreachable!("only interaction operations use interaction results"),
    }
}

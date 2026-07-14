use krometrail_core::{
    ActionDefinition, ActionabilityRequirement, BROWSER_OPERATION_REGISTRY, BrowserActionRequest,
    BrowserOperationKind, BrowserOperationRequest, BrowserOperationResult, CompletionKind,
    CoordinateSpace, CssPoint, ErrorCode, InteractionId, InteractionLocator, InteractionOutcome,
    InteractionRecord, InteractionResult, LiveObservationRequest, LocatorSummary,
    ObservationContext, PageSelection, Result, SanitizedParameters, TargetId,
};
use serde_json::{Value, json};

use super::{
    BoundTarget, PageControl, bind_target,
    navigation::OperationCancellation,
    operation_error,
    snapshot::{ReferenceRequirement, ResolvedNode, quad_bounds},
    transport_error,
};
use crate::{
    SupervisorState,
    transport::{CdpTransport, CommandScope, TransportEvents},
};

const NAVIGATION_AWARE_WINDOW: std::time::Duration = std::time::Duration::from_millis(750);

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
    pub(crate) async fn execute_interaction_request(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: BrowserOperationRequest,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult> {
        let plan = interaction_plan(&request)?;
        let bound = bind_target(state, plan.target)?;
        let started_at = self.session_time()?;
        let interaction_id = InteractionId::from_uuid(*self.ids.next().as_uuid());
        let generation = state.connection_generation;
        let scope = CommandScope::Session(bound.transport_session.clone());
        let resolved = self
            .resolve_interaction_target(
                transport,
                &bound,
                plan.locator.as_ref(),
                requirement(plan.action.actionability),
                cancel,
                generation,
            )
            .await?;
        let navigation_events = if plan.navigation_aware {
            Some(
                cancel
                    .race(
                        generation,
                        bound.target_id,
                        transport.subscribe_named(&scope, "Page.lifecycleEvent"),
                    )
                    .await?
                    .map_err(|error| {
                        transport_error(error, ErrorCode::InteractionFailed, bound.target_id)
                    })?,
            )
        } else {
            None
        };
        let dispatch_time = self.session_time()?;
        self.dispatch_action(transport, &bound, &request, &resolved, cancel, generation)
            .await?;
        self.complete_interaction(
            transport,
            &bound,
            plan.action.completion,
            navigation_events,
            cancel,
            generation,
        )
        .await?;

        let observation_started = self.session_time()?;
        let (observation, _interruption) = self
            .observe_live(
                transport,
                &bound,
                LiveObservationRequest {
                    target: plan.target,
                },
                observation_started,
                Some((cancel, generation)),
            )
            .await?;
        let BrowserOperationResult::ObserveLive(observation) = observation else {
            unreachable!("live observation returns its associated result")
        };
        let live_observation_time = observation.context.completed_at;
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
            None,
        )?;
        Ok(wrap_interaction_result(
            plan.kind,
            InteractionResult {
                record,
                observation: *observation,
            },
        ))
    }

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
            BrowserOperationRequest::UploadFiles(_) | BrowserOperationRequest::HandleDialog(_) => {
                Err(operation_error(
                    ErrorCode::Unsupported,
                    bound.target_id,
                    "upload and dialog interactions are not available",
                ))
            }
            _ => Err(interaction_error(
                bound.target_id,
                "operation is not an interaction",
            )),
        }
    }

    async fn resolve_interaction_target(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        locator: Option<&InteractionLocator>,
        requirement: ReferenceRequirement,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<ResolvedTarget> {
        let Some(locator) = locator else {
            return Ok(ResolvedTarget::TargetWide);
        };
        match locator {
            InteractionLocator::Element(locator) => {
                let resolved = match locator {
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
                let (min_x, max_x, min_y, max_y) = quad_bounds(&resolved.document_quad);
                let document_point = CssPoint::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)?;
                let viewport = self
                    .document_to_viewport(transport, bound, document_point, cancel, generation)
                    .await?;
                Ok(ResolvedTarget::Element {
                    node: resolved,
                    viewport_point: viewport,
                })
            }
            InteractionLocator::Coordinate { point, space } => {
                let viewport_point = match space {
                    CoordinateSpace::ViewportCss => *point,
                    CoordinateSpace::DocumentCss => {
                        self.document_to_viewport(transport, bound, *point, cancel, generation)
                            .await?
                    }
                };
                self.hit_test_coordinate(transport, bound, viewport_point, cancel, generation)
                    .await?;
                Ok(ResolvedTarget::Coordinate { viewport_point })
            }
        }
    }

    async fn document_to_viewport(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        point: CssPoint,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<CssPoint> {
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
        let page_x = protocol_number(viewport, "pageX", bound.target_id)?;
        let page_y = protocol_number(viewport, "pageY", bound.target_id)?;
        let local = CssPoint::new(point.x - page_x, point.y - page_y)?;
        ensure_viewport_point(viewport, local, bound.target_id)?;
        Ok(local)
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
            json!({"expression":expression,"returnByValue":true,"throwOnSideEffect":true,"silent":true}),
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

    async fn complete_interaction(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        completion: CompletionKind,
        navigation_events: Option<Box<dyn TransportEvents>>,
        cancel: &OperationCancellation,
        generation: u64,
    ) -> Result<()> {
        match completion {
            CompletionKind::InputAcknowledged => Ok(()),
            CompletionKind::Settled | CompletionKind::NavigationAware => {
                send_cdp(
                    transport,
                    bound,
                    "Runtime.evaluate",
                    json!({"expression":"Promise.resolve(true)","awaitPromise":true,"returnByValue":true,"silent":true}),
                    cancel,
                    generation,
                )
                .await?;
                if let Some(mut events) = navigation_events {
                    let next = tokio::time::timeout(NAVIGATION_AWARE_WINDOW, events.next());
                    // Navigation awareness is an optional completion refinement. A bounded timeout
                    // means no lifecycle event was observed, not that the already-dispatched input
                    // failed. Cancellation and disconnect still win explicitly.
                    if let Ok(result) = cancel.race(generation, bound.target_id, next).await? {
                        result.map_err(|error| {
                            transport_error(error, ErrorCode::InteractionFailed, bound.target_id)
                        })?;
                    }
                }
                Ok(())
            }
        }
    }
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
        .await?
        .map_err(|error| transport_error(error, ErrorCode::InteractionFailed, bound.target_id))
}

fn protocol_number(value: &Value, field: &str, target_id: TargetId) -> Result<f64> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| interaction_error(target_id, "viewport geometry is malformed"))
}
fn ensure_viewport_point(viewport: &Value, point: CssPoint, target_id: TargetId) -> Result<()> {
    let width = protocol_number(viewport, "clientWidth", target_id)?;
    let height = protocol_number(viewport, "clientHeight", target_id)?;
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

use krometrail_core::{
    ClickRequest, CssPoint, DragRequest, HoverRequest, Modifiers, MouseButton, Result, ScrollDelta,
    ScrollRequest,
};
use serde_json::{Map, Value, json};

use super::{
    BoundTarget,
    interaction::{ResolvedTarget, interaction_error, send_cdp, send_cdp_unmapped},
    navigation::OperationCancellation,
    transport_error,
};
use crate::transport::{CdpTransport, TransportError};
use krometrail_core::ErrorCode;

const DRAG_STEPS: usize = 5;

pub(super) async fn click(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &ClickRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let point = target.point(bound.target_id)?;
    mouse_event(
        transport,
        bound,
        "mouseMoved",
        point,
        None,
        0,
        request.modifiers,
        request.click_count,
        None,
        cancel,
        generation,
    )
    .await?;
    let buttons = button_mask(request.button);
    // The interaction executor may stop awaiting dispatch when a JavaScript dialog opens. Poll the
    // stateful press/release pair together so cancellation can happen before the press or after the
    // release has been queued, but never leave Chrome with only half of the gesture dispatched.
    let results = futures_util::future::join_all([
        gesture_mouse_event(
            transport,
            bound,
            "mousePressed",
            point,
            Some(request.button),
            buttons,
            request.modifiers,
            request.click_count,
            cancel,
            generation,
        ),
        gesture_mouse_event(
            transport,
            bound,
            "mouseReleased",
            point,
            Some(request.button),
            0,
            request.modifiers,
            request.click_count,
            cancel,
            generation,
        ),
    ])
    .await;
    for result in results {
        result?;
    }
    Ok(())
}

pub(super) async fn hover(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    _request: &HoverRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    mouse_event(
        transport,
        bound,
        "mouseMoved",
        target.point(bound.target_id)?,
        None,
        0,
        Modifiers::default(),
        0,
        None,
        cancel,
        generation,
    )
    .await
}

pub(super) async fn drag(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    _request: &DragRequest,
    source: &ResolvedTarget,
    destination: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let start = source.point(bound.target_id)?;
    let end = destination.point(bound.target_id)?;
    mouse_event(
        transport,
        bound,
        "mouseMoved",
        start,
        None,
        0,
        Modifiers::default(),
        0,
        None,
        cancel,
        generation,
    )
    .await?;
    // As with click, queue the complete stateful gesture in one pollable group. The fixed bound
    // keeps this eager staging small while preserving wire order through the transport.
    let mut gesture = Vec::with_capacity(DRAG_STEPS + 2);
    gesture.push(mouse_event(
        transport,
        bound,
        "mousePressed",
        start,
        Some(MouseButton::Left),
        1,
        Modifiers::default(),
        1,
        None,
        cancel,
        generation,
    ));
    for step in 1..=DRAG_STEPS {
        let ratio = step as f64 / DRAG_STEPS as f64;
        let point = CssPoint::new(
            start.x + (end.x - start.x) * ratio,
            start.y + (end.y - start.y) * ratio,
        )?;
        gesture.push(mouse_event(
            transport,
            bound,
            "mouseMoved",
            point,
            Some(MouseButton::Left),
            1,
            Modifiers::default(),
            1,
            None,
            cancel,
            generation,
        ));
    }
    gesture.push(mouse_event(
        transport,
        bound,
        "mouseReleased",
        end,
        Some(MouseButton::Left),
        0,
        Modifiers::default(),
        1,
        None,
        cancel,
        generation,
    ));
    let results = futures_util::future::join_all(gesture).await;
    for result in results {
        result?;
    }
    Ok(())
}

pub(super) async fn scroll(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &ScrollRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    match request.delta {
        ScrollDelta::ByOffset { dx, dy } => {
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
                .filter(|v| v.get("cssVisualViewport").is_some())
                .unwrap_or(&layout);
            let viewport = root
                .get("cssVisualViewport")
                .or_else(|| root.get("cssLayoutViewport"))
                .ok_or_else(|| {
                    interaction_error(bound.target_id, "visual viewport is unavailable")
                })?;
            let width = viewport
                .get("clientWidth")
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    interaction_error(bound.target_id, "visual viewport is malformed")
                })?;
            let height = viewport
                .get("clientHeight")
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    interaction_error(bound.target_id, "visual viewport is malformed")
                })?;
            mouse_event(
                transport,
                bound,
                "mouseWheel",
                CssPoint::new(width / 2.0, height / 2.0)?,
                None,
                0,
                Modifiers::default(),
                0,
                Some((dx, dy)),
                cancel,
                generation,
            )
            .await
        }
        ScrollDelta::ToElement(_) => {
            let node = target.node(bound.target_id)?;
            send_cdp(
                transport,
                bound,
                "DOM.scrollIntoViewIfNeeded",
                json!({"backendNodeId":node.backend_node_id}),
                cancel,
                generation,
            )
            .await?;
            let point = target.point(bound.target_id)?;
            mouse_event(
                transport,
                bound,
                "mouseWheel",
                point,
                None,
                0,
                Modifiers::default(),
                0,
                Some((0.0, 0.0)),
                cancel,
                generation,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn mouse_event(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    event_type: &str,
    point: CssPoint,
    button: Option<MouseButton>,
    buttons: u8,
    modifiers: Modifiers,
    click_count: u8,
    wheel: Option<(f64, f64)>,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let params = mouse_event_params(
        bound,
        event_type,
        point,
        button,
        buttons,
        modifiers,
        click_count,
        wheel,
    )?;
    send_cdp(
        transport,
        bound,
        "Input.dispatchMouseEvent",
        params,
        cancel,
        generation,
    )
    .await?;
    Ok(())
}

/// Dispatch one half of a stateful pointer gesture. A lost command
/// acknowledgement (`TransportError::CommandFailed`) after the command was
/// accepted for sending is not a dispatch failure: Chrome queues the input
/// before acknowledging it, and the response is routinely lost when the
/// gesture itself suspends the page (for example a click handler opening a
/// popup that steals focus). The post-action observation reports whatever
/// evidence remains reachable. Every other failure stays hard.
#[allow(clippy::too_many_arguments)]
async fn gesture_mouse_event(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    event_type: &str,
    point: CssPoint,
    button: Option<MouseButton>,
    buttons: u8,
    modifiers: Modifiers,
    click_count: u8,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let params = mouse_event_params(
        bound,
        event_type,
        point,
        button,
        buttons,
        modifiers,
        click_count,
        None,
    )?;
    match send_cdp_unmapped(
        transport,
        bound,
        "Input.dispatchMouseEvent",
        params,
        cancel,
        generation,
    )
    .await?
    {
        Ok(_) => Ok(()),
        Err(TransportError::CommandFailed) => {
            tracing::debug!(
                target_id = %bound.target_id,
                event_type,
                "pointer gesture acknowledgement was lost; treating the input as dispatched"
            );
            Ok(())
        }
        Err(error) => Err(transport_error(
            error,
            ErrorCode::InteractionFailed,
            bound.target_id,
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn mouse_event_params(
    bound: &BoundTarget,
    event_type: &str,
    point: CssPoint,
    button: Option<MouseButton>,
    buttons: u8,
    modifiers: Modifiers,
    click_count: u8,
    wheel: Option<(f64, f64)>,
) -> Result<Value> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(interaction_error(
            bound.target_id,
            "pointer coordinates must be finite",
        ));
    }
    let mut params = Map::new();
    params.insert("type".into(), Value::String(event_type.to_owned()));
    params.insert("x".into(), json!(point.x));
    params.insert("y".into(), json!(point.y));
    params.insert(
        "button".into(),
        Value::String(button.map_or("none", button_name).to_owned()),
    );
    params.insert("buttons".into(), json!(buttons));
    params.insert("clickCount".into(), json!(click_count));
    params.insert("modifiers".into(), json!(modifier_mask(modifiers)));
    params.insert("pointerType".into(), Value::String("mouse".to_owned()));
    if let Some((dx, dy)) = wheel {
        params.insert("deltaX".into(), json!(dx));
        params.insert("deltaY".into(), json!(dy));
    }
    Ok(Value::Object(params))
}

fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Middle => "middle",
        MouseButton::Right => "right",
    }
}
fn button_mask(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
    }
}
pub(super) fn modifier_mask(value: Modifiers) -> u8 {
    u8::from(value.alt)
        | (u8::from(value.control) << 1)
        | (u8::from(value.meta) << 2)
        | (u8::from(value.shift) << 3)
}

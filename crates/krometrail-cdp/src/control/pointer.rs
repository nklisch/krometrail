use krometrail_core::{
    ClickRequest, CssPoint, DragRequest, HoverRequest, Modifiers, MouseButton, Result, ScrollDelta,
    ScrollRequest,
};
use serde_json::{Map, Value, json};

use super::{
    BoundTarget,
    interaction::{ResolvedTarget, interaction_error, send_cdp},
    navigation::OperationCancellation,
};
use crate::transport::CdpTransport;

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
    mouse_event(
        transport,
        bound,
        "mousePressed",
        point,
        Some(request.button),
        buttons,
        request.modifiers,
        request.click_count,
        None,
        cancel,
        generation,
    )
    .await?;
    mouse_event(
        transport,
        bound,
        "mouseReleased",
        point,
        Some(request.button),
        0,
        request.modifiers,
        request.click_count,
        None,
        cancel,
        generation,
    )
    .await?;
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
    mouse_event(
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
    )
    .await?;
    for step in 1..=DRAG_STEPS {
        let ratio = step as f64 / DRAG_STEPS as f64;
        let point = CssPoint::new(
            start.x + (end.x - start.x) * ratio,
            start.y + (end.y - start.y) * ratio,
        )?;
        mouse_event(
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
        )
        .await?;
    }
    mouse_event(
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
    )
    .await?;
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
    send_cdp(
        transport,
        bound,
        "Input.dispatchMouseEvent",
        Value::Object(params),
        cancel,
        generation,
    )
    .await?;
    Ok(())
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

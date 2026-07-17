use krometrail_core::{
    FillMode, FillRequest, KeySegment, Modifier, Modifiers, NamedKey, PressKeysRequest, Result,
};
use serde_json::{Map, Value, json};

use super::{
    BoundTarget,
    interaction::{ResolvedTarget, send_cdp},
    navigation::OperationCancellation,
    pointer::modifier_mask,
};
use crate::transport::CdpTransport;

#[derive(Clone, Copy)]
struct KeyDispatch {
    key: &'static str,
    code: &'static str,
    location: u8,
    keycode: u16,
}

const KEY_DISPATCH: &[(NamedKey, KeyDispatch)] = &[
    (NamedKey::Enter, key("Enter", "Enter", 13)),
    (NamedKey::Tab, key("Tab", "Tab", 9)),
    (NamedKey::Escape, key("Escape", "Escape", 27)),
    (NamedKey::Backspace, key("Backspace", "Backspace", 8)),
    (NamedKey::Delete, key("Delete", "Delete", 46)),
    (NamedKey::Space, key(" ", "Space", 32)),
    (NamedKey::ArrowUp, key("ArrowUp", "ArrowUp", 38)),
    (NamedKey::ArrowDown, key("ArrowDown", "ArrowDown", 40)),
    (NamedKey::ArrowLeft, key("ArrowLeft", "ArrowLeft", 37)),
    (NamedKey::ArrowRight, key("ArrowRight", "ArrowRight", 39)),
    (NamedKey::Home, key("Home", "Home", 36)),
    (NamedKey::End, key("End", "End", 35)),
    (NamedKey::PageUp, key("PageUp", "PageUp", 33)),
    (NamedKey::PageDown, key("PageDown", "PageDown", 34)),
    (NamedKey::F1, key("F1", "F1", 112)),
    (NamedKey::F2, key("F2", "F2", 113)),
    (NamedKey::F3, key("F3", "F3", 114)),
    (NamedKey::F4, key("F4", "F4", 115)),
    (NamedKey::F5, key("F5", "F5", 116)),
    (NamedKey::F6, key("F6", "F6", 117)),
    (NamedKey::F7, key("F7", "F7", 118)),
    (NamedKey::F8, key("F8", "F8", 119)),
    (NamedKey::F9, key("F9", "F9", 120)),
    (NamedKey::F10, key("F10", "F10", 121)),
    (NamedKey::F11, key("F11", "F11", 122)),
    (NamedKey::F12, key("F12", "F12", 123)),
];
const fn key(key: &'static str, code: &'static str, keycode: u16) -> KeyDispatch {
    KeyDispatch {
        key,
        code,
        location: 0,
        keycode,
    }
}

pub(super) async fn fill(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &FillRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    focus(transport, bound, target, cancel, generation).await?;
    if request.mode == FillMode::Replace {
        clear_editable(transport, bound, target, cancel, generation).await?;
    }
    send_cdp(
        transport,
        bound,
        "Input.insertText",
        json!({"text":request.value.as_str()}),
        cancel,
        generation,
    )
    .await?;
    Ok(())
}

pub(super) async fn press_keys(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &PressKeysRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    if !matches!(target, ResolvedTarget::TargetWide) {
        focus(transport, bound, target, cancel, generation).await?;
    }
    for chord in &request.keys {
        let segments = chord.segments();
        let mut modifiers = Modifiers::default();
        let mut held = Vec::new();
        for segment in &segments {
            if let KeySegment::Modifier(modifier) = segment {
                set_modifier(&mut modifiers, *modifier, true);
                dispatch_modifier(
                    transport, bound, *modifier, true, modifiers, cancel, generation,
                )
                .await?;
                held.push(*modifier);
            }
        }
        for segment in segments {
            match segment {
                KeySegment::Modifier(_) => {}
                KeySegment::NamedKey(named) => {
                    dispatch_named(transport, bound, named, modifiers, cancel, generation).await?
                }
                KeySegment::Char(ch) => {
                    dispatch_char_key(transport, bound, ch, modifiers, cancel, generation).await?
                }
            }
        }
        for modifier in held.into_iter().rev() {
            set_modifier(&mut modifiers, modifier, false);
            dispatch_modifier(
                transport, bound, modifier, false, modifiers, cancel, generation,
            )
            .await?;
        }
    }
    Ok(())
}

async fn focus(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let node = target.node(bound.target_id)?;
    send_cdp(
        transport,
        bound,
        "DOM.focus",
        json!({"backendNodeId":node.backend_node_id}),
        cancel,
        generation,
    )
    .await?;
    Ok(())
}

async fn clear_editable(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let backend_node_id = target.node(bound.target_id)?.backend_node_id;
    let resolved = send_cdp(
        transport,
        bound,
        "DOM.resolveNode",
        json!({"backendNodeId": backend_node_id}),
        cancel,
        generation,
    )
    .await?;
    let object_id = resolved
        .pointer("/object/objectId")
        .or_else(|| resolved.pointer("/result/object/objectId"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            super::interaction::interaction_error(
                bound.target_id,
                "editable node cannot be resolved",
            )
        })?;
    send_cdp(
        transport,
        bound,
        "Runtime.callFunctionOn",
        json!({
            "objectId": object_id,
            "functionDeclaration": "function(){if(this instanceof HTMLInputElement||this instanceof HTMLTextAreaElement){this.select();return true;}if(this.isContentEditable){const r=document.createRange();r.selectNodeContents(this);const s=getSelection();s.removeAllRanges();s.addRange(r);return true;}return false;}",
            "returnByValue": true,
            "silent": true
        }),
        cancel,
        generation,
    )
    .await?;
    dispatch_named(
        transport,
        bound,
        NamedKey::Backspace,
        Modifiers::default(),
        cancel,
        generation,
    )
    .await?;
    let checked = send_cdp(
        transport,
        bound,
        "Runtime.callFunctionOn",
        json!({
            "objectId": object_id,
            "functionDeclaration": "function(){return this instanceof HTMLInputElement||this instanceof HTMLTextAreaElement?this.value.length:(this.textContent||'').length;}",
            "returnByValue": true,
            "silent": true
        }),
        cancel,
        generation,
    )
    .await?;
    let remaining = checked
        .pointer("/result/value")
        .or_else(|| checked.pointer("/result/result/value"))
        .and_then(Value::as_u64);
    if remaining != Some(0) {
        return Err(super::interaction::interaction_error(
            bound.target_id,
            "reference_not_actionable: editable contents could not be cleared",
        ));
    }
    Ok(())
}

async fn dispatch_named(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    named: NamedKey,
    modifiers: Modifiers,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let dispatch = KEY_DISPATCH
        .iter()
        .find_map(|(candidate, dispatch)| (*candidate == named).then_some(*dispatch))
        .expect("every named key is mapped");
    let text = match named {
        NamedKey::Enter => Some("\r"),
        NamedKey::Space => Some(" "),
        _ => None,
    };
    let emits_text = text.is_some() && !modifiers.control && !modifiers.meta && !modifiers.alt;
    dispatch_key_event(
        transport,
        bound,
        if emits_text { "keyDown" } else { "rawKeyDown" },
        dispatch.key,
        dispatch.code,
        dispatch.location,
        dispatch.keycode,
        text.filter(|_| emits_text),
        modifiers,
        cancel,
        generation,
    )
    .await?;
    dispatch_key_event(
        transport,
        bound,
        "keyUp",
        dispatch.key,
        dispatch.code,
        dispatch.location,
        dispatch.keycode,
        None,
        modifiers,
        cancel,
        generation,
    )
    .await
}

async fn dispatch_char_key(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    ch: char,
    modifiers: Modifiers,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let ch = if modifiers.shift && ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase()
    } else {
        ch
    };
    let text = ch.to_string();
    let code = if ch.is_ascii_alphabetic() {
        format!("Key{}", ch.to_ascii_uppercase())
    } else {
        String::new()
    };
    let keycode = ch.to_ascii_uppercase() as u32;
    let emits_text = !modifiers.control && !modifiers.meta && !modifiers.alt;
    dispatch_key_event(
        transport,
        bound,
        if emits_text { "keyDown" } else { "rawKeyDown" },
        &text,
        &code,
        0,
        keycode as u16,
        emits_text.then_some(text.as_str()),
        modifiers,
        cancel,
        generation,
    )
    .await?;
    dispatch_key_event(
        transport,
        bound,
        "keyUp",
        &text,
        &code,
        0,
        keycode as u16,
        None,
        modifiers,
        cancel,
        generation,
    )
    .await
}

async fn dispatch_modifier(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    modifier: Modifier,
    down: bool,
    modifiers: Modifiers,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let (key_name, code, keycode) = match modifier {
        Modifier::Alt => ("Alt", "AltLeft", 18),
        Modifier::Control => ("Control", "ControlLeft", 17),
        Modifier::Shift => ("Shift", "ShiftLeft", 16),
        Modifier::Meta => ("Meta", "MetaLeft", 91),
    };
    dispatch_key_event(
        transport,
        bound,
        if down { "rawKeyDown" } else { "keyUp" },
        key_name,
        code,
        1,
        keycode,
        None,
        modifiers,
        cancel,
        generation,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_key_event(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    event_type: &str,
    key: &str,
    code: &str,
    location: u8,
    keycode: u16,
    text: Option<&str>,
    modifiers: Modifiers,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let mut params = Map::new();
    params.insert("type".into(), Value::String(event_type.to_owned()));
    params.insert("key".into(), Value::String(key.to_owned()));
    params.insert("code".into(), Value::String(code.to_owned()));
    params.insert("location".into(), json!(location));
    params.insert("windowsVirtualKeyCode".into(), json!(keycode));
    params.insert("nativeVirtualKeyCode".into(), json!(keycode));
    params.insert("modifiers".into(), json!(modifier_mask(modifiers)));
    if let Some(text) = text {
        params.insert("text".into(), Value::String(text.to_owned()));
        params.insert("unmodifiedText".into(), Value::String(text.to_owned()));
    }
    send_cdp(
        transport,
        bound,
        "Input.dispatchKeyEvent",
        Value::Object(params),
        cancel,
        generation,
    )
    .await?;
    Ok(())
}

fn set_modifier(value: &mut Modifiers, modifier: Modifier, active: bool) {
    match modifier {
        Modifier::Alt => value.alt = active,
        Modifier::Control => value.control = active,
        Modifier::Shift => value.shift = active,
        Modifier::Meta => value.meta = active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_closed_named_key_has_one_cdp_mapping() {
        for named in NamedKey::ALL {
            assert_eq!(
                KEY_DISPATCH
                    .iter()
                    .filter(|(candidate, _)| candidate == named)
                    .count(),
                1
            );
        }
        assert_eq!(KEY_DISPATCH.len(), NamedKey::ALL.len());
    }

    #[test]
    fn modifier_bits_match_the_cdp_input_contract() {
        assert_eq!(
            modifier_mask(Modifiers {
                alt: true,
                control: true,
                shift: true,
                meta: true,
            }),
            15
        );
    }
}

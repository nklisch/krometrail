use std::collections::{HashMap, HashSet};

use krometrail_core::{
    AccessibleProperty, AccessibleValue, BrowserOperationResult, ErrorCode, NodeReference,
    ObservationContext, PageSnapshot, Result, SnapshotGeneration, SnapshotNode, SnapshotNodeId,
    SnapshotPageRequest, TargetId,
};
use serde_json::{Value, json};

use super::{BoundTarget, PageControl, malformed, operation_error, transport_error};
use crate::transport::{CdpTransport, CommandScope, TransportError};

const MAX_SNAPSHOT_NODES: usize = 5_000;
const MAX_SNAPSHOT_TEXT_BYTES: usize = 1 << 20;
const MAX_ACCESSIBLE_PROPERTY_COUNT: usize = 32;

const ACCESSIBLE_PROPERTIES: &[&str] = &[
    "disabled",
    "editable",
    "expanded",
    "focused",
    "focusable",
    "haspopup",
    "invalid",
    "level",
    "multiline",
    "multiselectable",
    "orientation",
    "pressed",
    "readonly",
    "required",
    "selected",
    "checked",
];
const ACTIONABLE_ROLES: &[&str] = &[
    "button",
    "checkbox",
    "combobox",
    "link",
    "listbox",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "radio",
    "scrollbar",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "textbox",
    "treeitem",
];
const ACTIONABLE_SIGNALS: &[&str] = &["focusable", "editable", "clickable"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DocumentFingerprint {
    frame_id: String,
    loader_id: String,
}

#[derive(Clone, Debug)]
struct NodeBinding {
    backend_node_id: i64,
}

#[derive(Clone, Debug)]
struct ActiveSnapshot {
    generation: SnapshotGeneration,
    attachment_generation: u64,
    document: DocumentFingerprint,
    bindings: HashMap<SnapshotNodeId, NodeBinding>,
}

#[derive(Default)]
struct TargetSnapshotRegistry {
    next_generation: u64,
    active: Option<ActiveSnapshot>,
}

#[derive(Default)]
pub(crate) struct SnapshotRegistry {
    targets: HashMap<TargetId, TargetSnapshotRegistry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceRequirement {
    Actionable,
    VisibleGeometry,
    Editable,
    Selectable,
    FileInput,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedNode {
    pub(crate) backend_node_id: i64,
    pub(crate) document_quad: [f64; 8],
}

impl PageControl {
    pub(super) async fn snapshot(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: SnapshotPageRequest,
        started_at: krometrail_core::SessionTime,
    ) -> Result<BrowserOperationResult> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let document = document_fingerprint(transport, &scope, bound.target_id).await?;
        let response = transport
            .send_raw(&scope, "Accessibility.getFullAXTree", json!({}))
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let generation = self.snapshots.next_generation(bound.target_id)?;
        let (nodes, bindings, omitted_node_count) =
            decode_ax_tree(&response, bound.target_id, generation)?;
        let completed_at = self.session_time()?;
        let context = ObservationContext::new(
            self.session_id,
            bound.target_id,
            bound.attachment_generation,
            started_at,
            completed_at,
        )?;
        let snapshot = PageSnapshot::new(context, generation, nodes, omitted_node_count)?;
        self.snapshots.install(
            bound.target_id,
            ActiveSnapshot {
                generation,
                attachment_generation: bound.attachment_generation,
                document,
                bindings,
            },
        );
        Ok(BrowserOperationResult::SnapshotPage(Box::new(snapshot)))
    }
}

impl SnapshotRegistry {
    fn next_generation(&mut self, target_id: TargetId) -> Result<SnapshotGeneration> {
        let target = self.targets.entry(target_id).or_default();
        let next = target.next_generation.checked_add(1).ok_or_else(|| {
            operation_error(
                ErrorCode::PageObservationFailed,
                target_id,
                "snapshot generation space is exhausted",
            )
        })?;
        SnapshotGeneration::new(next)
    }

    fn install(&mut self, target_id: TargetId, active: ActiveSnapshot) {
        let target = self.targets.entry(target_id).or_default();
        target.next_generation = active.generation.get();
        target.active = Some(active);
    }

    pub(crate) fn retain_targets(&mut self, live: impl Iterator<Item = TargetId>) {
        let live = live.collect::<HashSet<_>>();
        self.targets.retain(|target, _| live.contains(target));
    }

    pub(crate) fn invalidate_target(&mut self, target_id: TargetId) {
        if let Some(target) = self.targets.get_mut(&target_id) {
            target.active = None;
        }
    }

    pub(crate) async fn resolve(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        reference: NodeReference,
        requirement: ReferenceRequirement,
    ) -> Result<ResolvedNode> {
        if reference.target_id != bound.target_id {
            return Err(stale(
                bound.target_id,
                "reference belongs to another target",
            ));
        }
        let active = self
            .targets
            .get(&bound.target_id)
            .and_then(|target| target.active.as_ref())
            .ok_or_else(|| stale(bound.target_id, "target has no active snapshot"))?;
        if active.generation != reference.generation {
            return Err(stale(
                bound.target_id,
                "snapshot generation is no longer active",
            ));
        }
        if active.attachment_generation != bound.attachment_generation {
            return Err(stale(
                bound.target_id,
                "target attachment changed after the snapshot",
            ));
        }
        let scope = CommandScope::Session(bound.transport_session.clone());
        let current = document_fingerprint(transport, &scope, bound.target_id).await?;
        if current != active.document {
            return Err(stale(
                bound.target_id,
                "document changed after the snapshot",
            ));
        }
        let binding = active.bindings.get(&reference.node_id).ok_or_else(|| {
            stale(
                bound.target_id,
                "snapshot node has no backing document node",
            )
        })?;
        resolve_backend_node(
            transport,
            &scope,
            bound.target_id,
            binding.backend_node_id,
            requirement,
        )
        .await
    }

    pub(crate) async fn resolve_selector(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        selector: &str,
        requirement: ReferenceRequirement,
    ) -> Result<ResolvedNode> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let document = transport
            .send_raw(
                &scope,
                "DOM.getDocument",
                json!({"depth": 0, "pierce": true}),
            )
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let root_node_id = document
            .pointer("/root/nodeId")
            .or_else(|| document.pointer("/result/root/nodeId"))
            .and_then(Value::as_i64)
            .ok_or_else(|| malformed(bound.target_id, "document root response is malformed"))?;
        let query = transport
            .send_raw(
                &scope,
                "DOM.querySelector",
                json!({"nodeId": root_node_id, "selector": selector}),
            )
            .await
            .map_err(|error| {
                let code = if error == TransportError::CommandFailed {
                    ErrorCode::InvalidInput
                } else {
                    ErrorCode::PageObservationFailed
                };
                transport_error(error, code, bound.target_id)
            })?;
        let node_id = query
            .get("nodeId")
            .or_else(|| query.pointer("/result/nodeId"))
            .and_then(Value::as_i64)
            .ok_or_else(|| malformed(bound.target_id, "selector response is malformed"))?;
        if node_id == 0 {
            return Err(operation_error(
                ErrorCode::NotFound,
                bound.target_id,
                "CSS selector did not match an element",
            ));
        }
        let described = transport
            .send_raw(&scope, "DOM.describeNode", json!({"nodeId": node_id}))
            .await
            .map_err(|_| stale(bound.target_id, "selected node is no longer available"))?;
        let backend = described
            .pointer("/node/backendNodeId")
            .or_else(|| described.pointer("/result/node/backendNodeId"))
            .and_then(Value::as_i64)
            .ok_or_else(|| stale(bound.target_id, "selected node has no backing identity"))?;
        resolve_backend_node(transport, &scope, bound.target_id, backend, requirement).await
    }
}

async fn document_fingerprint(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
) -> Result<DocumentFingerprint> {
    let response = transport
        .send_raw(scope, "Page.getFrameTree", json!({}))
        .await
        .map_err(|error| transport_error(error, ErrorCode::PageObservationFailed, target_id))?;
    let frame = response
        .pointer("/frameTree/frame")
        .or_else(|| response.pointer("/result/frameTree/frame"))
        .ok_or_else(|| malformed(target_id, "main frame response is malformed"))?;
    let frame_id = frame
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| malformed(target_id, "main frame id is missing"))?;
    let loader_id = frame
        .get("loaderId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| malformed(target_id, "main frame loader id is missing"))?;
    Ok(DocumentFingerprint {
        frame_id: frame_id.to_owned(),
        loader_id: loader_id.to_owned(),
    })
}

async fn resolve_backend_node(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    requirement: ReferenceRequirement,
) -> Result<ResolvedNode> {
    let described = transport
        .send_raw(
            scope,
            "DOM.describeNode",
            json!({"backendNodeId": backend_node_id}),
        )
        .await
        .map_err(|_| stale(target_id, "backing node no longer exists"))?;
    let described_backend = described
        .pointer("/node/backendNodeId")
        .or_else(|| described.pointer("/result/node/backendNodeId"))
        .and_then(Value::as_i64)
        .ok_or_else(|| stale(target_id, "backing node response is malformed"))?;
    if described_backend != backend_node_id {
        return Err(stale(target_id, "backing node identity changed"));
    }
    let resolved = transport
        .send_raw(
            scope,
            "DOM.resolveNode",
            json!({"backendNodeId": backend_node_id}),
        )
        .await
        .map_err(|_| stale(target_id, "backing node cannot be resolved"))?;
    let object_id = resolved
        .pointer("/object/objectId")
        .or_else(|| resolved.pointer("/result/object/objectId"))
        .and_then(Value::as_str)
        .ok_or_else(|| stale(target_id, "backing node has no live runtime object"))?;
    let check = transport.send_raw(scope, "Runtime.callFunctionOn", json!({
        "objectId": object_id,
        // `inert`, native disabled state, and `aria-disabled` suppress interaction, not painting.
        // Keep them separate from actual visibility so screenshot-only resolution can still crop
        // a visible control. The parent walk captures inherited light-DOM inertness without a
        // selector query that Chrome's side-effect analysis may conservatively refuse.
        "functionDeclaration": "function(){const s=getComputedStyle(this);let n=this,inert=false;while(n&&!inert){inert=n.inert===true;n=n.parentElement;}const tag=this.tagName;const type=tag==='INPUT'?(this.type||'text').toLowerCase():null;return {connected:this.isConnected,visuallyHidden:this.hidden||s.display==='none'||s.visibility==='hidden'||s.visibility==='collapse'||s.contentVisibility==='hidden',interactionBlocked:inert||this.disabled||this.getAttribute('aria-disabled')==='true',tagName:tag,inputType:type,isEditable:!this.readOnly&&!this.disabled&&(this.isContentEditable||(tag==='INPUT'&&/^(text|search|url|email|tel|password|number)$/.test(type))||tag==='TEXTAREA'),isSelect:tag==='SELECT',isFileInput:tag==='INPUT'&&type==='file'};}",
        "returnByValue": true,
        "throwOnSideEffect": true,
        "silent": true,
    })).await.map_err(|error| transport_error(error, ErrorCode::ReferenceNotActionable, target_id))?;
    let state = check
        .pointer("/result/value")
        .or_else(|| check.pointer("/result/result/value"))
        .ok_or_else(|| not_actionable(target_id, "node actionability response is malformed"))?;
    validate_node_state(state, requirement, target_id)?;
    let box_model = transport
        .send_raw(
            scope,
            "DOM.getBoxModel",
            json!({"backendNodeId": backend_node_id}),
        )
        .await
        .map_err(|_| not_actionable(target_id, "backing node has no visible geometry"))?;
    let quad = box_model
        .pointer("/model/border")
        .or_else(|| box_model.pointer("/result/model/border"))
        .and_then(Value::as_array)
        .ok_or_else(|| not_actionable(target_id, "backing node geometry is malformed"))?;
    if quad.len() != 8 {
        return Err(not_actionable(
            target_id,
            "backing node geometry is malformed",
        ));
    }
    let mut document_quad = [0.0; 8];
    for (output, value) in document_quad.iter_mut().zip(quad) {
        *output = value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| not_actionable(target_id, "backing node geometry is non-finite"))?;
    }
    let (min_x, max_x, min_y, max_y) = quad_bounds(&document_quad);
    if max_x <= min_x || max_y <= min_y {
        return Err(not_actionable(
            target_id,
            "backing node has zero-area geometry",
        ));
    }
    Ok(ResolvedNode {
        backend_node_id,
        document_quad,
    })
}

fn validate_node_state(
    state: &Value,
    requirement: ReferenceRequirement,
    target_id: TargetId,
) -> Result<()> {
    if state.get("connected").and_then(Value::as_bool) != Some(true) {
        return Err(stale(target_id, "backing node is detached"));
    }
    if state.get("visuallyHidden").and_then(Value::as_bool) != Some(false) {
        return Err(not_actionable(target_id, "backing node is hidden"));
    }
    if requirement != ReferenceRequirement::VisibleGeometry
        && state.get("interactionBlocked").and_then(Value::as_bool) != Some(false)
    {
        return Err(not_actionable(
            target_id,
            "backing node is inert, disabled, or aria-disabled",
        ));
    }
    let valid_kind = match requirement {
        ReferenceRequirement::VisibleGeometry | ReferenceRequirement::Actionable => true,
        ReferenceRequirement::Editable => {
            state.get("isEditable").and_then(Value::as_bool) == Some(true)
        }
        ReferenceRequirement::Selectable => {
            state.get("isSelect").and_then(Value::as_bool) == Some(true)
        }
        ReferenceRequirement::FileInput => {
            state.get("isFileInput").and_then(Value::as_bool) == Some(true)
        }
    };
    if !valid_kind {
        return Err(not_actionable(
            target_id,
            "backing node is not valid for the requested interaction",
        ));
    }
    Ok(())
}

pub(crate) fn quad_bounds(quad: &[f64; 8]) -> (f64, f64, f64, f64) {
    let xs = [quad[0], quad[2], quad[4], quad[6]];
    let ys = [quad[1], quad[3], quad[5], quad[7]];
    (
        xs.into_iter().fold(f64::INFINITY, f64::min),
        xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
        ys.into_iter().fold(f64::INFINITY, f64::min),
        ys.into_iter().fold(f64::NEG_INFINITY, f64::max),
    )
}

fn decode_ax_tree(
    response: &Value,
    target_id: TargetId,
    generation: SnapshotGeneration,
) -> Result<(Vec<SnapshotNode>, HashMap<SnapshotNodeId, NodeBinding>, u32)> {
    let raw_nodes = response
        .get("nodes")
        .or_else(|| response.pointer("/result/nodes"))
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(target_id, "accessibility tree response is malformed"))?;
    let by_id = raw_nodes
        .iter()
        .filter_map(|node| {
            node.get("nodeId")
                .and_then(Value::as_str)
                .map(|id| (id, node))
        })
        .collect::<HashMap<_, _>>();
    let children = raw_nodes
        .iter()
        .flat_map(|node| {
            node.get("childIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        })
        .collect::<HashSet<_>>();
    let roots = raw_nodes.iter().filter_map(|node| {
        node.get("nodeId")
            .and_then(Value::as_str)
            .filter(|id| !children.contains(id))
    });
    let mut decoder = Decoder {
        target_id,
        generation,
        by_id,
        nodes: Vec::new(),
        bindings: HashMap::new(),
        text_bytes: 0,
        omitted: 0,
        visited: HashSet::new(),
    };
    for root in roots {
        decoder.visit(root, None, 0)?;
    }
    for node in raw_nodes {
        if let Some(id) = node.get("nodeId").and_then(Value::as_str) {
            if !decoder.visited.contains(id) {
                decoder.visit(id, None, 0)?;
            }
        }
    }
    Ok((decoder.nodes, decoder.bindings, decoder.omitted))
}

struct Decoder<'a> {
    target_id: TargetId,
    generation: SnapshotGeneration,
    by_id: HashMap<&'a str, &'a Value>,
    nodes: Vec<SnapshotNode>,
    bindings: HashMap<SnapshotNodeId, NodeBinding>,
    text_bytes: usize,
    omitted: u32,
    visited: HashSet<&'a str>,
}

impl<'a> Decoder<'a> {
    fn visit(&mut self, id: &'a str, parent: Option<SnapshotNodeId>, depth: u16) -> Result<()> {
        if !self.visited.insert(id) {
            return Ok(());
        }
        let node = *self
            .by_id
            .get(id)
            .ok_or_else(|| malformed(self.target_id, "accessibility child node is missing"))?;
        let ignored = node
            .get("ignored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let role = ax_string(node.get("role")).unwrap_or("generic");
        let skip = ignored || matches!(role, "none" | "presentation");
        let mut next_parent = parent;
        let mut next_depth = depth;
        if !skip {
            let name = ax_owned(node.get("name"));
            let value = ax_owned(node.get("value"));
            let description = ax_owned(node.get("description"));
            let (properties, property_bytes, disabled, hidden, signal) = decode_properties(node);
            let text_bytes = role.len()
                + name.as_ref().map_or(0, String::len)
                + value.as_ref().map_or(0, String::len)
                + description.as_ref().map_or(0, String::len)
                + property_bytes;
            if self.nodes.len() >= MAX_SNAPSHOT_NODES
                || self.text_bytes.saturating_add(text_bytes) > MAX_SNAPSHOT_TEXT_BYTES
            {
                self.omitted = self.omitted.saturating_add(1);
            } else {
                let numeric = u32::try_from(self.nodes.len() + 1)
                    .map_err(|_| malformed(self.target_id, "snapshot node count overflow"))?;
                let node_id = SnapshotNodeId::new(numeric)?;
                let backend = node.get("backendDOMNodeId").and_then(Value::as_i64);
                let actionable = backend.is_some()
                    && !disabled
                    && !hidden
                    && (ACTIONABLE_ROLES.contains(&role) || signal);
                let reference = actionable.then_some(NodeReference {
                    target_id: self.target_id,
                    generation: self.generation,
                    node_id,
                });
                self.nodes.push(SnapshotNode {
                    id: node_id,
                    parent,
                    depth,
                    role: role.to_owned(),
                    name,
                    value,
                    description,
                    properties,
                    actionable,
                    reference,
                });
                if let Some(backend_node_id) = backend.filter(|_| actionable) {
                    self.bindings
                        .insert(node_id, NodeBinding { backend_node_id });
                }
                self.text_bytes += text_bytes;
                next_parent = Some(node_id);
                next_depth = depth
                    .checked_add(1)
                    .ok_or_else(|| malformed(self.target_id, "snapshot depth exceeds u16"))?;
            }
        }
        if let Some(child_ids) = node.get("childIds").and_then(Value::as_array) {
            for child in child_ids.iter().filter_map(Value::as_str) {
                self.visit(child, next_parent, next_depth)?;
            }
        }
        Ok(())
    }
}

fn decode_properties(node: &Value) -> (Vec<AccessibleProperty>, usize, bool, bool, bool) {
    let mut properties = Vec::new();
    let mut bytes = 0;
    let mut disabled = false;
    let mut hidden = false;
    let mut signal = false;
    for property in node
        .get("properties")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = property.get("name").and_then(Value::as_str) else {
            continue;
        };
        let value = property.get("value").and_then(|value| value.get("value"));
        let bool_value = value.and_then(Value::as_bool);
        disabled |= name == "disabled" && bool_value == Some(true);
        hidden |= name == "hidden" && bool_value == Some(true);
        signal |= ACTIONABLE_SIGNALS.contains(&name) && bool_value == Some(true);
        if !ACCESSIBLE_PROPERTIES.contains(&name)
            || properties.len() >= MAX_ACCESSIBLE_PROPERTY_COUNT
        {
            continue;
        }
        let accessible = match value {
            Some(Value::Bool(value)) => AccessibleValue::Boolean(*value),
            Some(Value::Number(value)) => match value.as_f64().filter(|value| value.is_finite()) {
                Some(value) => AccessibleValue::Number(value),
                None => continue,
            },
            Some(Value::String(value)) => AccessibleValue::Text(value.clone()),
            _ => continue,
        };
        bytes += name.len()
            + match &accessible {
                AccessibleValue::Text(value) => value.len(),
                _ => 0,
            };
        properties.push(AccessibleProperty {
            name: name.to_owned(),
            value: accessible,
        });
    }
    (properties, bytes, disabled, hidden, signal)
}

fn ax_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
}
fn ax_owned(value: Option<&Value>) -> Option<String> {
    ax_string(value)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
fn stale(target_id: TargetId, message: &'static str) -> krometrail_core::KrometrailError {
    operation_error(ErrorCode::StaleReference, target_id, message)
}
fn not_actionable(target_id: TargetId, message: &'static str) -> krometrail_core::KrometrailError {
    operation_error(ErrorCode::ReferenceNotActionable, target_id, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
    fn target() -> TargetId {
        TargetId::from_uuid(UUID.parse().unwrap())
    }

    #[test]
    fn additive_ax_fields_are_ignored_and_ignored_nodes_are_flattened() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let response = json!({"nodes":[
            {"nodeId":"root","ignored":false,"role":{"value":"document"},"childIds":["ignored"],"future":true},
            {"nodeId":"ignored","ignored":true,"role":{"value":"none"},"childIds":["button"]},
            {"nodeId":"button","ignored":false,"role":{"value":"button"},"name":{"value":"Save"},"backendDOMNodeId":7,"properties":[{"name":"focusable","value":{"value":true}},{"name":"futureProperty","value":{"value":"x"}}]}
        ]});
        let (nodes, bindings, omitted) = decode_ax_tree(&response, target(), generation).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[1].parent, Some(nodes[0].id));
        assert_eq!(nodes[1].name.as_deref(), Some("Save"));
        assert!(nodes[1].actionable);
        assert_eq!(bindings.len(), 1);
        assert_eq!(omitted, 0);
        assert_eq!(nodes[1].properties.len(), 1);
    }

    #[test]
    fn visible_geometry_ignores_interaction_only_inert_and_disabled_state() {
        let blocked_but_visible = json!({
            "connected": true,
            "visuallyHidden": false,
            "interactionBlocked": true,
        });
        assert!(
            validate_node_state(
                &blocked_but_visible,
                ReferenceRequirement::VisibleGeometry,
                target(),
            )
            .is_ok()
        );
        assert_eq!(
            validate_node_state(
                &blocked_but_visible,
                ReferenceRequirement::Actionable,
                target(),
            )
            .unwrap_err()
            .code,
            ErrorCode::ReferenceNotActionable
        );
    }

    #[test]
    fn action_specific_requirements_use_the_shared_dom_fact_set() {
        let state = json!({
            "connected": true,
            "visuallyHidden": false,
            "interactionBlocked": false,
            "isEditable": true,
            "isSelect": false,
            "isFileInput": false,
        });
        assert!(validate_node_state(&state, ReferenceRequirement::Editable, target()).is_ok());
        assert_eq!(
            validate_node_state(&state, ReferenceRequirement::Selectable, target())
                .unwrap_err()
                .code,
            ErrorCode::ReferenceNotActionable
        );
        assert_eq!(
            validate_node_state(&state, ReferenceRequirement::FileInput, target())
                .unwrap_err()
                .code,
            ErrorCode::ReferenceNotActionable
        );
        let select = json!({"connected":true,"visuallyHidden":false,"interactionBlocked":false,"isSelect":true});
        assert!(validate_node_state(&select, ReferenceRequirement::Selectable, target()).is_ok());
        let file = json!({"connected":true,"visuallyHidden":false,"interactionBlocked":false,"isFileInput":true});
        assert!(validate_node_state(&file, ReferenceRequirement::FileInput, target()).is_ok());
    }

    #[test]
    fn every_requirement_rejects_hidden_or_disconnected_nodes() {
        for requirement in [
            ReferenceRequirement::VisibleGeometry,
            ReferenceRequirement::Actionable,
            ReferenceRequirement::Editable,
            ReferenceRequirement::Selectable,
            ReferenceRequirement::FileInput,
        ] {
            let hidden = json!({
                "connected": true,
                "visuallyHidden": true,
                "interactionBlocked": false,
            });
            assert_eq!(
                validate_node_state(&hidden, requirement, target())
                    .unwrap_err()
                    .code,
                ErrorCode::ReferenceNotActionable
            );
            let disconnected = json!({
                "connected": false,
                "visuallyHidden": false,
                "interactionBlocked": false,
            });
            assert_eq!(
                validate_node_state(&disconnected, requirement, target())
                    .unwrap_err()
                    .code,
                ErrorCode::StaleReference
            );
        }
    }

    #[test]
    fn quad_bounds_rejects_neither_rotation_nor_document_offsets() {
        assert_eq!(
            quad_bounds(&[10.0, 20.0, 20.0, 10.0, 30.0, 20.0, 20.0, 30.0]),
            (10.0, 30.0, 10.0, 30.0)
        );
    }
}

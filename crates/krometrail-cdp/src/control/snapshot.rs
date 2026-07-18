use std::collections::{HashMap, HashSet};

use krometrail_core::{
    AccessibleProperty, AccessibleValue, BrowserOperationResult, CssPoint, CssRect, CssSize,
    CurrentReferenceGeometryRequest, ErrorCode, ErrorContext, MAX_SEMANTIC_QUERY_TEXT_BYTES,
    NodeReference, NonEmptyText, ObservationContext, PageSnapshot, QueryPageRequest,
    QueryPageResult, ResolvedReferenceGeometry, Result, SemanticMatch, SemanticQuery,
    SnapshotGeneration, SnapshotNode, SnapshotNodeId, SnapshotPageRequest, TargetId,
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
    pub(super) frame_id: String,
    pub(super) loader_id: String,
}

#[derive(Clone, Debug)]
struct ResolvedFrameDocument {
    reference: krometrail_core::PageFrameReference,
    fingerprint: DocumentFingerprint,
}

#[derive(Clone, Debug)]
struct NodeBinding {
    backend_node_id: i64,
}

#[derive(Clone, Debug, Default)]
struct SemanticNodeMetadata {
    labels: Vec<String>,
    rendered_text: String,
    test_id: Option<String>,
}

#[derive(Clone, Debug)]
struct ActiveSnapshot {
    generation: SnapshotGeneration,
    attachment_generation: u64,
    document: DocumentFingerprint,
    frame: Option<krometrail_core::PageFrameReference>,
    bindings: HashMap<SnapshotNodeId, NodeBinding>,
    node_by_backend: HashMap<i64, SnapshotNodeId>,
    semantic: HashMap<SnapshotNodeId, SemanticNodeMetadata>,
    parent_by_node: HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic_captured: bool,
    next_node_id: u32,
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
    pub(crate) async fn current_reference_geometry(
        &mut self,
        transport: &dyn CdpTransport,
        state: &crate::SupervisorState,
        request: CurrentReferenceGeometryRequest,
    ) -> Result<ResolvedReferenceGeometry> {
        if request.session_id != self.session_id {
            return Err(current_reference_error(
                request,
                ErrorCode::StaleReference,
                "reference belongs to another browser session",
            ));
        }

        self.retain_live_snapshot_targets(state);
        let bound = super::bind_target(
            state,
            krometrail_core::PageSelection::Target(request.reference.target_id),
        )
        .map_err(|_| {
            current_reference_error(
                request,
                ErrorCode::StaleReference,
                "reference target is not attached to the current browser session",
            )
        })?;
        let resolved = self
            .snapshots
            .resolve(
                transport,
                &bound,
                request.reference,
                ReferenceRequirement::VisibleGeometry,
            )
            .await
            .map_err(|error| current_reference_context(error, request))?;

        // Box quads are document CSS coordinates. Read the layout origin after resolving the
        // exact backing node, then subtract it once without viewport clipping. This samples a
        // fixed current region; it does not turn the reference into historical identity.
        let scope = CommandScope::Session(bound.transport_session.clone());
        let layout = transport
            .send_raw(&scope, "Page.getLayoutMetrics", json!({}))
            .await
            .map_err(|error| {
                current_reference_context(
                    transport_error(error, ErrorCode::PageObservationFailed, bound.target_id),
                    request,
                )
            })?;
        let layout_root = layout
            .get("result")
            .filter(|value| value.get("cssLayoutViewport").is_some())
            .unwrap_or(&layout);
        let viewport_origin = super::rect_from_viewport(
            layout_root.get("cssLayoutViewport"),
            "layout viewport",
            bound.target_id,
        )
        .map_err(|error| current_reference_context(error, request))?
        .origin;
        let (min_x, max_x, min_y, max_y) = quad_bounds(&resolved.document_quad);
        let viewport_css_rect = CssRect::new(
            CssPoint::new(min_x - viewport_origin.x, min_y - viewport_origin.y).map_err(|_| {
                current_reference_context(malformed_current_geometry(bound.target_id), request)
            })?,
            CssSize::new(max_x - min_x, max_y - min_y).map_err(|_| {
                current_reference_context(malformed_current_geometry(bound.target_id), request)
            })?,
        )
        .map_err(|_| {
            current_reference_context(malformed_current_geometry(bound.target_id), request)
        })?;
        let observed_at = self.clock.now();
        let resolved_at = self.session_origin.normalize(observed_at).map_err(|_| {
            current_reference_error(
                request,
                ErrorCode::PageObservationFailed,
                "current reference geometry timing is unavailable",
            )
        })?;
        ResolvedReferenceGeometry::new(
            request,
            bound.target_id,
            bound.attachment_generation,
            observed_at,
            resolved_at,
            viewport_css_rect,
        )
        .map_err(|_| {
            current_reference_error(
                request,
                ErrorCode::PageObservationFailed,
                "current reference geometry is malformed",
            )
        })
    }

    pub(super) async fn snapshot(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: SnapshotPageRequest,
        started_at: krometrail_core::SessionTime,
    ) -> Result<BrowserOperationResult> {
        self.capture_snapshot(transport, bound, started_at, false)
            .await
            .map(|snapshot| BrowserOperationResult::SnapshotPage(Box::new(snapshot)))
    }

    pub(super) async fn query_page(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: QueryPageRequest,
        started_at: krometrail_core::SessionTime,
    ) -> Result<BrowserOperationResult> {
        let frame = match &request.document {
            krometrail_core::SemanticDocumentScope::MainDocument => None,
            krometrail_core::SemanticDocumentScope::Frame(reference) => {
                Some(Self::resolve_frame_document(transport, bound, reference).await?)
            }
        };
        let snapshot = self
            .capture_snapshot_for_frame(transport, bound, started_at, true, frame.as_ref())
            .await?;
        let result = self.snapshots.query(bound, &request, &snapshot)?;
        Ok(BrowserOperationResult::QueryPage(Box::new(result)))
    }

    async fn capture_snapshot(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        started_at: krometrail_core::SessionTime,
        include_semantic: bool,
    ) -> Result<PageSnapshot> {
        self.capture_snapshot_for_frame(transport, bound, started_at, include_semantic, None)
            .await
    }

    async fn capture_snapshot_for_frame(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        started_at: krometrail_core::SessionTime,
        include_semantic: bool,
        frame: Option<&ResolvedFrameDocument>,
    ) -> Result<PageSnapshot> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let document = match frame {
            Some(frame) => frame.fingerprint.clone(),
            None => document_fingerprint(transport, &scope, bound.target_id).await?,
        };
        let ax_params = frame.map_or_else(
            || json!({}),
            |frame| json!({"frameId": frame.fingerprint.frame_id}),
        );
        let ax_response = transport
            .send_raw(&scope, "Accessibility.getFullAXTree", ax_params)
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let dom_response = if include_semantic {
            Some(
                transport
                    .send_raw(
                        &scope,
                        "DOMSnapshot.captureSnapshot",
                        json!({
                            "computedStyles": [],
                            "includePaintOrder": false,
                            "includeDOMRects": false,
                            "includeBlendedBackgroundColors": false,
                            "includeTextColorOpacities": false,
                        }),
                    )
                    .await
                    .map_err(|error| {
                        transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
                    })?,
            )
        } else {
            None
        };
        let (generation, mut node_by_backend, mut next_node_id) = self.snapshots.begin_snapshot(
            bound.target_id,
            bound.attachment_generation,
            &document,
        )?;
        let (nodes, bindings, omitted_node_count) = decode_ax_tree_with_ids(
            &ax_response,
            bound.target_id,
            generation,
            &mut node_by_backend,
            &mut next_node_id,
            Some(document.frame_id.as_str()),
        )?;
        let semantic = match dom_response {
            Some(response) => {
                let metadata = decode_dom_snapshot(&response, &document, bound.target_id)?;
                let current = match frame {
                    Some(frame) => {
                        Self::resolve_frame_document(transport, bound, &frame.reference)
                            .await?
                            .fingerprint
                    }
                    None => document_fingerprint(transport, &scope, bound.target_id).await?,
                };
                if current != document {
                    return Err(stale(
                        bound.target_id,
                        "document changed while capturing the semantic snapshot",
                    ));
                }
                metadata
                    .into_iter()
                    .filter_map(|(backend, metadata)| {
                        node_by_backend
                            .get(&backend)
                            .copied()
                            .map(|node_id| (node_id, metadata))
                    })
                    .collect()
            }
            None => HashMap::new(),
        };
        let parent_by_node = nodes.iter().map(|node| (node.id, node.parent)).collect();
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
                frame: frame.map(|frame| frame.reference.clone()),
                bindings,
                node_by_backend,
                semantic,
                parent_by_node,
                semantic_captured: include_semantic,
                next_node_id,
            },
        );
        Ok(snapshot)
    }

    async fn resolve_frame_document(
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        reference: &krometrail_core::PageFrameReference,
    ) -> Result<ResolvedFrameDocument> {
        let (frame_id, loader_id) = Self::resolve_frame_id(transport, bound, reference).await?;
        Ok(ResolvedFrameDocument {
            reference: reference.clone(),
            fingerprint: DocumentFingerprint {
                frame_id,
                loader_id,
            },
        })
    }
}

impl SnapshotRegistry {
    fn begin_snapshot(
        &mut self,
        target_id: TargetId,
        attachment_generation: u64,
        document: &DocumentFingerprint,
    ) -> Result<(SnapshotGeneration, HashMap<i64, SnapshotNodeId>, u32)> {
        let target = self.targets.entry(target_id).or_default();
        if let Some(active) = &target.active
            && active.attachment_generation == attachment_generation
            && &active.document == document
        {
            return Ok((
                active.generation,
                active.node_by_backend.clone(),
                active.next_node_id,
            ));
        }
        let next = target.next_generation.checked_add(1).ok_or_else(|| {
            operation_error(
                ErrorCode::PageObservationFailed,
                target_id,
                "snapshot generation space is exhausted",
            )
        })?;
        Ok((SnapshotGeneration::new(next)?, HashMap::new(), 0))
    }

    fn install(&mut self, target_id: TargetId, active: ActiveSnapshot) {
        let target = self.targets.entry(target_id).or_default();
        target.next_generation = active.generation.get();
        target.active = Some(active);
    }

    fn query(
        &self,
        bound: &BoundTarget,
        request: &QueryPageRequest,
        snapshot: &PageSnapshot,
    ) -> Result<QueryPageResult> {
        let active = self
            .targets
            .get(&bound.target_id)
            .and_then(|target| target.active.as_ref())
            .filter(|active| {
                active.generation == snapshot.generation
                    && active.attachment_generation == bound.attachment_generation
            })
            .ok_or_else(|| stale(bound.target_id, "semantic snapshot is no longer active"))?;
        if !active.semantic_captured {
            return Err(operation_error(
                ErrorCode::PageObservationFailed,
                bound.target_id,
                "semantic snapshot metadata is unavailable",
            ));
        }
        if snapshot.omitted_node_count != 0 {
            return Err(operation_error(
                ErrorCode::PageObservationFailed,
                bound.target_id,
                "semantic query requires a complete snapshot; omitted nodes could change the match outcome",
            ));
        }
        if let Some(scope) = request.scope {
            self.active_reference_backend(bound, scope)?;
        }

        let matches = snapshot
            .nodes
            .iter()
            .filter_map(|node| {
                let reference = node.reference?;
                if request.scope.is_some_and(|scope| {
                    !is_strict_descendant(node.id, scope.node_id, &active.parent_by_node)
                }) {
                    return None;
                }
                semantic_query_matches(
                    &request.query,
                    node,
                    active
                        .semantic
                        .get(&node.id)
                        .unwrap_or(&SemanticNodeMetadata::default()),
                    &active.parent_by_node,
                    &active.semantic,
                )
                .then(|| SemanticMatch {
                    reference,
                    role: node.role.clone(),
                    name: node.name.clone(),
                })
            })
            .collect();
        QueryPageResult::new(
            snapshot.context.clone(),
            snapshot.generation,
            matches,
            request.max_matches,
        )
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
        let scope = CommandScope::Session(bound.transport_session.clone());
        let backend = self
            .validated_reference_backend(transport, bound, reference)
            .await?;
        resolve_backend_node(transport, &scope, bound.target_id, backend, requirement).await
    }

    pub(crate) async fn resolve_selector(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        selector: &str,
        requirement: ReferenceRequirement,
    ) -> Result<ResolvedNode> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let backend = query_selector_backend(transport, &scope, bound.target_id, selector)
            .await?
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::NotFound,
                    bound.target_id,
                    "CSS selector did not match an element",
                )
            })?;
        resolve_backend_node(transport, &scope, bound.target_id, backend, requirement).await
    }

    /// Resolve through the same snapshot/selector authority as interactions, but stop before
    /// actionability checks so waits can truthfully observe hidden and disabled states.
    pub(crate) async fn resolve_wait_object(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        locator: &krometrail_core::ElementLocator,
    ) -> Result<Option<String>> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let backend = match locator {
            krometrail_core::ElementLocator::Reference(reference) => Some(
                self.validated_reference_backend(transport, bound, *reference)
                    .await?,
            ),
            krometrail_core::ElementLocator::CssSelector(selector) => {
                query_selector_backend(transport, &scope, bound.target_id, selector.as_str())
                    .await?
            }
        };
        match backend {
            Some(backend) => resolve_backend_object(transport, &scope, bound.target_id, backend)
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    async fn validated_reference_backend(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        reference: NodeReference,
    ) -> Result<i64> {
        let (document, backend) = self.active_reference_backend(bound, reference)?;
        let scope = CommandScope::Session(bound.transport_session.clone());
        let frame = self
            .targets
            .get(&bound.target_id)
            .and_then(|target| target.active.as_ref())
            .and_then(|active| active.frame.as_ref());
        let current = match frame {
            Some(frame) => {
                let (frame_id, loader_id) =
                    PageControl::resolve_frame_id(transport, bound, frame).await?;
                DocumentFingerprint {
                    frame_id,
                    loader_id,
                }
            }
            None => document_fingerprint(transport, &scope, bound.target_id).await?,
        };
        if current != *document {
            return Err(stale(
                bound.target_id,
                "document changed after the snapshot",
            ));
        }
        Ok(backend)
    }

    fn active_reference_backend(
        &self,
        bound: &BoundTarget,
        reference: NodeReference,
    ) -> Result<(&DocumentFingerprint, i64)> {
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
        let backend = active
            .bindings
            .get(&reference.node_id)
            .map(|binding| binding.backend_node_id)
            .ok_or_else(|| {
                stale(
                    bound.target_id,
                    "snapshot node has no backing document node",
                )
            })?;
        Ok((&active.document, backend))
    }
}

fn semantic_query_matches(
    query: &SemanticQuery,
    node: &SnapshotNode,
    metadata: &SemanticNodeMetadata,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic: &HashMap<SnapshotNodeId, SemanticNodeMetadata>,
) -> bool {
    match query {
        SemanticQuery::Role {
            role,
            name,
            container_text,
        } => {
            node.role == role.as_str()
                && name.as_ref().is_none_or(|name| {
                    node.name
                        .as_deref()
                        .is_some_and(|value| name.matches(value))
                })
                && container_text.as_ref().is_none_or(|expected| {
                    nearest_container_text_matches(node.id, expected, parents, semantic)
                })
        }
        SemanticQuery::Label { text } => metadata.labels.iter().any(|label| text.matches(label)),
        SemanticQuery::Text { text } => text.matches(&metadata.rendered_text),
        SemanticQuery::TestId { value } => metadata
            .test_id
            .as_deref()
            .is_some_and(|candidate| candidate == value.as_str()),
    }
}

fn nearest_container_text_matches(
    node: SnapshotNodeId,
    expected: &krometrail_core::SemanticTextMatch,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic: &HashMap<SnapshotNodeId, SemanticNodeMetadata>,
) -> bool {
    let mut current = parents.get(&node).copied().flatten();
    while let Some(ancestor) = current {
        // The AX root's rendered text is page-wide. Containers must be a bounded ancestor below
        // that root so unrelated page text cannot qualify a control.
        if parents.get(&ancestor).copied().flatten().is_none() {
            return false;
        }
        if semantic
            .get(&ancestor)
            .is_some_and(|metadata| expected.matches(&metadata.rendered_text))
        {
            return true;
        }
        current = parents.get(&ancestor).copied().flatten();
    }
    false
}

fn is_strict_descendant(
    candidate: SnapshotNodeId,
    scope: SnapshotNodeId,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
) -> bool {
    let mut current = parents.get(&candidate).copied().flatten();
    while let Some(node) = current {
        if node == scope {
            return true;
        }
        current = parents.get(&node).copied().flatten();
    }
    false
}

#[derive(Debug)]
struct DecodedDomNode {
    backend_node_id: i64,
    parent: Option<usize>,
    is_label: bool,
    label_for: Option<String>,
    aria_labelledby: Option<String>,
    test_id: Option<String>,
}

fn decode_dom_snapshot(
    response: &Value,
    document: &DocumentFingerprint,
    target_id: TargetId,
) -> Result<HashMap<i64, SemanticNodeMetadata>> {
    let root = response
        .get("result")
        .filter(|result| result.get("documents").is_some())
        .unwrap_or(response);
    let strings = root
        .get("strings")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(target_id, "DOM snapshot string table is malformed"))?;
    if strings.iter().any(|value| value.as_str().is_none()) {
        return Err(malformed(
            target_id,
            "DOM snapshot string table contains a non-string value",
        ));
    }
    let documents = root
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(target_id, "DOM snapshot documents are malformed"))?;
    let document = documents
        .iter()
        .find(|candidate| {
            candidate
                .get("frameId")
                .and_then(Value::as_u64)
                .and_then(|index| snapshot_string(strings, index).ok())
                == Some(document.frame_id.as_str())
        })
        .ok_or_else(|| malformed(target_id, "DOM snapshot does not contain the main document"))?;
    let nodes = document
        .get("nodes")
        .ok_or_else(|| malformed(target_id, "DOM snapshot node table is missing"))?;
    let backend_ids = required_array(nodes, "backendNodeId", target_id)?;
    if backend_ids.len() > MAX_SNAPSHOT_NODES {
        return Err(malformed(
            target_id,
            "DOM snapshot exceeds the 5000-node semantic limit",
        ));
    }
    let node_count = backend_ids.len();
    let parents = required_parallel_array(nodes, "parentIndex", node_count, target_id)?;
    let node_names = required_parallel_array(nodes, "nodeName", node_count, target_id)?;
    let attributes = required_parallel_array(nodes, "attributes", node_count, target_id)?;
    let mut text_bytes = 0_usize;
    let mut decoded = Vec::with_capacity(node_count);
    let mut id_to_index = HashMap::new();
    for index in 0..node_count {
        let backend_node_id = backend_ids[index]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(|| malformed(target_id, "DOM snapshot backend node id is invalid"))?;
        let parent = match parents[index].as_i64() {
            Some(-1) => None,
            Some(value)
                if value >= 0 && usize::try_from(value).is_ok_and(|value| value < index) =>
            {
                Some(usize::try_from(value).expect("validated parent index"))
            }
            _ => return Err(malformed(target_id, "DOM snapshot parent index is invalid")),
        };
        let node_name = snapshot_string_value(strings, &node_names[index], target_id)
            .map_err(|_| malformed(target_id, "DOM snapshot node-name string index is invalid"))?;
        let attrs = attributes[index]
            .as_array()
            .filter(|values| values.len() % 2 == 0)
            .ok_or_else(|| malformed(target_id, "DOM snapshot attributes are malformed"))?;
        let mut id = None;
        let mut label_for = None;
        let mut aria_labelledby = None;
        let mut test_id = None;
        for pair in attrs.chunks_exact(2) {
            let name = snapshot_string_value(strings, &pair[0], target_id).map_err(|_| {
                malformed(
                    target_id,
                    "DOM snapshot attribute-name string index is invalid",
                )
            })?;
            let destination = match name {
                "id" => &mut id,
                "for" => &mut label_for,
                "aria-labelledby" => &mut aria_labelledby,
                "data-testid" => &mut test_id,
                _ => continue,
            };
            let value = optional_snapshot_string_value(strings, &pair[1], target_id)
                .map_err(|_| {
                    malformed(
                        target_id,
                        "DOM snapshot attribute-value string index is invalid",
                    )
                })?
                .unwrap_or("");
            text_bytes = text_bytes.saturating_add(value.len());
            if text_bytes > MAX_SNAPSHOT_TEXT_BYTES {
                return Err(malformed(
                    target_id,
                    "DOM snapshot exceeds the semantic text limit",
                ));
            }
            *destination = bounded_semantic_value(value);
        }
        if let Some(id) = &id {
            id_to_index.entry(id.clone()).or_insert(index);
        }
        decoded.push(DecodedDomNode {
            backend_node_id,
            parent,
            is_label: node_name.eq_ignore_ascii_case("label"),
            label_for,
            aria_labelledby,
            test_id,
        });
    }

    let layout = document
        .get("layout")
        .ok_or_else(|| malformed(target_id, "DOM snapshot layout table is missing"))?;
    let layout_nodes = required_array(layout, "nodeIndex", target_id)?;
    let layout_text = required_parallel_array(layout, "text", layout_nodes.len(), target_id)?;
    let mut rendered = vec![String::new(); node_count];
    for (node_index, text) in layout_nodes.iter().zip(layout_text) {
        let node_index = node_index
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < node_count)
            .ok_or_else(|| malformed(target_id, "DOM snapshot layout node index is invalid"))?;
        let Some(text) =
            optional_snapshot_string_value(strings, text, target_id).map_err(|_| {
                malformed(
                    target_id,
                    "DOM snapshot layout-text string index is invalid",
                )
            })?
        else {
            continue;
        };
        text_bytes = text_bytes.saturating_add(text.len());
        if text_bytes > MAX_SNAPSHOT_TEXT_BYTES {
            return Err(malformed(
                target_id,
                "DOM snapshot exceeds the semantic text limit",
            ));
        }
        let mut ancestor = Some(node_index);
        while let Some(index) = ancestor {
            append_semantic_text(&mut rendered[index], text);
            ancestor = decoded[index].parent;
        }
    }

    let mut metadata = decoded
        .iter()
        .enumerate()
        .map(|(index, node)| {
            (
                node.backend_node_id,
                SemanticNodeMetadata {
                    labels: Vec::new(),
                    rendered_text: rendered[index].clone(),
                    test_id: node.test_id.clone(),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    for (label_index, label) in decoded.iter().enumerate().filter(|(_, node)| node.is_label) {
        let text = &rendered[label_index];
        if text.is_empty() {
            continue;
        }
        if let Some(target) = label
            .label_for
            .as_ref()
            .and_then(|value| id_to_index.get(value))
            .and_then(|index| decoded.get(*index))
        {
            push_label(&mut metadata, target.backend_node_id, text);
        }
    }
    for node in &decoded {
        let mut parent = node.parent;
        while let Some(parent_index) = parent {
            if decoded[parent_index].is_label {
                push_label(&mut metadata, node.backend_node_id, &rendered[parent_index]);
                break;
            }
            parent = decoded[parent_index].parent;
        }
        if let Some(labelledby) = &node.aria_labelledby {
            let mut composed = String::new();
            for id in labelledby.split_ascii_whitespace() {
                if let Some(label_index) = id_to_index.get(id) {
                    append_semantic_text(&mut composed, &rendered[*label_index]);
                }
            }
            push_label(&mut metadata, node.backend_node_id, &composed);
        }
    }
    Ok(metadata)
}

fn required_array<'a>(
    value: &'a Value,
    field: &str,
    target_id: TargetId,
) -> Result<&'a Vec<Value>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(target_id, "DOM snapshot parallel array is missing"))
}

fn required_parallel_array<'a>(
    value: &'a Value,
    field: &str,
    expected: usize,
    target_id: TargetId,
) -> Result<&'a Vec<Value>> {
    let values = required_array(value, field, target_id)?;
    if values.len() != expected {
        return Err(malformed(
            target_id,
            "DOM snapshot parallel arrays have inconsistent lengths",
        ));
    }
    Ok(values)
}

fn snapshot_string(strings: &[Value], index: u64) -> Result<&str> {
    usize::try_from(index)
        .ok()
        .and_then(|index| strings.get(index))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            krometrail_core::KrometrailError::new(
                ErrorCode::PageObservationFailed,
                NonEmptyText::new("DOM snapshot string index is invalid").unwrap(),
            )
        })
}

fn snapshot_string_value<'a>(
    strings: &'a [Value],
    value: &Value,
    target_id: TargetId,
) -> Result<&'a str> {
    value
        .as_u64()
        .ok_or_else(|| malformed(target_id, "DOM snapshot string index is invalid"))
        .and_then(|index| {
            snapshot_string(strings, index)
                .map_err(|_| malformed(target_id, "DOM snapshot string index is invalid"))
        })
}

fn optional_snapshot_string_value<'a>(
    strings: &'a [Value],
    value: &Value,
    target_id: TargetId,
) -> Result<Option<&'a str>> {
    match value.as_i64() {
        Some(-1) => Ok(None),
        Some(index) if index >= 0 => snapshot_string(strings, index as u64)
            .map(Some)
            .map_err(|_| malformed(target_id, "DOM snapshot string index is invalid")),
        _ => Err(malformed(target_id, "DOM snapshot string index is invalid")),
    }
}

fn bounded_semantic_value(value: &str) -> Option<String> {
    (!value.is_empty() && value.len() <= MAX_SEMANTIC_QUERY_TEXT_BYTES).then(|| value.to_owned())
}

fn append_semantic_text(destination: &mut String, value: &str) {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || destination.len() >= MAX_SEMANTIC_QUERY_TEXT_BYTES {
        return;
    }
    let separator = usize::from(!destination.is_empty());
    let remaining = MAX_SEMANTIC_QUERY_TEXT_BYTES - destination.len();
    if remaining <= separator {
        return;
    }
    if separator == 1 {
        destination.push(' ');
    }
    let available = remaining - separator;
    let end = normalized
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(normalized.len()))
        .take_while(|index| *index <= available)
        .last()
        .unwrap_or(0);
    destination.push_str(&normalized[..end]);
}

fn push_label(
    metadata: &mut HashMap<i64, SemanticNodeMetadata>,
    backend_node_id: i64,
    value: &str,
) {
    if value.is_empty() {
        return;
    }
    let labels = &mut metadata.entry(backend_node_id).or_default().labels;
    if !labels.iter().any(|label| label == value) {
        labels.push(value.to_owned());
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

async fn query_selector_backend(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    selector: &str,
) -> Result<Option<i64>> {
    let document = transport
        .send_raw(
            scope,
            "DOM.getDocument",
            json!({"depth": 0, "pierce": true}),
        )
        .await
        .map_err(|error| transport_error(error, ErrorCode::PageObservationFailed, target_id))?;
    let root_node_id = document
        .pointer("/root/nodeId")
        .or_else(|| document.pointer("/result/root/nodeId"))
        .and_then(Value::as_i64)
        .ok_or_else(|| malformed(target_id, "document root response is malformed"))?;
    let query = transport
        .send_raw(
            scope,
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
            transport_error(error, code, target_id)
        })?;
    let node_id = query
        .get("nodeId")
        .or_else(|| query.pointer("/result/nodeId"))
        .and_then(Value::as_i64)
        .ok_or_else(|| malformed(target_id, "selector response is malformed"))?;
    if node_id == 0 {
        return Ok(None);
    }
    let described = transport
        .send_raw(scope, "DOM.describeNode", json!({"nodeId": node_id}))
        .await
        .map_err(|_| stale(target_id, "selected node is no longer available"))?;
    described
        .pointer("/node/backendNodeId")
        .or_else(|| described.pointer("/result/node/backendNodeId"))
        .and_then(Value::as_i64)
        .map(Some)
        .ok_or_else(|| stale(target_id, "selected node has no backing identity"))
}

async fn resolve_backend_object(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
) -> Result<String> {
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
    resolved
        .pointer("/object/objectId")
        .or_else(|| resolved.pointer("/result/object/objectId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| stale(target_id, "backing node has no live runtime object"))
}

async fn resolve_backend_node(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    requirement: ReferenceRequirement,
) -> Result<ResolvedNode> {
    let object_id = resolve_backend_object(transport, scope, target_id, backend_node_id).await?;
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

fn malformed_current_geometry(target_id: TargetId) -> krometrail_core::KrometrailError {
    operation_error(
        ErrorCode::PageObservationFailed,
        target_id,
        "current layout viewport geometry is malformed",
    )
    .with_recovery(
        NonEmptyText::new("request a new structured snapshot and retry with its reference")
            .expect("static current-reference recovery is non-empty"),
    )
}

fn current_reference_context(
    error: krometrail_core::KrometrailError,
    request: CurrentReferenceGeometryRequest,
) -> krometrail_core::KrometrailError {
    error
        .with_context(ErrorContext {
            session_id: Some(request.session_id),
            target_id: Some(request.reference.target_id),
            interaction_id: None,
            range: None,
        })
        .with_recovery(
            NonEmptyText::new("request a new structured snapshot and retry with its reference")
                .expect("static current-reference recovery is non-empty"),
        )
}

pub(crate) fn current_reference_error(
    request: CurrentReferenceGeometryRequest,
    code: ErrorCode,
    message: &'static str,
) -> krometrail_core::KrometrailError {
    current_reference_context(
        operation_error(code, request.reference.target_id, message),
        request,
    )
}

#[cfg(test)]
fn decode_ax_tree(
    response: &Value,
    target_id: TargetId,
    generation: SnapshotGeneration,
) -> Result<(Vec<SnapshotNode>, HashMap<SnapshotNodeId, NodeBinding>, u32)> {
    let mut node_by_backend = HashMap::new();
    let mut next_node_id = 0;
    decode_ax_tree_with_ids(
        response,
        target_id,
        generation,
        &mut node_by_backend,
        &mut next_node_id,
        None,
    )
}

fn decode_ax_tree_with_ids(
    response: &Value,
    target_id: TargetId,
    generation: SnapshotGeneration,
    node_by_backend: &mut HashMap<i64, SnapshotNodeId>,
    next_node_id: &mut u32,
    expected_frame_id: Option<&str>,
) -> Result<(Vec<SnapshotNode>, HashMap<SnapshotNodeId, NodeBinding>, u32)> {
    let raw_nodes = response
        .get("nodes")
        .or_else(|| response.pointer("/result/nodes"))
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(target_id, "accessibility tree response is malformed"))?;
    let observed_frame_ids = raw_nodes
        .iter()
        .filter_map(|node| node.get("frameId").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    let nodes: Vec<&Value> = raw_nodes
        .iter()
        .filter(|node| {
            expected_frame_id.is_none_or(|expected| {
                node.get("frameId")
                    .and_then(Value::as_str)
                    .is_none_or(|actual| actual == expected)
            })
        })
        .collect();
    if expected_frame_id.is_some()
        && !observed_frame_ids.is_empty()
        && !observed_frame_ids.contains(expected_frame_id.expect("checked expected frame"))
    {
        return Err(stale(
            target_id,
            "accessibility tree belongs to a different document than the resolved frame",
        ));
    }
    let by_id = nodes
        .iter()
        .copied()
        .filter_map(|node| {
            node.get("nodeId")
                .and_then(Value::as_str)
                .map(|id| (id, node))
        })
        .collect::<HashMap<_, _>>();
    let children = nodes
        .iter()
        .flat_map(|node| {
            node.get("childIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        })
        .collect::<HashSet<_>>();
    let roots = nodes.iter().filter_map(|node| {
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
        node_by_backend,
        next_node_id,
        seen_backends: HashSet::new(),
    };
    for root in roots {
        decoder.visit(root, None, 0)?;
    }
    for node in nodes {
        if let Some(id) = node.get("nodeId").and_then(Value::as_str) {
            if !decoder.visited.contains(id) {
                decoder.visit(id, None, 0)?;
            }
        }
    }
    decoder
        .node_by_backend
        .retain(|backend, _| decoder.seen_backends.contains(backend));
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
    node_by_backend: &'a mut HashMap<i64, SnapshotNodeId>,
    next_node_id: &'a mut u32,
    seen_backends: HashSet<i64>,
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
                let backend = node.get("backendDOMNodeId").and_then(Value::as_i64);
                let node_id = if let Some(backend_node_id) = backend {
                    self.seen_backends.insert(backend_node_id);
                    if let Some(id) = self.node_by_backend.get(&backend_node_id) {
                        *id
                    } else {
                        *self.next_node_id = self.next_node_id.checked_add(1).ok_or_else(|| {
                            malformed(self.target_id, "snapshot node identity space exhausted")
                        })?;
                        let id = SnapshotNodeId::new(*self.next_node_id)?;
                        self.node_by_backend.insert(backend_node_id, id);
                        id
                    }
                } else {
                    *self.next_node_id = self.next_node_id.checked_add(1).ok_or_else(|| {
                        malformed(self.target_id, "snapshot node identity space exhausted")
                    })?;
                    SnapshotNodeId::new(*self.next_node_id)?
                };
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
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use krometrail_core::{
        IdSource, IdValue, MonotonicClock, ObservedTime, SessionId, SessionOrigin,
    };

    use super::*;
    use crate::transport::{
        CdpTransport, CommandScope, TransportClose, TransportError, TransportEvents,
        TransportFuture, TransportSessionId,
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
    fn target() -> TargetId {
        TargetId::from_uuid(UUID.parse().unwrap())
    }

    #[derive(Default)]
    struct SnapshotTransport {
        calls: Mutex<Vec<(String, Value)>>,
        responses: Mutex<HashMap<String, VecDeque<std::result::Result<Value, TransportError>>>>,
    }

    impl SnapshotTransport {
        fn push(&self, method: &str, response: Value) {
            self.responses
                .lock()
                .unwrap()
                .entry(method.to_owned())
                .or_default()
                .push_back(Ok(response));
        }
    }

    struct EmptyEvents;

    impl TransportEvents for EmptyEvents {
        fn next(
            &mut self,
        ) -> TransportFuture<
            '_,
            std::result::Result<Option<crate::transport::NamedEvent>, TransportError>,
        > {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    impl CdpTransport for SnapshotTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            let response = self
                .responses
                .lock()
                .unwrap()
                .get_mut(method)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| Ok(json!({})));
            Box::pin(std::future::ready(response))
        }

        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            Box::pin(std::future::ready(Ok(
                Box::new(EmptyEvents) as Box<dyn TransportEvents>
            )))
        }

        fn close_reason(&self) -> Option<TransportClose> {
            None
        }

        fn is_closed(&self) -> bool {
            false
        }
    }

    struct TestClock;

    impl MonotonicClock for TestClock {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(0)
        }
    }

    struct TestIds;

    impl IdSource for TestIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(uuid::Uuid::from_u128(1))
        }
    }

    fn frame_bound() -> BoundTarget {
        BoundTarget {
            target_id: target(),
            browser_target_key: "target-a".into(),
            attachment_generation: 1,
            transport_session: TransportSessionId::new("session-a").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        }
    }

    fn page_control() -> PageControl {
        PageControl::new(
            Arc::new(TestClock),
            Arc::new(TestIds),
            SessionId::from_uuid(uuid::Uuid::from_u128(2)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
        )
    }

    fn frame_tree(loader_id: &str) -> Value {
        json!({"frameTree": {
            "frame": {"id":"main","loaderId":"main-loader","url":"https://example.test/"},
            "childFrames": [{
                "frame": {"id":"child","loaderId":loader_id,"url":"https://example.test/child"}
            }]
        }})
    }

    fn child_ax_tree() -> Value {
        json!({"nodes":[
            {"nodeId":"main-root","frameId":"main","ignored":false,"role":{"value":"document"},"childIds":["main-button"]},
            {"nodeId":"main-button","frameId":"main","ignored":false,"role":{"value":"button"},"name":{"value":"Main action"},"backendDOMNodeId":7,"properties":[{"name":"focusable","value":{"value":true}}]},
            {"nodeId":"child-root","frameId":"child","ignored":false,"role":{"value":"document"},"childIds":["child-heading"]},
            {"nodeId":"child-heading","frameId":"child","ignored":false,"role":{"value":"heading"},"name":{"value":"Nested heading"},"backendDOMNodeId":107,"properties":[{"name":"focusable","value":{"value":true}}]}
        ]})
    }

    fn multi_document_snapshot() -> Value {
        let strings = vec!["main", "child", "DIV", "H1", "#text", "Nested heading"];
        let document = |frame_id, backend_offset| {
            json!({
                "frameId": frame_id,
                "nodes": {
                    "parentIndex": [-1, 0, 1],
                    "nodeName": [2, 3, 4],
                    "backendNodeId": [1 + backend_offset, 2 + backend_offset, 3 + backend_offset],
                    "attributes": [[], [], []]
                },
                "layout": {"nodeIndex": [2], "text": [5]}
            })
        };
        json!({"strings": strings, "documents": [document(0, 0), document(1, 100)]})
    }

    fn script_frame_capture(transport: &SnapshotTransport, final_loader_id: &str) {
        for loader_id in ["child-loader", "child-loader", final_loader_id] {
            transport.push("Page.getFrameTree", frame_tree(loader_id));
            transport.push("Target.getTargets", json!({"targetInfos": []}));
        }
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
        transport.push("DOMSnapshot.captureSnapshot", multi_document_snapshot());
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
    fn ax_snapshot_selects_nodes_from_the_resolved_frame() {
        let response = json!({"nodes":[
            {"nodeId":"main-root","frameId":"main","ignored":false,"role":{"value":"document"},"childIds":["main-button"]},
            {"nodeId":"main-button","frameId":"main","ignored":false,"role":{"value":"button"},"name":{"value":"Main action"},"backendDOMNodeId":7,"properties":[{"name":"focusable","value":{"value":true}}]},
            {"nodeId":"child-root","frameId":"child","ignored":false,"role":{"value":"document"},"childIds":["child-heading"]},
            {"nodeId":"child-heading","frameId":"child","ignored":false,"role":{"value":"heading"},"name":{"value":"Nested heading"},"backendDOMNodeId":107,"properties":[{"name":"focusable","value":{"value":true}}]}
        ]});
        let mut node_by_backend = HashMap::new();
        let mut next_node_id = 0;
        let (nodes, bindings, omitted) = decode_ax_tree_with_ids(
            &response,
            target(),
            SnapshotGeneration::new(1).unwrap(),
            &mut node_by_backend,
            &mut next_node_id,
            Some("child"),
        )
        .unwrap();
        assert_eq!(omitted, 0);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[1].name.as_deref(), Some("Nested heading"));
        assert_eq!(bindings.len(), 1);
        assert_eq!(node_by_backend.len(), 1);
        assert!(node_by_backend.contains_key(&107));
    }

    #[tokio::test]
    async fn same_origin_frame_query_uses_the_matching_ax_and_dom_documents() {
        let transport = SnapshotTransport::default();
        script_frame_capture(&transport, "child-loader");
        let mut control = page_control();
        let bound = frame_bound();
        let frames = control.list_frames(&transport, &bound).await.unwrap();
        let child = frames.frames[1].reference.clone();
        let resolved = PageControl::resolve_frame_document(&transport, &bound, &child)
            .await
            .unwrap();
        let snapshot = control
            .capture_snapshot_for_frame(
                &transport,
                &bound,
                krometrail_core::SessionTime::ZERO,
                true,
                Some(&resolved),
            )
            .await
            .unwrap();
        let request = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            SemanticQuery::role(
                "heading",
                Some(
                    krometrail_core::SemanticTextMatch::new(
                        "Nested heading",
                        krometrail_core::SemanticTextMatchMode::Exact,
                        false,
                    )
                    .unwrap(),
                ),
            )
            .unwrap(),
            None,
            10,
        )
        .unwrap();
        let result = control
            .snapshots
            .query(&bound, &request, &snapshot)
            .unwrap();
        assert_eq!(
            result.outcome,
            krometrail_core::SemanticQueryOutcome::Unique
        );
        assert_eq!(result.matches[0].name.as_deref(), Some("Nested heading"));
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(method, params)| {
                    method == "Accessibility.getFullAXTree" && params["frameId"] == "child"
                })
        );
    }

    #[tokio::test]
    async fn frame_navigation_during_semantic_capture_returns_stale_evidence() {
        let transport = SnapshotTransport::default();
        script_frame_capture(&transport, "child-loader-after-navigation");
        let mut control = page_control();
        let bound = frame_bound();
        let frames = control.list_frames(&transport, &bound).await.unwrap();
        let child = frames.frames[1].reference.clone();
        let resolved = PageControl::resolve_frame_document(&transport, &bound, &child)
            .await
            .unwrap();
        assert_eq!(
            control
                .capture_snapshot_for_frame(
                    &transport,
                    &bound,
                    krometrail_core::SessionTime::ZERO,
                    true,
                    Some(&resolved),
                )
                .await
                .unwrap_err()
                .code,
            ErrorCode::StaleReference
        );
    }

    fn semantic_dom_snapshot() -> Value {
        let strings = vec![
            "main",
            "DIV",
            "BUTTON",
            "#text",
            "LABEL",
            "INPUT",
            "SPAN",
            "id",
            "scope",
            "save",
            "data-testid",
            "primary-action",
            "Save action",
            "for",
            "named-input",
            "Explicit label",
            "Wrapping label",
            "wrapped-action",
            "aria-caption",
            "Aria caption",
            "aria-labelledby",
            "aria-second-caption",
            "Second caption",
            "aria-caption aria-second-caption",
        ];
        let index = |value: &str| {
            strings
                .iter()
                .position(|candidate| *candidate == value)
                .unwrap()
        };
        json!({
            "strings": strings,
            "documents": [{
                "frameId": index("main"),
                "nodes": {
                    "parentIndex": [-1,0,1,2,1,4,1,6,1,8,8,10,1,12,1,14],
                    "nodeName": [
                        index("DIV"), index("DIV"), index("BUTTON"), index("#text"),
                        index("LABEL"), index("#text"), index("INPUT"), index("#text"),
                        index("LABEL"), index("#text"), index("INPUT"), index("#text"),
                        index("SPAN"), index("#text"), index("SPAN"), index("#text")
                    ],
                    "backendNodeId": [1,2,10,11,20,21,30,31,40,41,50,51,60,61,70,71],
                    "attributes": [
                        [], [index("id"),index("scope")],
                        [index("id"),index("save"),index("data-testid"),index("primary-action")],
                        [], [index("for"),index("named-input")], [],
                        [index("id"),index("named-input")], [],
                        [], [], [index("data-testid"),index("wrapped-action")], [],
                        [index("id"),index("aria-caption")], [],
                        [index("id"),index("aria-second-caption")], []
                    ]
                },
                "layout": {
                    "nodeIndex": [3,5,7,9,13,15],
                    "text": [
                        index("Save action"), index("Explicit label"), index("Save action"),
                        index("Wrapping label"), index("Aria caption"), index("Second caption")
                    ]
                }
            }]
        })
    }

    #[test]
    fn dom_snapshot_enriches_text_labels_and_test_identifiers() {
        let metadata = decode_dom_snapshot(
            &semantic_dom_snapshot(),
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
        )
        .unwrap();
        assert_eq!(metadata.get(&10).unwrap().rendered_text, "Save action");
        assert_eq!(
            metadata.get(&10).unwrap().test_id.as_deref(),
            Some("primary-action")
        );
        assert_eq!(metadata.get(&30).unwrap().labels, vec!["Explicit label"]);
        assert_eq!(metadata.get(&50).unwrap().labels, vec!["Wrapping label"]);

        let mut aria = semantic_dom_snapshot();
        let strings = aria["strings"].as_array_mut().unwrap();
        let labelledby_name = strings
            .iter()
            .position(|value| value == "aria-labelledby")
            .unwrap();
        let caption_id = strings
            .iter()
            .position(|value| value == "aria-caption")
            .unwrap();
        aria["documents"][0]["nodes"]["attributes"][10] = json!([labelledby_name, caption_id]);
        let metadata = decode_dom_snapshot(
            &aria,
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
        )
        .unwrap();
        assert_eq!(
            metadata.get(&50).unwrap().labels,
            vec!["Wrapping label", "Aria caption"]
        );

        let mut multi_id = semantic_dom_snapshot();
        let strings = multi_id["strings"].as_array().unwrap();
        let labelledby_name = strings
            .iter()
            .position(|value| value == "aria-labelledby")
            .unwrap();
        let composed_ids = strings
            .iter()
            .position(|value| value == "aria-caption aria-second-caption")
            .unwrap();
        multi_id["documents"][0]["nodes"]["attributes"][10] =
            json!([labelledby_name, composed_ids]);
        let metadata = decode_dom_snapshot(
            &multi_id,
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
        )
        .unwrap();
        assert_eq!(
            metadata.get(&50).unwrap().labels,
            vec!["Wrapping label", "Aria caption Second caption"]
        );
        assert!(
            krometrail_core::SemanticTextMatch::new(
                "Aria caption Second caption",
                krometrail_core::SemanticTextMatchMode::Exact,
                false,
            )
            .unwrap()
            .matches(&metadata.get(&50).unwrap().labels[1])
        );
    }

    #[test]
    fn dom_snapshot_selects_the_qualified_child_document() {
        let mut snapshot = semantic_dom_snapshot();
        let child_index = snapshot["strings"].as_array().unwrap().len();
        snapshot["strings"]
            .as_array_mut()
            .unwrap()
            .push(json!("child"));
        let mut child = snapshot["documents"][0].clone();
        child["frameId"] = json!(child_index);
        for backend in child["nodes"]["backendNodeId"].as_array_mut().unwrap() {
            *backend = json!(backend.as_i64().unwrap() + 100);
        }
        snapshot["documents"].as_array_mut().unwrap().push(child);

        let metadata = decode_dom_snapshot(
            &snapshot,
            &DocumentFingerprint {
                frame_id: "child".into(),
                loader_id: "child-loader".into(),
            },
            target(),
        )
        .unwrap();
        assert!(metadata.contains_key(&110));
        assert!(!metadata.contains_key(&10));
    }

    #[test]
    fn container_role_queries_use_only_bounded_ancestor_text() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let first_container = SnapshotNodeId::new(2).unwrap();
        let first_checkbox = SnapshotNodeId::new(3).unwrap();
        let second_container = SnapshotNodeId::new(4).unwrap();
        let second_checkbox = SnapshotNodeId::new(5).unwrap();
        let reference = |node_id| NodeReference {
            target_id: target(),
            generation,
            node_id,
        };
        let node = |id, parent, depth, role: &str, actionable| SnapshotNode {
            id,
            parent,
            depth,
            role: role.into(),
            name: None,
            value: None,
            description: None,
            properties: vec![],
            actionable,
            reference: actionable.then(|| reference(id)),
        };
        let snapshot = PageSnapshot::new(
            ObservationContext::new(
                krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(2)),
                target(),
                4,
                krometrail_core::SessionTime::ZERO,
                krometrail_core::SessionTime::ZERO,
            )
            .unwrap(),
            generation,
            vec![
                node(root, None, 0, "document", false),
                node(first_container, Some(root), 1, "listitem", false),
                node(first_checkbox, Some(first_container), 2, "checkbox", true),
                node(second_container, Some(root), 1, "listitem", false),
                node(second_checkbox, Some(second_container), 2, "checkbox", true),
            ],
            0,
        )
        .unwrap();
        let bound = BoundTarget {
            target_id: target(),
            browser_target_key: "target-a".into(),
            attachment_generation: 4,
            transport_session: crate::transport::TransportSessionId::new("session-a").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        };
        let mut registry = SnapshotRegistry::default();
        registry.install(
            target(),
            ActiveSnapshot {
                generation,
                attachment_generation: 4,
                document: DocumentFingerprint {
                    frame_id: "main".into(),
                    loader_id: "loader".into(),
                },
                frame: None,
                bindings: HashMap::from([
                    (first_checkbox, NodeBinding { backend_node_id: 3 }),
                    (second_checkbox, NodeBinding { backend_node_id: 5 }),
                ]),
                node_by_backend: HashMap::from([
                    (1, root),
                    (2, first_container),
                    (3, first_checkbox),
                    (4, second_container),
                    (5, second_checkbox),
                ]),
                semantic: HashMap::from([
                    (
                        root,
                        SemanticNodeMetadata {
                            rendered_text: "Page-wide unrelated text".into(),
                            ..Default::default()
                        },
                    ),
                    (
                        first_container,
                        SemanticNodeMetadata {
                            rendered_text: "Buy milk".into(),
                            ..Default::default()
                        },
                    ),
                    (
                        second_container,
                        SemanticNodeMetadata {
                            rendered_text: "Ship release".into(),
                            ..Default::default()
                        },
                    ),
                ]),
                parent_by_node: HashMap::from([
                    (root, None),
                    (first_container, Some(root)),
                    (first_checkbox, Some(first_container)),
                    (second_container, Some(root)),
                    (second_checkbox, Some(second_container)),
                ]),
                semantic_captured: true,
                next_node_id: 5,
            },
        );
        let request = |container_text| {
            QueryPageRequest::new(
                krometrail_core::PageSelection::Target(target()),
                SemanticQuery::role_in_container(
                    "checkbox",
                    None,
                    krometrail_core::SemanticTextMatch::new(
                        container_text,
                        krometrail_core::SemanticTextMatchMode::Exact,
                        false,
                    )
                    .unwrap(),
                )
                .unwrap(),
                None,
                20,
            )
            .unwrap()
        };
        assert_eq!(
            registry
                .query(&bound, &request("Buy milk"), &snapshot)
                .unwrap()
                .matches[0]
                .reference
                .node_id,
            first_checkbox
        );
        assert_eq!(
            registry
                .query(&bound, &request("Ship release"), &snapshot)
                .unwrap()
                .matches[0]
                .reference
                .node_id,
            second_checkbox
        );
        assert_eq!(
            registry
                .query(&bound, &request("Page-wide unrelated text"), &snapshot)
                .unwrap()
                .outcome,
            krometrail_core::SemanticQueryOutcome::NoMatch
        );
    }

    #[test]
    fn semantic_query_is_preordered_scoped_bounded_and_explicit() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let scope = SnapshotNodeId::new(2).unwrap();
        let first = SnapshotNodeId::new(3).unwrap();
        let second = SnapshotNodeId::new(4).unwrap();
        let outside = SnapshotNodeId::new(5).unwrap();
        let reference = |node_id| NodeReference {
            target_id: target(),
            generation,
            node_id,
        };
        let node = |id, parent, depth, name: &str| SnapshotNode {
            id,
            parent,
            depth,
            role: if id == root { "document" } else { "button" }.into(),
            name: (!name.is_empty()).then(|| name.to_owned()),
            value: None,
            description: None,
            properties: vec![],
            actionable: id != root,
            reference: (id != root).then(|| reference(id)),
        };
        let nodes = vec![
            node(root, None, 0, ""),
            node(scope, Some(root), 1, "Scope"),
            node(first, Some(scope), 2, "Duplicate"),
            node(second, Some(scope), 2, "Duplicate"),
            node(outside, Some(root), 1, "Duplicate"),
        ];
        let context = ObservationContext::new(
            krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(2)),
            target(),
            4,
            krometrail_core::SessionTime::ZERO,
            krometrail_core::SessionTime::ZERO,
        )
        .unwrap();
        let snapshot = PageSnapshot::new(context, generation, nodes, 0).unwrap();
        let bound = BoundTarget {
            target_id: target(),
            browser_target_key: "target-a".into(),
            attachment_generation: 4,
            transport_session: crate::transport::TransportSessionId::new("session-a").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        };
        let mut registry = SnapshotRegistry::default();
        registry.install(
            target(),
            ActiveSnapshot {
                generation,
                attachment_generation: 4,
                document: DocumentFingerprint {
                    frame_id: "main".into(),
                    loader_id: "loader".into(),
                },
                frame: None,
                bindings: HashMap::from([
                    (scope, NodeBinding { backend_node_id: 2 }),
                    (first, NodeBinding { backend_node_id: 3 }),
                    (second, NodeBinding { backend_node_id: 4 }),
                    (outside, NodeBinding { backend_node_id: 5 }),
                ]),
                node_by_backend: HashMap::from([(2, scope), (3, first), (4, second), (5, outside)]),
                semantic: HashMap::from([
                    (
                        first,
                        SemanticNodeMetadata {
                            test_id: Some("duplicate".into()),
                            ..Default::default()
                        },
                    ),
                    (
                        second,
                        SemanticNodeMetadata {
                            test_id: Some("duplicate".into()),
                            ..Default::default()
                        },
                    ),
                    (
                        outside,
                        SemanticNodeMetadata {
                            test_id: Some("duplicate".into()),
                            ..Default::default()
                        },
                    ),
                ]),
                parent_by_node: HashMap::from([
                    (root, None),
                    (scope, Some(root)),
                    (first, Some(scope)),
                    (second, Some(scope)),
                    (outside, Some(root)),
                ]),
                semantic_captured: true,
                next_node_id: 5,
            },
        );
        let query = SemanticQuery::test_id("duplicate").unwrap();
        let scoped = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            query.clone(),
            Some(reference(scope)),
            1,
        )
        .unwrap();
        let result = registry.query(&bound, &scoped, &snapshot).unwrap();
        assert_eq!(
            result.outcome,
            krometrail_core::SemanticQueryOutcome::Truncated
        );
        assert_eq!(result.matches[0].reference.node_id, first);
        assert_eq!(result.omitted_match_count, 1);

        let unscoped = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            query,
            None,
            20,
        )
        .unwrap();
        let result = registry.query(&bound, &unscoped, &snapshot).unwrap();
        assert_eq!(
            result.outcome,
            krometrail_core::SemanticQueryOutcome::Ambiguous
        );
        assert_eq!(
            result
                .matches
                .iter()
                .map(|candidate| candidate.reference.node_id)
                .collect::<Vec<_>>(),
            vec![first, second, outside]
        );

        let incomplete = PageSnapshot::new(
            snapshot.context.clone(),
            generation,
            snapshot
                .nodes
                .iter()
                .filter(|node| node.id != second)
                .cloned()
                .collect(),
            1,
        )
        .unwrap();
        let would_be_unique_with_omitted_matching_actionable_node = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            SemanticQuery::test_id("duplicate").unwrap(),
            Some(reference(scope)),
            20,
        )
        .unwrap();
        assert_eq!(
            registry
                .query(
                    &bound,
                    &would_be_unique_with_omitted_matching_actionable_node,
                    &incomplete,
                )
                .unwrap_err()
                .code,
            ErrorCode::PageObservationFailed
        );

        let stale_scope = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            SemanticQuery::test_id("missing").unwrap(),
            Some(NodeReference {
                generation: SnapshotGeneration::new(2).unwrap(),
                ..reference(scope)
            }),
            20,
        )
        .unwrap();
        assert_eq!(
            registry
                .query(&bound, &stale_scope, &snapshot)
                .unwrap_err()
                .code,
            ErrorCode::StaleReference
        );
    }

    #[test]
    fn malformed_dom_snapshot_fails_closed() {
        let mut malformed_snapshot = semantic_dom_snapshot();
        malformed_snapshot["documents"][0]["nodes"]["parentIndex"] = json!([-1]);
        assert_eq!(
            decode_dom_snapshot(
                &malformed_snapshot,
                &DocumentFingerprint {
                    frame_id: "main".into(),
                    loader_id: "loader".into(),
                },
                target(),
            )
            .unwrap_err()
            .code,
            ErrorCode::PageObservationFailed
        );
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

    #[test]
    fn exact_reference_authority_rejects_refresh_reconnect_close_scope_and_backing_drift() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let node_id = SnapshotNodeId::new(1).unwrap();
        let reference = NodeReference {
            target_id: target(),
            generation,
            node_id,
        };
        let bound = BoundTarget {
            target_id: target(),
            browser_target_key: "target-a".into(),
            attachment_generation: 4,
            transport_session: crate::transport::TransportSessionId::new("session-a").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        };
        let active = || ActiveSnapshot {
            generation,
            attachment_generation: 4,
            document: DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            frame: None,
            bindings: HashMap::from([(
                node_id,
                NodeBinding {
                    backend_node_id: 42,
                },
            )]),
            node_by_backend: HashMap::from([(42, node_id)]),
            semantic: HashMap::new(),
            parent_by_node: HashMap::new(),
            semantic_captured: false,
            next_node_id: 1,
        };
        let assert_stale = |error: krometrail_core::KrometrailError| {
            assert_eq!(error.code, ErrorCode::StaleReference);
            assert!(
                error
                    .recovery
                    .as_ref()
                    .is_some_and(|value| value.as_str().contains("new structured snapshot"))
            );
        };

        let mut registry = SnapshotRegistry::default();
        registry.install(target(), active());
        assert_eq!(
            registry
                .active_reference_backend(&bound, reference)
                .unwrap()
                .1,
            42
        );

        let wrong_target = NodeReference {
            target_id: TargetId::from_uuid(uuid::Uuid::from_u128(99)),
            ..reference
        };
        assert_stale(
            registry
                .active_reference_backend(&bound, wrong_target)
                .unwrap_err(),
        );

        let mut reattached = bound.clone();
        reattached.attachment_generation += 1;
        assert_stale(
            registry
                .active_reference_backend(&reattached, reference)
                .unwrap_err(),
        );

        let refreshed_generation = SnapshotGeneration::new(2).unwrap();
        registry.install(
            target(),
            ActiveSnapshot {
                generation: refreshed_generation,
                ..active()
            },
        );
        assert_stale(
            registry
                .active_reference_backend(&bound, reference)
                .unwrap_err(),
        );

        let refreshed = NodeReference {
            generation: refreshed_generation,
            ..reference
        };
        assert_stale(
            registry
                .active_reference_backend(
                    &bound,
                    NodeReference {
                        node_id: SnapshotNodeId::new(2).unwrap(),
                        ..refreshed
                    },
                )
                .unwrap_err(),
        );

        registry.invalidate_target(target());
        assert_stale(
            registry
                .active_reference_backend(&bound, refreshed)
                .unwrap_err(),
        );
    }

    #[test]
    fn same_document_snapshot_reuses_generation_and_backend_identity() {
        let mut registry = SnapshotRegistry::default();
        let document = DocumentFingerprint {
            frame_id: "main".into(),
            loader_id: "loader".into(),
        };
        let generation = SnapshotGeneration::new(1).unwrap();
        let node_id = SnapshotNodeId::new(7).unwrap();
        registry.install(
            target(),
            ActiveSnapshot {
                generation,
                attachment_generation: 4,
                document: document.clone(),
                frame: None,
                bindings: HashMap::new(),
                node_by_backend: HashMap::from([(42, node_id)]),
                semantic: HashMap::new(),
                parent_by_node: HashMap::new(),
                semantic_captured: false,
                next_node_id: 7,
            },
        );

        let (next_generation, node_by_backend, next_node_id) =
            registry.begin_snapshot(target(), 4, &document).unwrap();
        assert_eq!(next_generation, generation);
        assert_eq!(node_by_backend.get(&42), Some(&node_id));
        assert_eq!(next_node_id, 7);
    }
}

use std::collections::{HashMap, HashSet};

use krometrail_core::{
    AccessibleProperty, AccessibleValue, BrowserOperationResult, CssPoint, CssRect, CssSize,
    CurrentReferenceGeometryRequest, ErrorCode, ErrorContext, ExpectationTargetRole,
    KrometrailError, MAX_SEMANTIC_QUERY_TEXT_BYTES, NodeReference, NodeStateFacts, NonEmptyText,
    ObservationContext, PageSnapshot, QueryPageRequest, QueryPageResult, RelaxedMatchCandidates,
    ResolvedReferenceGeometry, Result, RetryAdvice, SemanticMatch, SemanticQuery,
    SemanticQueryOutcome, SnapshotGeneration, SnapshotNode, SnapshotNodeId, SnapshotPageAnchor,
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
const STRUCTURAL_WEB_AREA_ROLES: &[&str] = &["rootwebarea", "webarea", "document"];
const LOCAL_CONTAINER_ROLES: &[&str] = &[
    "listitem",
    "row",
    "cell",
    "gridcell",
    "group",
    "article",
    "region",
    "label",
    "labeltext",
];
const GENERIC_CONTAINER_ROLES: &[&str] = &["generic", "none", "presentation"];

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
    expectation_role: ExpectationTargetRole,
}

#[derive(Clone, Debug, Default)]
struct SemanticNodeMetadata {
    labels: Vec<String>,
    rendered_text: String,
    /// The normalized rendered-text length before the agent-facing value was bounded.
    ///
    /// DOM snapshots retain only a bounded prefix for matching and diagnostics, but generic
    /// ancestor eligibility must still see the true collapsed size so a page-scale wrapper cannot
    /// qualify a control merely because its prefix fits the bound.
    collapsed_text_bytes: usize,
    test_id: Option<String>,
}

impl SemanticNodeMetadata {
    fn true_collapsed_text_bytes(&self) -> usize {
        self.collapsed_text_bytes
            .max(krometrail_core::collapsed_semantic_text_bytes(
                &self.rendered_text,
            ))
    }
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
    dom_semantics_captured: bool,
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

#[derive(Debug)]
pub(super) struct SemanticPresenceProbe {
    pub outcome: SemanticQueryOutcome,
    pub match_count: u32,
    pub relaxed_match_candidates: Option<RelaxedMatchCandidates>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceRequirement {
    Actionable,
    VisibleGeometry,
    Editable,
    Selectable,
    FileInput,
}

impl ReferenceRequirement {
    /// File uploads act on a backend node id and do not need paint or box geometry.
    pub(crate) const fn requires_visible_geometry(self) -> bool {
        !matches!(self, Self::FileInput)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TemporalInputKind {
    Date,
    Time,
    DatetimeLocal,
    Month,
    Week,
}

impl TemporalInputKind {
    pub(crate) fn from_input_type(input_type: &str) -> Option<Self> {
        match input_type {
            "date" => Some(Self::Date),
            "time" => Some(Self::Time),
            "datetime-local" => Some(Self::DatetimeLocal),
            "month" => Some(Self::Month),
            "week" => Some(Self::Week),
            _ => None,
        }
    }

    pub(crate) const fn input_type(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Time => "time",
            Self::DatetimeLocal => "datetime-local",
            Self::Month => "month",
            Self::Week => "week",
        }
    }

    pub(crate) const fn expected_format(self) -> &'static str {
        match self {
            Self::Date => "YYYY-MM-DD",
            Self::Time => "HH:MM[:SS]",
            Self::DatetimeLocal => "YYYY-MM-DDTHH:MM[:SS]",
            Self::Month => "YYYY-MM",
            Self::Week => "YYYY-Www",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedNode {
    pub(crate) backend_node_id: i64,
    pub(crate) document_quad: Option<[f64; 8]>,
    pub(crate) facts: NodeStateFacts,
    pub(crate) expectation_role: Option<ExpectationTargetRole>,
    pub(crate) temporal_input: Option<TemporalInputKind>,
}

impl ResolvedNode {
    pub(crate) fn geometry(&self, target_id: TargetId) -> Result<&[f64; 8]> {
        self.document_quad
            .as_ref()
            .ok_or_else(|| not_actionable(target_id, "interaction requires visible geometry"))
    }
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
        let (min_x, max_x, min_y, max_y) = quad_bounds(resolved.geometry(bound.target_id)?);
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
        request: SnapshotPageRequest,
        started_at: krometrail_core::SessionTime,
        include_document_geometry: bool,
    ) -> Result<BrowserOperationResult> {
        let frame = match &request.document {
            krometrail_core::SemanticDocumentScope::MainDocument => None,
            krometrail_core::SemanticDocumentScope::Frame(reference) => {
                Some(Self::resolve_frame_document(transport, bound, reference).await?)
            }
        };
        let include_document_geometry =
            include_document_geometry || request.anchor == SnapshotPageAnchor::Viewport;
        let viewport_scope = if request.anchor == SnapshotPageAnchor::Viewport {
            let scope = CommandScope::Session(bound.transport_session.clone());
            let layout = transport
                .send_raw(&scope, "Page.getLayoutMetrics", json!({}))
                .await
                .map_err(|error| {
                    transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
                })?;
            let layout_root = layout
                .get("result")
                .filter(|value| value.get("cssVisualViewport").is_some())
                .unwrap_or(&layout);
            Some(super::rect_from_viewport(
                layout_root.get("cssVisualViewport"),
                "visual viewport",
                bound.target_id,
            )?)
        } else {
            None
        };
        let snapshot = self
            .capture_snapshot_for_frame(
                transport,
                bound,
                started_at,
                false,
                include_document_geometry,
                viewport_scope,
                frame.as_ref(),
            )
            .await?;
        let snapshot = if let Some(viewport) = viewport_scope {
            snapshot.with_visual_viewport(viewport)
        } else {
            snapshot
        };
        Ok(BrowserOperationResult::SnapshotPage(Box::new(snapshot)))
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
            .capture_snapshot_for_frame(
                transport,
                bound,
                started_at,
                request.query.requires_dom_semantics(),
                false,
                None,
                frame.as_ref(),
            )
            .await?;
        let result = self.snapshots.query(bound, &request, &snapshot)?;
        Ok(BrowserOperationResult::QueryPage(Box::new(result)))
    }

    pub(super) async fn probe_semantic_presence(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        query: &SemanticQuery,
        started_at: krometrail_core::SessionTime,
    ) -> Result<SemanticPresenceProbe> {
        let snapshot = self
            .capture_snapshot_for_frame(
                transport,
                bound,
                started_at,
                query.requires_dom_semantics(),
                false,
                None,
                None,
            )
            .await?;
        self.snapshots.probe_presence(bound, query, &snapshot)
    }

    #[allow(clippy::too_many_arguments)]
    async fn capture_snapshot_for_frame(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        started_at: krometrail_core::SessionTime,
        include_dom_semantics: bool,
        include_document_geometry: bool,
        viewport_scope: Option<CssRect>,
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
        let dom_response = if include_dom_semantics || include_document_geometry {
            Some(
                transport
                    .send_raw(
                        &scope,
                        "DOMSnapshot.captureSnapshot",
                        json!({
                            "computedStyles": [],
                            "includePaintOrder": false,
                            "includeDOMRects": include_document_geometry,
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
        let (semantic, document_rects, geometry_omitted) = match dom_response {
            Some(response) => {
                let dom_snapshot = decode_dom_snapshot_with_geometry(
                    &response,
                    &document,
                    bound.target_id,
                    include_document_geometry,
                    viewport_scope,
                )?;
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
                let semantic = dom_snapshot
                    .metadata
                    .into_iter()
                    .filter_map(|(backend, metadata)| {
                        node_by_backend
                            .get(&backend)
                            .copied()
                            .map(|node_id| (node_id, metadata))
                    })
                    .collect();
                (
                    semantic,
                    dom_snapshot.document_rects,
                    dom_snapshot.geometry_omitted,
                )
            }
            None => (HashMap::new(), HashMap::new(), false),
        };
        let mut nodes = nodes;
        if !document_rects.is_empty() {
            let node_indexes = nodes
                .iter()
                .enumerate()
                .map(|(index, node)| (node.id, index))
                .collect::<HashMap<_, _>>();
            for (backend, rect) in document_rects {
                if let Some(node_id) = node_by_backend.get(&backend)
                    && let Some(index) = node_indexes.get(node_id)
                {
                    nodes[*index].document_rect = Some(rect);
                }
            }
        }
        let parent_by_node = nodes.iter().map(|node| (node.id, node.parent)).collect();
        let completed_at = self.session_time()?;
        let context = ObservationContext::new(
            self.session_id,
            bound.target_id,
            bound.attachment_generation,
            started_at,
            completed_at,
        )?;
        let snapshot = PageSnapshot::new(context, generation, nodes, omitted_node_count)?
            .with_geometry_omitted(geometry_omitted);
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
                dom_semantics_captured: include_dom_semantics,
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

    fn active_for_query(
        &self,
        bound: &BoundTarget,
        snapshot: &PageSnapshot,
        requires_dom_semantics: bool,
    ) -> Result<&ActiveSnapshot> {
        let active = self
            .targets
            .get(&bound.target_id)
            .and_then(|target| target.active.as_ref())
            .filter(|active| {
                active.generation == snapshot.generation
                    && active.attachment_generation == bound.attachment_generation
            })
            .ok_or_else(|| stale(bound.target_id, "semantic snapshot is no longer active"))?;
        if requires_dom_semantics && !active.dom_semantics_captured {
            return Err(operation_error(
                ErrorCode::PageObservationFailed,
                bound.target_id,
                "this query requires DOM semantic acquisition, but the active snapshot contains accessibility data only",
            ));
        }
        if snapshot.omitted_node_count != 0 {
            let actual = u64::try_from(snapshot.nodes.len())
                .unwrap_or(u64::MAX)
                .saturating_add(u64::from(snapshot.omitted_node_count));
            return Err(snapshot_node_limit_error(bound.target_id, actual, true));
        }
        Ok(active)
    }

    fn query(
        &self,
        bound: &BoundTarget,
        request: &QueryPageRequest,
        snapshot: &PageSnapshot,
    ) -> Result<QueryPageResult> {
        let active =
            self.active_for_query(bound, snapshot, request.query.requires_dom_semantics())?;
        if let Some(scope) = request.scope {
            self.active_reference_backend(bound, scope)?;
        }

        let in_scope = |node: &SnapshotNode| {
            !request.scope.is_some_and(|scope| {
                !is_strict_descendant(node.id, scope.node_id, &active.parent_by_node)
            })
        };
        let evaluate = |query: &krometrail_core::SemanticQuery, node: &SnapshotNode| {
            semantic_query_matches(
                query,
                node,
                active
                    .semantic
                    .get(&node.id)
                    .unwrap_or(&SemanticNodeMetadata::default()),
                &active.parent_by_node,
                &active.semantic,
                &snapshot.nodes,
            )
        };

        let matches: Vec<SemanticMatch> = snapshot
            .nodes
            .iter()
            .filter_map(|node| {
                let reference = node.reference?;
                if !in_scope(node) {
                    return None;
                }
                evaluate(&request.query, node).then(|| SemanticMatch {
                    reference,
                    role: node.role.clone(),
                    name: node.name.clone(),
                })
            })
            .collect();

        // Sites decorate accessible names routinely ("Cargo.toml, (File)"), so an exact-mode
        // query that finds nothing is usually one relaxation away from the intended node. Report
        // how many nodes a `contains` retry would reach instead of leaving the caller to guess.
        // The scan runs only on the empty result, over the same already-bounded snapshot nodes,
        // and stops at the declared candidate cap.
        let relaxed_match_candidates = if matches.is_empty() {
            request.query.relaxed_to_contains().map(|relaxed| {
                let limit = usize::from(krometrail_core::MAX_SEMANTIC_RELAXED_CANDIDATES);
                let count = snapshot
                    .nodes
                    .iter()
                    .filter(|node| node.reference.is_some() && in_scope(node))
                    .filter(|node| evaluate(&relaxed, node))
                    .take(limit)
                    .count();
                krometrail_core::RelaxedMatchCandidates::new(count)
            })
        } else {
            None
        };

        let uncontained_match_candidates = if matches.is_empty() {
            request.query.without_container_text().map(|stripped| {
                let limit = usize::from(krometrail_core::MAX_SEMANTIC_RELAXED_CANDIDATES);
                let count = snapshot
                    .nodes
                    .iter()
                    .filter(|node| node.reference.is_some() && in_scope(node))
                    .filter(|node| evaluate(&stripped, node))
                    .take(limit)
                    .count();
                krometrail_core::RelaxedMatchCandidates::new(count)
            })
        } else {
            None
        };

        QueryPageResult::with_no_match_diagnostics(
            snapshot.context.clone(),
            snapshot.generation,
            matches,
            request.max_matches,
            relaxed_match_candidates,
            uncontained_match_candidates,
        )
    }

    pub(super) fn probe_presence(
        &self,
        bound: &BoundTarget,
        query: &SemanticQuery,
        snapshot: &PageSnapshot,
    ) -> Result<SemanticPresenceProbe> {
        let active = self.active_for_query(bound, snapshot, query.requires_dom_semantics())?;
        let evaluate = |query: &SemanticQuery, node: &SnapshotNode| {
            semantic_query_matches(
                query,
                node,
                active
                    .semantic
                    .get(&node.id)
                    .unwrap_or(&SemanticNodeMetadata::default()),
                &active.parent_by_node,
                &active.semantic,
                &snapshot.nodes,
            )
        };
        let match_count = snapshot
            .nodes
            .iter()
            .filter(|node| evaluate(query, node))
            .fold(0_u32, |count, _| count.saturating_add(1));
        let outcome = match match_count {
            0 => SemanticQueryOutcome::NoMatch,
            1 => SemanticQueryOutcome::Unique,
            _ => SemanticQueryOutcome::Ambiguous,
        };
        let relaxed_match_candidates = if match_count == 0 {
            query.relaxed_to_contains().map(|relaxed| {
                let limit = usize::from(krometrail_core::MAX_SEMANTIC_RELAXED_CANDIDATES);
                let count = snapshot
                    .nodes
                    .iter()
                    .filter(|node| evaluate(&relaxed, node))
                    .take(limit)
                    .count();
                RelaxedMatchCandidates::new(count)
            })
        } else {
            None
        };

        Ok(SemanticPresenceProbe {
            outcome,
            match_count,
            relaxed_match_candidates,
        })
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
        let (backend, expectation_role) = self
            .validated_reference_backend(transport, bound, reference)
            .await?;
        resolve_backend_node(
            transport,
            &scope,
            bound.target_id,
            backend,
            Some(expectation_role),
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
        let backend = query_selector_backend(transport, &scope, bound.target_id, selector)
            .await?
            .ok_or_else(|| {
                operation_error(
                    ErrorCode::NotFound,
                    bound.target_id,
                    "CSS selector did not match an element",
                )
            })?;
        resolve_backend_node(
            transport,
            &scope,
            bound.target_id,
            backend,
            None,
            requirement,
        )
        .await
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
                    .await?
                    .0,
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
    ) -> Result<(i64, ExpectationTargetRole)> {
        let (document, backend, expectation_role) =
            self.active_reference_backend(bound, reference)?;
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
        Ok((backend, expectation_role))
    }

    fn active_reference_backend(
        &self,
        bound: &BoundTarget,
        reference: NodeReference,
    ) -> Result<(&DocumentFingerprint, i64, ExpectationTargetRole)> {
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
        let binding = active.bindings.get(&reference.node_id).ok_or_else(|| {
            stale(
                bound.target_id,
                "snapshot node has no backing document node",
            )
        })?;
        Ok((
            &active.document,
            binding.backend_node_id,
            binding.expectation_role,
        ))
    }
}

fn semantic_query_matches(
    query: &SemanticQuery,
    node: &SnapshotNode,
    metadata: &SemanticNodeMetadata,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic: &HashMap<SnapshotNodeId, SemanticNodeMetadata>,
    nodes: &[SnapshotNode],
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
                    nearest_container_text_matches(node.id, expected, parents, semantic, nodes)
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
    nodes: &[SnapshotNode],
) -> bool {
    let mut current = parents.get(&node).copied().flatten();
    while let Some(ancestor) = current {
        let Some(ancestor_node) = nodes.iter().find(|candidate| candidate.id == ancestor) else {
            return false;
        };
        if is_local_container_role(&ancestor_node.role) {
            // A semantic container declares an identity boundary: the nearest one is the sole
            // authority for the query, exactly as before.
            return semantic
                .get(&ancestor)
                .is_some_and(|metadata| expected.matches(&metadata.rendered_text));
        }
        if is_generic_container_role(&ancestor_node.role)
            && let Some(metadata) = semantic.get(&ancestor)
            && metadata.true_collapsed_text_bytes()
                <= krometrail_core::MAX_GENERIC_CONTAINER_TEXT_BYTES
            && expected.matches(&metadata.rendered_text)
        {
            // Styling divs do not declare identity boundaries, so a bounded generic ancestor
            // qualifies opportunistically and a non-matching one stays transparent. Rendered
            // text only grows upward, so page-scale wrappers can never qualify a control.
            return true;
        }
        current = parents.get(&ancestor).copied().flatten();
    }
    false
}

fn is_local_container_role(role: &str) -> bool {
    LOCAL_CONTAINER_ROLES
        .iter()
        .any(|candidate| role.eq_ignore_ascii_case(candidate))
}

fn is_generic_container_role(role: &str) -> bool {
    GENERIC_CONTAINER_ROLES
        .iter()
        .any(|candidate| role.eq_ignore_ascii_case(candidate))
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
    id: Option<String>,
    is_label: bool,
    label_for: Option<String>,
    aria_labelledby: Option<String>,
    test_id: Option<String>,
}

struct DecodedDomSnapshot {
    metadata: HashMap<i64, SemanticNodeMetadata>,
    document_rects: HashMap<i64, CssRect>,
    geometry_omitted: bool,
}

#[cfg(test)]
fn decode_dom_snapshot(
    response: &Value,
    document: &DocumentFingerprint,
    target_id: TargetId,
) -> Result<HashMap<i64, SemanticNodeMetadata>> {
    Ok(decode_dom_snapshot_with_geometry(response, document, target_id, false, None)?.metadata)
}

fn snapshot_node_limit_error(
    target_id: TargetId,
    actual: impl std::fmt::Display,
    query_exists: bool,
) -> KrometrailError {
    let recovery = if query_exists {
        "narrow the semantic query to a smaller document"
    } else {
        "request a smaller document snapshot or use viewport-scoped geometry"
    };
    KrometrailError::limit_exceeded(
        ErrorCode::PageObservationFailed,
        "accessibility nodes",
        actual,
        MAX_SNAPSHOT_NODES,
        None::<usize>,
    )
    .with_context(ErrorContext {
        target_id: Some(target_id),
        ..ErrorContext::default()
    })
    .with_retry(RetryAdvice::Never)
    .with_recovery(NonEmptyText::new(recovery).expect("snapshot limit recovery is non-empty"))
}

fn decode_dom_snapshot_with_geometry(
    response: &Value,
    document: &DocumentFingerprint,
    target_id: TargetId,
    include_document_geometry: bool,
    viewport_scope: Option<CssRect>,
) -> Result<DecodedDomSnapshot> {
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
    let node_count = backend_ids.len();
    let parents = required_parallel_array(nodes, "parentIndex", node_count, target_id)?;
    let node_names = required_parallel_array(nodes, "nodeName", node_count, target_id)?;
    let attributes = required_parallel_array(nodes, "attributes", node_count, target_id)?;
    let layout = document
        .get("layout")
        .ok_or_else(|| malformed(target_id, "DOM snapshot layout table is missing"))?;
    let layout_nodes = required_array(layout, "nodeIndex", target_id)?;
    let layout_text = required_parallel_array(layout, "text", layout_nodes.len(), target_id)?;
    if backend_ids.len() > MAX_SNAPSHOT_NODES {
        if let Some(viewport) = viewport_scope {
            return decode_viewport_scoped_dom_snapshot(
                strings,
                backend_ids,
                parents,
                node_names,
                attributes,
                layout_nodes,
                layout_text,
                layout,
                viewport,
                target_id,
            );
        }
        if include_document_geometry {
            return Ok(DecodedDomSnapshot {
                metadata: HashMap::new(),
                document_rects: HashMap::new(),
                geometry_omitted: true,
            });
        }
        return Err(snapshot_node_limit_error(
            target_id,
            backend_ids.len(),
            false,
        ));
    }
    let mut text_bytes = 0_usize;
    let mut decoded = Vec::with_capacity(node_count);
    let mut id_to_index = HashMap::new();
    for index in 0..node_count {
        let node = decode_dom_node(
            index,
            backend_ids,
            parents,
            node_names,
            attributes,
            strings,
            target_id,
            &mut text_bytes,
        )?;
        let id = node.id.clone();
        if let Some(id) = &id {
            id_to_index.entry(id.clone()).or_insert(index);
        }
        decoded.push(node);
    }
    let layout_bounds = include_document_geometry
        .then(|| required_parallel_array(layout, "bounds", layout_nodes.len(), target_id))
        .transpose()?;
    let mut document_rects = HashMap::new();
    if let Some(bounds) = layout_bounds {
        for (node_index, bounds) in layout_nodes.iter().zip(bounds) {
            let Some(node_index) = node_index
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value < node_count)
            else {
                return Err(malformed(
                    target_id,
                    "DOM snapshot layout node index is invalid",
                ));
            };
            let Some(bounds) = bounds.as_array().filter(|values| values.len() == 4) else {
                return Err(malformed(
                    target_id,
                    "DOM snapshot layout bounds are malformed",
                ));
            };
            let values = bounds.iter().map(Value::as_f64).collect::<Option<Vec<_>>>();
            let Some(values) = values else {
                return Err(malformed(
                    target_id,
                    "DOM snapshot layout bounds are malformed",
                ));
            };
            if values.iter().all(|value| value.is_finite())
                && let Ok(origin) = CssPoint::new(values[0], values[1])
                && let Ok(size) = CssSize::new(values[2], values[3])
                && let Ok(rect) = CssRect::new(origin, size)
            {
                document_rects.insert(decoded[node_index].backend_node_id, rect);
            }
        }
    }
    let mut rendered = vec![String::new(); node_count];
    let mut collapsed_text_bytes = vec![0_usize; node_count];
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
            append_semantic_text(&mut rendered[index], &mut collapsed_text_bytes[index], text);
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
                    collapsed_text_bytes: collapsed_text_bytes[index],
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
            let mut composed_bytes = 0;
            for id in labelledby.split_ascii_whitespace() {
                if let Some(label_index) = id_to_index.get(id) {
                    append_semantic_text(
                        &mut composed,
                        &mut composed_bytes,
                        &rendered[*label_index],
                    );
                }
            }
            push_label(&mut metadata, node.backend_node_id, &composed);
        }
    }
    Ok(DecodedDomSnapshot {
        metadata,
        document_rects,
        geometry_omitted: false,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_dom_node(
    index: usize,
    backend_ids: &[Value],
    parents: &[Value],
    node_names: &[Value],
    attributes: &[Value],
    strings: &[Value],
    target_id: TargetId,
    text_bytes: &mut usize,
) -> Result<DecodedDomNode> {
    let backend_node_id = backend_ids[index]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(|| malformed(target_id, "DOM snapshot backend node id is invalid"))?;
    let parent = match parents[index].as_i64() {
        Some(-1) => None,
        Some(value) if value >= 0 && usize::try_from(value).is_ok_and(|value| value < index) => {
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
        *text_bytes = text_bytes.saturating_add(value.len());
        if *text_bytes > MAX_SNAPSHOT_TEXT_BYTES {
            return Err(malformed(
                target_id,
                "DOM snapshot exceeds the semantic text limit",
            ));
        }
        *destination = bounded_semantic_value(value);
    }
    Ok(DecodedDomNode {
        backend_node_id,
        parent,
        id,
        is_label: node_name.eq_ignore_ascii_case("label"),
        label_for,
        aria_labelledby,
        test_id,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_viewport_scoped_dom_snapshot(
    strings: &[Value],
    backend_ids: &[Value],
    parents: &[Value],
    node_names: &[Value],
    attributes: &[Value],
    layout_nodes: &[Value],
    layout_text: &[Value],
    layout: &Value,
    viewport: CssRect,
    target_id: TargetId,
) -> Result<DecodedDomSnapshot> {
    let bounds = required_parallel_array(layout, "bounds", layout_nodes.len(), target_id)?;
    let mut selected_indexes = Vec::new();
    let mut geometry_omitted = false;
    let mut document_rects = HashMap::new();
    for (node_index, bounds) in layout_nodes.iter().zip(bounds) {
        let Some(node_index) = node_index
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < backend_ids.len())
        else {
            return Err(malformed(
                target_id,
                "DOM snapshot layout node index is invalid",
            ));
        };
        let Some(bounds) = bounds.as_array().filter(|values| values.len() == 4) else {
            return Err(malformed(
                target_id,
                "DOM snapshot layout bounds are malformed",
            ));
        };
        let values = bounds.iter().map(Value::as_f64).collect::<Option<Vec<_>>>();
        let Some(values) = values.filter(|values| {
            values.iter().all(|value| value.is_finite()) && values[2] > 0.0 && values[3] > 0.0
        }) else {
            continue;
        };
        let intersects = values[0] < viewport.origin.x + viewport.size.width
            && values[0] + values[2] > viewport.origin.x
            && values[1] < viewport.origin.y + viewport.size.height
            && values[1] + values[3] > viewport.origin.y;
        if !intersects {
            continue;
        }
        if selected_indexes.len() >= MAX_SNAPSHOT_NODES {
            geometry_omitted = true;
            continue;
        }
        selected_indexes.push(node_index);
        let rect = CssRect::new(
            CssPoint::new(values[0], values[1])?,
            CssSize::new(values[2], values[3])?,
        )?;
        let backend = backend_ids[node_index]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(|| malformed(target_id, "DOM snapshot backend node id is invalid"))?;
        document_rects.insert(backend, rect);
    }

    let selected = selected_indexes.iter().copied().collect::<HashSet<_>>();
    let mut text_bytes = 0_usize;
    let mut decoded = HashMap::new();
    let mut id_to_index = HashMap::new();
    for index in &selected_indexes {
        let node = decode_dom_node(
            *index,
            backend_ids,
            parents,
            node_names,
            attributes,
            strings,
            target_id,
            &mut text_bytes,
        )?;
        if let Some(id) = &node.id {
            id_to_index.entry(id.clone()).or_insert(*index);
        }
        decoded.insert(*index, node);
    }

    let mut rendered = HashMap::<usize, String>::new();
    let mut collapsed_text_bytes = HashMap::<usize, usize>::new();
    for (node_index, text) in layout_nodes.iter().zip(layout_text) {
        let node_index = node_index
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < backend_ids.len())
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
        if !selected.contains(&node_index) {
            continue;
        }
        text_bytes = text_bytes.saturating_add(text.len());
        if text_bytes > MAX_SNAPSHOT_TEXT_BYTES {
            return Err(malformed(
                target_id,
                "DOM snapshot exceeds the semantic text limit",
            ));
        }
        let mut ancestor = Some(node_index);
        while let Some(index) = ancestor {
            if selected.contains(&index) {
                append_semantic_text(
                    rendered.entry(index).or_default(),
                    collapsed_text_bytes.entry(index).or_default(),
                    text,
                );
            }
            ancestor = decoded.get(&index).and_then(|node| node.parent);
        }
    }

    let mut metadata = decoded
        .iter()
        .map(|(index, node)| {
            (
                node.backend_node_id,
                SemanticNodeMetadata {
                    labels: Vec::new(),
                    rendered_text: rendered.get(index).cloned().unwrap_or_default(),
                    collapsed_text_bytes: collapsed_text_bytes.get(index).copied().unwrap_or(0),
                    test_id: node.test_id.clone(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for (label_index, label) in &decoded {
        let text = rendered
            .get(label_index)
            .map(String::as_str)
            .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        if let Some(target_index) = label
            .label_for
            .as_ref()
            .and_then(|value| id_to_index.get(value))
            .and_then(|index| decoded.get(index))
        {
            push_label(&mut metadata, target_index.backend_node_id, text);
        }
    }
    for node in decoded.values() {
        let mut parent = node.parent;
        while let Some(parent_index) = parent {
            if decoded
                .get(&parent_index)
                .is_some_and(|parent| parent.is_label)
            {
                let text = rendered
                    .get(&parent_index)
                    .map(String::as_str)
                    .unwrap_or_default();
                push_label(&mut metadata, node.backend_node_id, text);
                break;
            }
            parent = decoded.get(&parent_index).and_then(|parent| parent.parent);
        }
        if let Some(labelledby) = &node.aria_labelledby {
            let mut composed = String::new();
            let mut composed_bytes = 0;
            for id in labelledby.split_ascii_whitespace() {
                if let Some(label_index) = id_to_index.get(id) {
                    if let Some(text) = rendered.get(label_index) {
                        append_semantic_text(&mut composed, &mut composed_bytes, text);
                    }
                }
            }
            push_label(&mut metadata, node.backend_node_id, &composed);
        }
    }
    Ok(DecodedDomSnapshot {
        metadata,
        document_rects,
        geometry_omitted,
    })
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

fn append_semantic_text(destination: &mut String, collapsed_bytes: &mut usize, value: &str) {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return;
    }
    let separator = usize::from(!destination.is_empty());
    *collapsed_bytes = collapsed_bytes
        .saturating_add(separator)
        .saturating_add(normalized.len());
    if destination.len() >= MAX_SEMANTIC_QUERY_TEXT_BYTES {
        return;
    }
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

// `inert`, native disabled state, and `aria-disabled` suppress interaction, not painting.
// Keep them separate from actual visibility so screenshot-only resolution can still crop
// a visible control. The parent walk captures inherited light-DOM inertness without a
// selector query that Chrome's side-effect analysis may conservatively refuse.
//
// The same probe additionally reads bounded postcondition state facts
// (checked/expanded/selected/pressed and the value length — never the value).
// Each fact read is individually guarded so one property Chrome's side-effect
// analysis refuses degrades that fact to null instead of failing the probe.
const NODE_STATE_PROBE: &str = "function(){const g=f=>{try{return f()}catch(_){return null}};const ab=v=>v==='true'?true:v==='false'?false:null;const s=getComputedStyle(this);let n=this,inert=false;while(n&&!inert){inert=n.inert===true;n=n.parentElement;}const tag=this.tagName;const type=tag==='INPUT'?(this.type||'text').toLowerCase():null;return {connected:this.isConnected,visuallyHidden:this.hidden||s.display==='none'||s.visibility==='hidden'||s.visibility==='collapse'||s.contentVisibility==='hidden',interactionBlocked:inert||this.disabled||this.getAttribute('aria-disabled')==='true',tagName:tag,inputType:type,isEditable:!this.readOnly&&!this.disabled&&(this.isContentEditable||(tag==='INPUT'&&/^(text|search|url|email|tel|password|number|date|time|datetime-local|month|week)$/.test(type))||tag==='TEXTAREA'),isSelect:tag==='SELECT',isFileInput:tag==='INPUT'&&type==='file',checked:g(()=>tag==='INPUT'&&(type==='checkbox'||type==='radio')?this.checked===true:ab(this.getAttribute('aria-checked'))),ariaExpanded:g(()=>ab(this.getAttribute('aria-expanded'))),selected:g(()=>tag==='OPTION'?this.selected===true:ab(this.getAttribute('aria-selected'))),pressed:g(()=>ab(this.getAttribute('aria-pressed'))),valueLength:g(()=>typeof this.value==='string'?this.value.length:null)};}";

const ASSOCIATED_FILE_INPUT_FUNCTION: &str = "function(){const isFile=e=>e instanceof HTMLInputElement&&e.type==='file';if(isFile(this))return this;const label=this.closest&&this.closest('label');if(label&&isFile(label.control))return label.control;const contained=this.querySelector&&this.querySelector('input[type=file]');if(isFile(contained))return contained;const ids=(this.getAttribute('aria-controls')||'').split(/\\s+/).concat((this.getAttribute('aria-owns')||'').split(/\\s+/));for(const id of ids){if(!id)continue;const candidate=document.getElementById(id);if(isFile(candidate))return candidate;}const ownId=this.id;if(ownId){const labelled=Array.from(document.querySelectorAll('input[type=file][aria-labelledby]')).find(input=>(input.getAttribute('aria-labelledby')||'').split(/\\s+/).includes(ownId));if(labelled)return labelled;}const parent=this.parentElement;if(parent){const candidates=Array.from(parent.querySelectorAll('input[type=file]'));if(candidates.length===1)return candidates[0];}return null;}";

const EDITABLE_HOST_FUNCTION: &str = "function(){const root=this.getRootNode&&this.getRootNode();const host=root&&root.host;return host instanceof HTMLInputElement?host:this;}";

/// Parses the bounded state facts out of a probe response. Every missing or
/// non-boolean field degrades that one fact to unobserved.
fn parse_node_state_facts(state: &Value) -> NodeStateFacts {
    let flag = |field: &str| state.get(field).and_then(Value::as_bool);
    NodeStateFacts {
        connected: flag("connected").unwrap_or(false),
        checked: flag("checked"),
        expanded: flag("ariaExpanded"),
        selected: flag("selected"),
        pressed: flag("pressed"),
        value_length: state
            .get("valueLength")
            .and_then(Value::as_u64)
            .map(|value| u32::try_from(value).unwrap_or(u32::MAX)),
    }
}

/// Bounded post-action state probe of an already-resolved backing node.
/// Every failure — transport, timeout, or a payload missing the boolean
/// `connected` fact — degrades to `None`; postcondition assembly maps that to
/// an unobserved target outcome, never to a detachment claim, and never fails
/// a proven dispatch. Only a probe that ran and reported `connected: false`
/// yields a detachment fact.
pub(super) async fn probe_backend_node_facts(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
) -> Option<NodeStateFacts> {
    let object_id = resolve_backend_object(transport, scope, target_id, backend_node_id)
        .await
        .ok()?;
    let check = transport
        .send_raw(
            scope,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": NODE_STATE_PROBE,
                "returnByValue": true,
                "throwOnSideEffect": true,
                "silent": true,
            }),
        )
        .await
        .ok()?;
    let state = check
        .pointer("/result/value")
        .or_else(|| check.pointer("/result/result/value"))?;
    // A payload without the boolean connected fact is an unobserved probe;
    // defaulting it to false would fabricate a detachment observation.
    state.get("connected").and_then(Value::as_bool)?;
    Some(parse_node_state_facts(state))
}

async fn resolve_backend_node(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    expectation_role: Option<ExpectationTargetRole>,
    requirement: ReferenceRequirement,
) -> Result<ResolvedNode> {
    let object_id = resolve_backend_object(transport, scope, target_id, backend_node_id).await?;
    let check = transport
        .send_raw(
            scope,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": NODE_STATE_PROBE,
                "returnByValue": true,
                "throwOnSideEffect": true,
                "silent": true,
            }),
        )
        .await
        .map_err(|error| transport_error(error, ErrorCode::ReferenceNotActionable, target_id))?;
    let state = check
        .pointer("/result/value")
        .or_else(|| check.pointer("/result/result/value"))
        .ok_or_else(|| not_actionable(target_id, "node actionability response is malformed"))?;
    if requirement == ReferenceRequirement::FileInput
        && state.get("isFileInput").and_then(Value::as_bool) != Some(true)
    {
        if state.get("connected").and_then(Value::as_bool) != Some(true) {
            return Err(stale(target_id, "backing node is detached"));
        }
        if state.get("interactionBlocked").and_then(Value::as_bool) != Some(false) {
            return Err(not_actionable(
                target_id,
                "backing node is inert, disabled, or aria-disabled",
            ));
        }
        let associated_backend =
            resolve_associated_file_input(transport, scope, target_id, &object_id).await?;
        let Some(associated_backend) = associated_backend else {
            return Err(upload_target_not_file_input(target_id));
        };
        // The association probe is intentionally not recursive: the canonical node is
        // revalidated once, so a stale or non-file association cannot widen the search.
        return resolve_backend_node_once(
            transport,
            scope,
            target_id,
            associated_backend,
            expectation_role,
            requirement,
        )
        .await;
    }
    if requirement == ReferenceRequirement::Editable
        && state.get("isEditable").and_then(Value::as_bool) != Some(true)
    {
        if state.get("connected").and_then(Value::as_bool) != Some(true) {
            return Err(stale(target_id, "backing node is detached"));
        }
        if state.get("visuallyHidden").and_then(Value::as_bool) != Some(false) {
            return Err(not_actionable(target_id, "backing node is hidden"));
        }
        if state.get("interactionBlocked").and_then(Value::as_bool) != Some(false) {
            return Err(not_actionable(
                target_id,
                "backing node is inert, disabled, or aria-disabled",
            ));
        }
        let host_backend =
            resolve_editable_host(transport, scope, target_id, backend_node_id, &object_id).await?;
        if let Some(host_backend) = host_backend.filter(|backend| *backend != backend_node_id) {
            // The host is the owning native input. Revalidation is bounded to this one
            // promotion and never attempts another shadow traversal.
            return resolve_backend_node_once(
                transport,
                scope,
                target_id,
                host_backend,
                expectation_role,
                requirement,
            )
            .await;
        }
        let message = if state
            .get("inputType")
            .and_then(Value::as_str)
            .and_then(TemporalInputKind::from_input_type)
            .is_some()
        {
            "backing node is not valid for the requested interaction; for a native date/time field, target the input element itself"
        } else {
            "backing node is not valid for the requested interaction"
        };
        return Err(not_actionable(target_id, message));
    }
    resolve_backend_node_from_state(
        transport,
        scope,
        target_id,
        backend_node_id,
        expectation_role,
        requirement,
        state,
    )
    .await
}

async fn resolve_backend_node_once(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    expectation_role: Option<ExpectationTargetRole>,
    requirement: ReferenceRequirement,
) -> Result<ResolvedNode> {
    let object_id = resolve_backend_object(transport, scope, target_id, backend_node_id).await?;
    let check = transport
        .send_raw(
            scope,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": NODE_STATE_PROBE,
                "returnByValue": true,
                "throwOnSideEffect": true,
                "silent": true,
            }),
        )
        .await
        .map_err(|error| transport_error(error, ErrorCode::ReferenceNotActionable, target_id))?;
    let state = check
        .pointer("/result/value")
        .or_else(|| check.pointer("/result/result/value"))
        .ok_or_else(|| not_actionable(target_id, "node actionability response is malformed"))?;
    resolve_backend_node_from_state(
        transport,
        scope,
        target_id,
        backend_node_id,
        expectation_role,
        requirement,
        state,
    )
    .await
}

async fn resolve_backend_node_from_state(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    expectation_role: Option<ExpectationTargetRole>,
    requirement: ReferenceRequirement,
    state: &Value,
) -> Result<ResolvedNode> {
    validate_node_state(state, requirement, target_id)?;
    let facts = parse_node_state_facts(state);
    let temporal_input = state
        .get("inputType")
        .and_then(Value::as_str)
        .and_then(TemporalInputKind::from_input_type);
    if !requirement.requires_visible_geometry() {
        return Ok(ResolvedNode {
            backend_node_id,
            document_quad: None,
            facts,
            expectation_role,
            temporal_input,
        });
    }
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
        document_quad: Some(document_quad),
        facts,
        expectation_role,
        temporal_input,
    })
}

async fn resolve_associated_file_input(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    object_id: &str,
) -> Result<Option<i64>> {
    let response = transport
        .send_raw(
            scope,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": ASSOCIATED_FILE_INPUT_FUNCTION,
                "returnByValue": false,
                "throwOnSideEffect": false,
                "silent": true,
            }),
        )
        .await
        .map_err(|error| transport_error(error, ErrorCode::ReferenceNotActionable, target_id))?;
    let Some(associated_object_id) = response
        .pointer("/result/result/objectId")
        .or_else(|| response.pointer("/result/objectId"))
        .or_else(|| response.pointer("/result/object/objectId"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let described = transport
        .send_raw(
            scope,
            "DOM.describeNode",
            json!({"objectId": associated_object_id}),
        )
        .await
        .map_err(|_| stale(target_id, "associated file input is no longer available"))?;
    Ok(described
        .pointer("/node/backendNodeId")
        .or_else(|| described.pointer("/result/node/backendNodeId"))
        .and_then(Value::as_i64))
}

async fn resolve_editable_host(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    object_id: &str,
) -> Result<Option<i64>> {
    let response = transport
        .send_raw(
            scope,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": EDITABLE_HOST_FUNCTION,
                "returnByValue": false,
                "throwOnSideEffect": false,
                "silent": true,
            }),
        )
        .await
        .map_err(|error| transport_error(error, ErrorCode::ReferenceNotActionable, target_id))?;
    let Some(host_object_id) = response
        .pointer("/result/result/objectId")
        .or_else(|| response.pointer("/result/objectId"))
        .or_else(|| response.pointer("/result/object/objectId"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let described = transport
        .send_raw(
            scope,
            "DOM.describeNode",
            json!({"objectId": host_object_id}),
        )
        .await
        .map_err(|_| stale(target_id, "native date/time owner is no longer available"))?;
    let host_backend = described
        .pointer("/node/backendNodeId")
        .or_else(|| described.pointer("/result/node/backendNodeId"))
        .and_then(Value::as_i64);
    Ok(host_backend.or(Some(backend_node_id)))
}

fn upload_target_not_file_input(target_id: TargetId) -> krometrail_core::KrometrailError {
    operation_error(
        ErrorCode::ReferenceNotActionable,
        target_id,
        "upload_target_not_file_input: element is not a file input and no associated file input was found (label association, contained input, aria-controls/aria-owns, aria-labelledby, unique sibling input)",
    )
    .with_recovery(
        NonEmptyText::new(
            "target the page's native input[type=file] directly (CSS selector escape hatch) or an element associated with it",
        )
        .expect("static upload recovery is non-empty"),
    )
}

fn validate_node_state(
    state: &Value,
    requirement: ReferenceRequirement,
    target_id: TargetId,
) -> Result<()> {
    if state.get("connected").and_then(Value::as_bool) != Some(true) {
        return Err(stale(target_id, "backing node is detached"));
    }
    if requirement.requires_visible_geometry()
        && state.get("visuallyHidden").and_then(Value::as_bool) != Some(false)
    {
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
                let structural_web_area = STRUCTURAL_WEB_AREA_ROLES
                    .iter()
                    .any(|candidate| role.eq_ignore_ascii_case(candidate));
                let actionable = backend.is_some()
                    && !disabled
                    && !hidden
                    && (ACTIONABLE_ROLES.contains(&role) || (signal && !structural_web_area));
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
                    document_rect: None,
                });
                if let Some(backend_node_id) = backend.filter(|_| actionable) {
                    self.bindings.insert(
                        node_id,
                        NodeBinding {
                            backend_node_id,
                            expectation_role: ExpectationTargetRole::from_accessibility_role(role),
                        },
                    );
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

    fn status_registry_fixture(
        status_names: &[&str],
        dom_semantics_captured: bool,
        omitted_node_count: u32,
    ) -> (SnapshotRegistry, BoundTarget, PageSnapshot) {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let nodes = std::iter::once(SnapshotNode {
            id: root,
            parent: None,
            depth: 0,
            role: "document".into(),
            name: None,
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        })
        .chain(status_names.iter().enumerate().map(|(index, name)| {
            let id = SnapshotNodeId::new(u32::try_from(index + 2).unwrap()).unwrap();
            SnapshotNode {
                id,
                parent: Some(root),
                depth: 1,
                role: "status".into(),
                name: Some((*name).to_owned()),
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
                document_rect: None,
            }
        }))
        .collect::<Vec<_>>();
        let context = ObservationContext::new(
            krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(12)),
            target(),
            1,
            krometrail_core::SessionTime::ZERO,
            krometrail_core::SessionTime::ZERO,
        )
        .unwrap();
        let snapshot = PageSnapshot::new(context, generation, nodes, omitted_node_count).unwrap();
        let bound = frame_bound();
        let mut registry = SnapshotRegistry::default();
        registry.install(
            target(),
            ActiveSnapshot {
                generation,
                attachment_generation: bound.attachment_generation,
                document: DocumentFingerprint {
                    frame_id: "main".into(),
                    loader_id: "loader".into(),
                },
                frame: None,
                bindings: HashMap::new(),
                node_by_backend: HashMap::new(),
                semantic: HashMap::new(),
                parent_by_node: snapshot
                    .nodes
                    .iter()
                    .map(|node| (node.id, node.parent))
                    .collect(),
                dom_semantics_captured,
                next_node_id: u32::try_from(snapshot.nodes.len()).unwrap(),
            },
        );
        (registry, bound, snapshot)
    }

    fn status_query(name: Option<krometrail_core::SemanticTextMatch>) -> SemanticQuery {
        SemanticQuery::role("status", name).unwrap()
    }

    #[test]
    fn semantic_presence_probe_counts_full_tree_while_query_stays_actionable_only() {
        for (status_names, expected_outcome, expected_count) in [
            (vec![], SemanticQueryOutcome::NoMatch, 0),
            (vec!["Ready"], SemanticQueryOutcome::Unique, 1),
            (
                vec!["Ready", "Still ready"],
                SemanticQueryOutcome::Ambiguous,
                2,
            ),
        ] {
            let (registry, bound, snapshot) = status_registry_fixture(&status_names, false, 0);
            let query = status_query(None);
            let probe = registry.probe_presence(&bound, &query, &snapshot).unwrap();
            assert_eq!(probe.outcome, expected_outcome);
            assert_eq!(probe.match_count, expected_count);
        }

        let (registry, bound, snapshot) = status_registry_fixture(&["Ready"], false, 0);
        let query = status_query(None);
        let request = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            query.clone(),
            None,
            20,
        )
        .unwrap();
        let result = registry.query(&bound, &request, &snapshot).unwrap();
        assert_eq!(result.outcome, SemanticQueryOutcome::NoMatch);
        assert!(result.matches.is_empty());
        assert_eq!(
            registry
                .probe_presence(&bound, &query, &snapshot)
                .unwrap()
                .match_count,
            1
        );
    }

    #[test]
    fn semantic_presence_probe_reports_relaxed_candidates_over_nonactionable_nodes() {
        let status_names =
            vec!["Toast ready"; usize::from(krometrail_core::MAX_SEMANTIC_RELAXED_CANDIDATES,) + 1];
        let (registry, bound, snapshot) = status_registry_fixture(&status_names, false, 0);
        let query = status_query(Some(
            krometrail_core::SemanticTextMatch::new(
                "Toast",
                krometrail_core::SemanticTextMatchMode::Exact,
                false,
            )
            .unwrap(),
        ));
        let probe = registry.probe_presence(&bound, &query, &snapshot).unwrap();
        assert_eq!(probe.outcome, SemanticQueryOutcome::NoMatch);
        assert_eq!(probe.match_count, 0);
        assert_eq!(
            probe.relaxed_match_candidates,
            Some(RelaxedMatchCandidates {
                count: krometrail_core::MAX_SEMANTIC_RELAXED_CANDIDATES,
                saturated: true,
            })
        );
    }

    #[test]
    fn semantic_presence_probe_reuses_query_guards() {
        let (registry, bound, snapshot) = status_registry_fixture(&["Ready"], false, 0);
        let query = SemanticQuery::Text {
            text: krometrail_core::SemanticTextMatch::new(
                "Ready",
                krometrail_core::SemanticTextMatchMode::Exact,
                false,
            )
            .unwrap(),
        };
        let request = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            query.clone(),
            None,
            20,
        )
        .unwrap();
        let query_error = registry.query(&bound, &request, &snapshot).unwrap_err();
        let probe_error = registry
            .probe_presence(&bound, &query, &snapshot)
            .unwrap_err();
        assert_eq!(query_error.code, probe_error.code);
        assert_eq!(query_error.message, probe_error.message);

        let (registry, bound, snapshot) = status_registry_fixture(&["Ready"], false, 1);
        let query = status_query(None);
        let request = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            query.clone(),
            None,
            20,
        )
        .unwrap();
        let query_error = registry.query(&bound, &request, &snapshot).unwrap_err();
        let probe_error = registry
            .probe_presence(&bound, &query, &snapshot)
            .unwrap_err();
        assert_eq!(query_error.code, probe_error.code);
        assert_eq!(query_error.message, probe_error.message);
    }

    fn frame_tree(loader_id: &str) -> Value {
        frame_tree_with_urls(
            loader_id,
            "https://example.test/",
            "https://example.test/child",
        )
    }

    fn frame_tree_with_urls(loader_id: &str, root_url: &str, child_url: &str) -> Value {
        json!({"frameTree": {
            "frame": {"id":"main","loaderId":"main-loader","url":root_url},
            "childFrames": [{
                "frame": {"id":"child","loaderId":loader_id,"url":child_url}
            }]
        }})
    }

    fn opaque_frame_tree(loader_id: &str) -> Value {
        json!({"frameTree": {
            "frame": {"id":"main","loaderId":"main-loader","url":"data:text/html,root"},
            "childFrames": [{
                "frame": {"id":"child","loaderId":loader_id,"url":"about:srcdoc"}
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

    fn multi_document_snapshot_with_large_parent() -> Value {
        let strings = vec!["main", "child", "DIV", "H1", "#text", "Nested heading"];
        let parent_count = MAX_SNAPSHOT_NODES + 1;
        let parent_indices = (0..parent_count)
            .map(|index| if index == 0 { -1_i64 } else { 0_i64 })
            .collect::<Vec<_>>();
        let parent = json!({
            "frameId": 0,
            "nodes": {
                "parentIndex": parent_indices,
                "nodeName": vec![2; parent_count],
                "backendNodeId": (1..=parent_count).collect::<Vec<_>>(),
                "attributes": vec![Vec::<usize>::new(); parent_count]
            },
            "layout": {"nodeIndex": [], "text": []}
        });
        let child = json!({
            "frameId": 1,
            "nodes": {
                "parentIndex": [-1, 0, 1],
                "nodeName": [2, 3, 4],
                "backendNodeId": [101, 102, 107],
                "attributes": [[], [], []]
            },
            "layout": {"nodeIndex": [2], "text": [5]}
        });
        json!({"strings": strings, "documents": [parent, child]})
    }

    fn large_viewport_dom_snapshot(layout_count: usize) -> Value {
        let node_count = (MAX_SNAPSHOT_NODES + 1).max(layout_count);
        let mut attributes = vec![json!([]); node_count];
        attributes[10] = json!([2, 3]);
        let bounds = (0..layout_count)
            .map(|index| json!([index as f64, 10.0, 1.0, 10.0]))
            .collect::<Vec<_>>();
        json!({
            "strings": ["main", "DIV", "data-testid", "on-screen", "visible text"],
            "documents": [{
                "frameId": 0,
                "nodes": {
                    "parentIndex": (0..node_count).map(|index| if index == 0 { -1 } else { 0 }).collect::<Vec<_>>(),
                    "nodeName": vec![1; node_count],
                    "backendNodeId": (1..=node_count).collect::<Vec<_>>(),
                    "attributes": attributes
                },
                "layout": {
                    "nodeIndex": (0..layout_count).collect::<Vec<_>>(),
                    "text": vec![-1; layout_count],
                    "bounds": bounds
                }
            }]
        })
    }

    fn script_frame_capture(transport: &SnapshotTransport, final_loader_id: &str) {
        for loader_id in ["child-loader", "child-loader", final_loader_id] {
            transport.push("Page.getFrameTree", frame_tree(loader_id));
            transport.push("Target.getTargets", json!({"targetInfos": []}));
        }
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
        transport.push("DOMSnapshot.captureSnapshot", multi_document_snapshot());
    }

    fn script_opaque_frame_query(transport: &SnapshotTransport) {
        transport.push("Page.getFrameTree", opaque_frame_tree("child-loader"));
        transport.push("Target.getTargets", json!({"targetInfos": []}));
        transport.push("Page.getFrameTree", opaque_frame_tree("child-loader"));
        transport.push("Target.getTargets", json!({"targetInfos": []}));
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
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
    fn structural_web_area_is_not_actionable_from_a_generic_focusable_signal() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let response = json!({"nodes":[
            {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"Example"},"backendDOMNodeId":1,"childIds":["button"],"properties":[{"name":"focusable","value":{"value":true}}]},
            {"nodeId":"button","ignored":false,"role":{"value":"button"},"name":{"value":"Save"},"backendDOMNodeId":7,"properties":[{"name":"focusable","value":{"value":true}}]}
        ]});
        let (nodes, bindings, omitted) = decode_ax_tree(&response, target(), generation).unwrap();
        assert_eq!(omitted, 0);
        assert_eq!(nodes.len(), 2);
        assert!(!nodes[0].actionable);
        assert!(nodes[0].reference.is_none());
        assert!(nodes[1].actionable);
        assert_eq!(bindings.len(), 1);
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
                false,
                None,
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
    async fn opaque_same_process_frame_query_matches_its_inventory_access_label() {
        let transport = SnapshotTransport::default();
        script_opaque_frame_query(&transport);
        let mut control = page_control();
        let bound = frame_bound();
        let frames = control.list_frames(&transport, &bound).await.unwrap();
        let child = &frames.frames[1];
        assert_eq!(
            child.access,
            krometrail_core::FrameAccess::SameOriginSameProcess
        );

        let mut request = QueryPageRequest::new(
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
        request.document = krometrail_core::SemanticDocumentScope::Frame(child.reference.clone());

        let BrowserOperationResult::QueryPage(result) = control
            .query_page(
                &transport,
                &bound,
                request,
                krometrail_core::SessionTime::ZERO,
            )
            .await
            .unwrap()
        else {
            panic!("expected query result");
        };
        assert_eq!(
            result.outcome,
            krometrail_core::SemanticQueryOutcome::Unique
        );
    }

    #[tokio::test]
    async fn same_origin_frame_snapshot_exposes_non_actionable_semantic_content() {
        let transport = SnapshotTransport::default();
        for _ in 0..2 {
            transport.push("Page.getFrameTree", frame_tree("child-loader"));
            transport.push("Target.getTargets", json!({"targetInfos": []}));
        }
        transport.push(
            "Accessibility.getFullAXTree",
            json!({"nodes":[
                {"nodeId":"child-root","frameId":"child","ignored":false,"role":{"value":"document"},"childIds":["editor","text"]},
                {"nodeId":"editor","frameId":"child","ignored":false,"role":{"value":"generic"},"name":{"value":"Rich Text Area"},"backendDOMNodeId":106},
                {"nodeId":"text","frameId":"child","ignored":false,"role":{"value":"StaticText"},"name":{"value":"Your content goes here."},"backendDOMNodeId":107}
            ]}),
        );
        let mut control = page_control();
        let bound = frame_bound();
        let child = control
            .list_frames(&transport, &bound)
            .await
            .unwrap()
            .frames[1]
            .reference
            .clone();
        let mut request = SnapshotPageRequest::new(target());
        request.document = krometrail_core::SemanticDocumentScope::Frame(child);

        let BrowserOperationResult::SnapshotPage(snapshot) = control
            .snapshot(
                &transport,
                &bound,
                request,
                krometrail_core::SessionTime::ZERO,
                false,
            )
            .await
            .unwrap()
        else {
            panic!("expected snapshot result");
        };
        assert!(snapshot.nodes.iter().any(|node| {
            node.name.as_deref() == Some("Your content goes here.") && node.reference.is_none()
        }));
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
    async fn cross_origin_and_oopif_frame_labels_match_query_rejection() {
        let transport = SnapshotTransport::default();
        for (root_url, child_url, targets) in [
            (
                "https://example.test/",
                "https://other.test/child",
                json!({"targetInfos": []}),
            ),
            (
                "https://example.test/",
                "https://example.test/child-oopif",
                json!({"targetInfos": [{"type":"iframe","targetId":"child"}]}),
            ),
        ] {
            transport.push(
                "Page.getFrameTree",
                frame_tree_with_urls("child-loader", root_url, child_url),
            );
            transport.push("Target.getTargets", targets.clone());
            transport.push(
                "Page.getFrameTree",
                frame_tree_with_urls("child-loader", root_url, child_url),
            );
            transport.push("Target.getTargets", targets);
        }

        let mut control = page_control();
        let bound = frame_bound();
        for expected_access in [
            krometrail_core::FrameAccess::CrossOrigin,
            krometrail_core::FrameAccess::OutOfProcess,
        ] {
            let child = control
                .list_frames(&transport, &bound)
                .await
                .unwrap()
                .frames[1]
                .clone();
            assert_eq!(child.access, expected_access);
            let mut request = QueryPageRequest::new(
                krometrail_core::PageSelection::Target(target()),
                SemanticQuery::role("heading", None).unwrap(),
                None,
                10,
            )
            .unwrap();
            request.document = krometrail_core::SemanticDocumentScope::Frame(child.reference);
            let error = control
                .query_page(
                    &transport,
                    &bound,
                    request,
                    krometrail_core::SessionTime::ZERO,
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Unsupported);
        }
    }

    #[tokio::test]
    async fn role_name_frame_query_uses_ax_without_capturing_dom_semantics() {
        let transport = SnapshotTransport::default();
        for _ in 0..2 {
            transport.push("Page.getFrameTree", frame_tree("child-loader"));
            transport.push("Target.getTargets", json!({"targetInfos": []}));
        }
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
        let mut control = page_control();
        let bound = frame_bound();
        let child = control
            .list_frames(&transport, &bound)
            .await
            .unwrap()
            .frames[1]
            .reference
            .clone();
        let mut request = QueryPageRequest::new(
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
        request.document = krometrail_core::SemanticDocumentScope::Frame(child);

        let BrowserOperationResult::QueryPage(result) = control
            .query_page(
                &transport,
                &bound,
                request,
                krometrail_core::SessionTime::ZERO,
            )
            .await
            .unwrap()
        else {
            panic!("expected query result");
        };
        assert_eq!(
            result.outcome,
            krometrail_core::SemanticQueryOutcome::Unique
        );
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .all(|(method, _)| method != "DOMSnapshot.captureSnapshot")
        );
    }

    #[tokio::test]
    async fn ordinary_snapshot_does_not_capture_dom_layout() {
        let transport = SnapshotTransport::default();
        transport.push("Page.getFrameTree", frame_tree("main-loader"));
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
        let mut control = page_control();
        control
            .snapshot(
                &transport,
                &frame_bound(),
                SnapshotPageRequest::new(target()),
                krometrail_core::SessionTime::ZERO,
                false,
            )
            .await
            .unwrap();
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .all(|(method, _)| method != "DOMSnapshot.captureSnapshot")
        );
    }

    #[test]
    fn node_limit_errors_name_actual_limit_and_scope_specific_recovery() {
        let query_error = snapshot_node_limit_error(target(), 5_001, true);
        assert!(query_error.message.as_str().contains("5001"));
        assert!(query_error.message.as_str().contains("5000"));
        assert!(
            query_error
                .recovery
                .as_ref()
                .unwrap()
                .as_str()
                .contains("semantic query")
        );
        let geometry_error = snapshot_node_limit_error(target(), 5_001, false);
        assert!(
            geometry_error
                .recovery
                .as_ref()
                .unwrap()
                .as_str()
                .contains("smaller document snapshot")
        );
        assert_eq!(geometry_error.retry, RetryAdvice::Never);
    }

    #[tokio::test]
    async fn geometry_over_cap_omits_layout_and_keeps_snapshot_available() {
        let transport = SnapshotTransport::default();
        transport.push("Page.getFrameTree", frame_tree("main-loader"));
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
        transport.push(
            "DOMSnapshot.captureSnapshot",
            multi_document_snapshot_with_large_parent(),
        );
        transport.push("Page.getFrameTree", frame_tree("main-loader"));
        let mut control = page_control();
        let BrowserOperationResult::SnapshotPage(snapshot) = control
            .snapshot(
                &transport,
                &frame_bound(),
                SnapshotPageRequest::new(target()),
                krometrail_core::SessionTime::ZERO,
                true,
            )
            .await
            .unwrap()
        else {
            panic!("expected snapshot result");
        };

        assert!(snapshot.geometry_omitted);
        assert!(snapshot.nodes.iter().any(|node| node.actionable));
        assert!(
            snapshot
                .nodes
                .iter()
                .all(|node| node.document_rect.is_none())
        );
    }

    #[tokio::test]
    async fn viewport_anchor_captures_visual_viewport_with_snapshot_geometry() {
        let transport = SnapshotTransport::default();
        transport.push("Page.getFrameTree", frame_tree("main-loader"));
        let mut ax = child_ax_tree();
        ax["nodes"][1]["backendDOMNodeId"] = json!(10);
        transport.push("Accessibility.getFullAXTree", ax);
        let mut dom = semantic_dom_snapshot();
        dom["documents"][0]["layout"] = json!({
            "nodeIndex": [2],
            "text": [-1],
            "bounds": [[0.0, 10.0, 20.0, 20.0]]
        });
        transport.push("DOMSnapshot.captureSnapshot", dom);
        transport.push("Page.getFrameTree", frame_tree("main-loader"));
        transport.push(
            "Page.getLayoutMetrics",
            json!({
                "cssVisualViewport": {
                    "pageX": 0.0,
                    "pageY": 0.0,
                    "clientWidth": 100.0,
                    "clientHeight": 100.0
                }
            }),
        );

        let mut control = page_control();
        let mut request = SnapshotPageRequest::new(target());
        request.anchor = SnapshotPageAnchor::Viewport;
        let BrowserOperationResult::SnapshotPage(snapshot) = control
            .snapshot(
                &transport,
                &frame_bound(),
                request,
                krometrail_core::SessionTime::ZERO,
                false,
            )
            .await
            .unwrap()
        else {
            panic!("expected snapshot result");
        };

        assert_eq!(snapshot.visual_viewport.unwrap().size.width, 100.0);
        assert_eq!(snapshot.visual_viewport.unwrap().origin.x, 0.0);
        assert_eq!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "Page.getLayoutMetrics")
                .count(),
            1
        );
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.document_rect.is_some())
        );
    }

    #[tokio::test]
    async fn dom_query_limits_only_the_selected_frame_document() {
        let transport = SnapshotTransport::default();
        for _ in 0..3 {
            transport.push("Page.getFrameTree", frame_tree("child-loader"));
            transport.push("Target.getTargets", json!({"targetInfos": []}));
        }
        transport.push("Accessibility.getFullAXTree", child_ax_tree());
        transport.push(
            "DOMSnapshot.captureSnapshot",
            multi_document_snapshot_with_large_parent(),
        );
        let mut control = page_control();
        let bound = frame_bound();
        let child = control
            .list_frames(&transport, &bound)
            .await
            .unwrap()
            .frames[1]
            .reference
            .clone();
        let mut request = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            SemanticQuery::Text {
                text: krometrail_core::SemanticTextMatch::new(
                    "Nested heading",
                    krometrail_core::SemanticTextMatchMode::Exact,
                    false,
                )
                .unwrap(),
            },
            None,
            10,
        )
        .unwrap();
        request.document = krometrail_core::SemanticDocumentScope::Frame(child);

        let BrowserOperationResult::QueryPage(result) = control
            .query_page(
                &transport,
                &bound,
                request,
                krometrail_core::SessionTime::ZERO,
            )
            .await
            .unwrap()
        else {
            panic!("expected query result");
        };
        assert_eq!(
            result.outcome,
            krometrail_core::SemanticQueryOutcome::Unique
        );
        assert_eq!(result.matches[0].name.as_deref(), Some("Nested heading"));
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
                    false,
                    None,
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

    fn over_cap_generic_container_dom_snapshot() -> Value {
        let over_cap_text = "x".repeat(krometrail_core::MAX_GENERIC_CONTAINER_TEXT_BYTES + 1);
        json!({
            "strings": ["main", "DIV", "#text", over_cap_text],
            "documents": [{
                "frameId": 0,
                "nodes": {
                    "parentIndex": [-1, 0, 1, 1],
                    "nodeName": [1, 1, 2, 1],
                    "backendNodeId": [1, 7, 8, 9],
                    "attributes": [[], [], [], []]
                },
                "layout": {"nodeIndex": [2], "text": [3]}
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
    fn dom_snapshot_geometry_joins_layout_bounds_to_backend_nodes() {
        let mut snapshot = semantic_dom_snapshot();
        snapshot["documents"][0]["layout"]["bounds"] = Value::Array(vec![
            json!([0.0, 0.0, 800.0, 600.0]),
            json!([0.0, 10.0, 120.0, 24.0]),
            json!([0.0, 40.0, 120.0, 24.0]),
            json!([0.0, 70.0, 120.0, 24.0]),
            json!([0.0, 100.0, 120.0, 24.0]),
            json!([0.0, 130.0, 120.0, 24.0]),
        ]);
        let decoded = decode_dom_snapshot_with_geometry(
            &snapshot,
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
            true,
            None,
        )
        .unwrap();
        assert!(!decoded.geometry_omitted);
        assert_eq!(decoded.document_rects[&31].origin.y, 40.0);
        assert_eq!(decoded.document_rects[&31].size.height, 24.0);
    }

    #[test]
    fn over_cap_viewport_decode_keeps_intersecting_geometry_and_metadata() {
        let viewport = CssRect::new(
            CssPoint::new(0.0, 0.0).unwrap(),
            CssSize::new(100.0, 100.0).unwrap(),
        )
        .unwrap();
        let snapshot = large_viewport_dom_snapshot(11);
        let decoded = decode_dom_snapshot_with_geometry(
            &snapshot,
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
            true,
            Some(viewport),
        )
        .unwrap();
        assert!(!decoded.geometry_omitted);
        assert_eq!(decoded.document_rects.len(), 11);
        assert_eq!(decoded.metadata[&11].test_id.as_deref(), Some("on-screen"));

        let omitted = decode_dom_snapshot_with_geometry(
            &snapshot,
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
            true,
            None,
        )
        .unwrap();
        assert!(omitted.geometry_omitted);
        assert!(omitted.document_rects.is_empty());
    }

    #[test]
    fn over_cap_viewport_decode_accounts_for_selection_truncation() {
        let viewport = CssRect::new(
            CssPoint::new(0.0, 0.0).unwrap(),
            CssSize::new(10_000.0, 100.0).unwrap(),
        )
        .unwrap();
        let decoded = decode_dom_snapshot_with_geometry(
            &large_viewport_dom_snapshot(MAX_SNAPSHOT_NODES + 1),
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
            true,
            Some(viewport),
        )
        .unwrap();
        assert!(decoded.geometry_omitted);
        assert_eq!(decoded.document_rects.len(), MAX_SNAPSHOT_NODES);
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
    fn container_role_queries_preserve_authority_and_support_generic_ancestors() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let first_container = SnapshotNodeId::new(2).unwrap();
        let first_checkbox = SnapshotNodeId::new(3).unwrap();
        let second_container = SnapshotNodeId::new(4).unwrap();
        let second_checkbox = SnapshotNodeId::new(5).unwrap();
        let main = SnapshotNodeId::new(6).unwrap();
        let generic_wrapper = SnapshotNodeId::new(7).unwrap();
        let unrelated_text = SnapshotNodeId::new(8).unwrap();
        let uncontained_checkbox = SnapshotNodeId::new(9).unwrap();
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
            document_rect: None,
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
                node(main, Some(root), 1, "main", false),
                node(generic_wrapper, Some(main), 2, "generic", false),
                node(
                    unrelated_text,
                    Some(generic_wrapper),
                    3,
                    "StaticText",
                    false,
                ),
                node(
                    uncontained_checkbox,
                    Some(generic_wrapper),
                    3,
                    "checkbox",
                    true,
                ),
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
                    (
                        first_checkbox,
                        NodeBinding {
                            backend_node_id: 3,
                            expectation_role: ExpectationTargetRole::Checkbox,
                        },
                    ),
                    (
                        second_checkbox,
                        NodeBinding {
                            backend_node_id: 5,
                            expectation_role: ExpectationTargetRole::Checkbox,
                        },
                    ),
                    (
                        uncontained_checkbox,
                        NodeBinding {
                            backend_node_id: 9,
                            expectation_role: ExpectationTargetRole::Checkbox,
                        },
                    ),
                ]),
                node_by_backend: HashMap::from([
                    (1, root),
                    (2, first_container),
                    (3, first_checkbox),
                    (4, second_container),
                    (5, second_checkbox),
                    (6, main),
                    (7, generic_wrapper),
                    (8, unrelated_text),
                    (9, uncontained_checkbox),
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
                            rendered_text: "First item".into(),
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
                    (
                        main,
                        SemanticNodeMetadata {
                            rendered_text: "Unrelated sibling text".into(),
                            ..Default::default()
                        },
                    ),
                    (
                        generic_wrapper,
                        SemanticNodeMetadata {
                            rendered_text: "Buy milk".into(),
                            ..Default::default()
                        },
                    ),
                    (
                        unrelated_text,
                        SemanticNodeMetadata {
                            rendered_text: "Unrelated sibling text".into(),
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
                    (main, Some(root)),
                    (generic_wrapper, Some(main)),
                    (unrelated_text, Some(generic_wrapper)),
                    (uncontained_checkbox, Some(generic_wrapper)),
                ]),
                dom_semantics_captured: true,
                next_node_id: 9,
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
                .query(&bound, &request("First item"), &snapshot)
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
        let page_text = registry
            .query(&bound, &request("Page-wide unrelated text"), &snapshot)
            .unwrap();
        assert_eq!(
            page_text.outcome,
            krometrail_core::SemanticQueryOutcome::NoMatch
        );
        assert_eq!(
            page_text
                .uncontained_match_candidates
                .expect("the role matches when the container qualifier is dropped")
                .count,
            3
        );
        let shared_page_text = QueryPageRequest::new(
            krometrail_core::PageSelection::Target(target()),
            SemanticQuery::role_in_container(
                "checkbox",
                None,
                krometrail_core::SemanticTextMatch::new(
                    "Buy",
                    krometrail_core::SemanticTextMatchMode::Contains,
                    false,
                )
                .unwrap(),
            )
            .unwrap(),
            None,
            20,
        )
        .unwrap();
        let shared_page_text_result = registry
            .query(&bound, &shared_page_text, &snapshot)
            .unwrap();
        assert_eq!(
            shared_page_text_result.outcome,
            krometrail_core::SemanticQueryOutcome::Unique
        );
        assert_eq!(
            shared_page_text_result.matches[0].reference.node_id,
            uncontained_checkbox
        );

        // Both explanations can coexist: the exact container text misses, while the relaxed
        // container and the unqualified role each identify useful follow-up candidates.
        let both_diagnostics = registry
            .query(
                &bound,
                &QueryPageRequest::new(
                    krometrail_core::PageSelection::Target(target()),
                    SemanticQuery::role_in_container(
                        "checkbox",
                        None,
                        krometrail_core::SemanticTextMatch::new(
                            "Buy",
                            krometrail_core::SemanticTextMatchMode::Exact,
                            false,
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                    None,
                    20,
                )
                .unwrap(),
                &snapshot,
            )
            .unwrap();
        assert_eq!(
            both_diagnostics.outcome,
            krometrail_core::SemanticQueryOutcome::NoMatch
        );
        assert_eq!(
            both_diagnostics
                .relaxed_match_candidates
                .expect("the contains relaxation reaches the generic row")
                .count,
            1
        );
        assert_eq!(
            both_diagnostics
                .uncontained_match_candidates
                .expect("the stripped role reaches all checkboxes")
                .count,
            3
        );

        // Once the generic row exceeds the bound it no longer qualifies, but its control remains
        // visible in the uncontained diagnostic.
        let decoded = decode_dom_snapshot(
            &over_cap_generic_container_dom_snapshot(),
            &DocumentFingerprint {
                frame_id: "main".into(),
                loader_id: "loader".into(),
            },
            target(),
        )
        .unwrap();
        let over_cap_metadata = decoded
            .get(&7)
            .cloned()
            .expect("DOM decoder retains the generic wrapper metadata");
        assert_eq!(
            over_cap_metadata.rendered_text.len(),
            krometrail_core::MAX_SEMANTIC_QUERY_TEXT_BYTES
        );
        assert!(
            over_cap_metadata.collapsed_text_bytes
                > krometrail_core::MAX_GENERIC_CONTAINER_TEXT_BYTES
        );
        registry
            .targets
            .get_mut(&target())
            .unwrap()
            .active
            .as_mut()
            .unwrap()
            .semantic
            .insert(generic_wrapper, over_cap_metadata);
        let capped = registry
            .query(&bound, &shared_page_text, &snapshot)
            .unwrap();
        assert_eq!(
            capped.outcome,
            krometrail_core::SemanticQueryOutcome::NoMatch
        );
        assert_eq!(
            capped
                .uncontained_match_candidates
                .expect("the role still matches outside a qualifying container")
                .count,
            3
        );
    }

    #[test]
    fn generic_container_walk_skips_transparent_wrappers_and_excludes_page_text() {
        let id = |value| SnapshotNodeId::new(value).unwrap();
        let node = |id: SnapshotNodeId, parent: Option<SnapshotNodeId>, role: &str| SnapshotNode {
            id,
            parent,
            depth: 0,
            role: role.into(),
            name: None,
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        };
        let nodes = vec![
            node(id(1), None, "document"),
            node(id(2), Some(id(1)), "presentation"),
            node(id(3), Some(id(2)), "none"),
            node(id(4), Some(id(3)), "checkbox"),
        ];
        let parents = HashMap::from([
            (id(1), None),
            (id(2), Some(id(1))),
            (id(3), Some(id(2))),
            (id(4), Some(id(3))),
        ]);
        let semantic = HashMap::from([(
            id(2),
            SemanticNodeMetadata {
                rendered_text: "Buy milk".into(),
                ..Default::default()
            },
        )]);
        let expected = krometrail_core::SemanticTextMatch::new(
            "Buy milk",
            krometrail_core::SemanticTextMatchMode::Exact,
            false,
        )
        .unwrap();
        assert!(nearest_container_text_matches(
            id(4),
            &expected,
            &parents,
            &semantic,
            &nodes
        ));

        let authority_nodes = vec![
            node(id(1), None, "document"),
            node(id(2), Some(id(1)), "generic"),
            node(id(3), Some(id(2)), "listitem"),
            node(id(4), Some(id(3)), "checkbox"),
        ];
        let authority_parents = HashMap::from([
            (id(1), None),
            (id(2), Some(id(1))),
            (id(3), Some(id(2))),
            (id(4), Some(id(3))),
        ]);
        let authority_semantic = HashMap::from([
            (
                id(2),
                SemanticNodeMetadata {
                    rendered_text: "Buy milk".into(),
                    ..Default::default()
                },
            ),
            (
                id(3),
                SemanticNodeMetadata {
                    rendered_text: "Ship release".into(),
                    ..Default::default()
                },
            ),
        ]);
        assert!(!nearest_container_text_matches(
            id(4),
            &expected,
            &authority_parents,
            &authority_semantic,
            &authority_nodes
        ));

        let generic_root = vec![
            node(id(1), None, "document"),
            node(id(2), Some(id(1)), "generic"),
            node(id(3), Some(id(2)), "checkbox"),
        ];
        let generic_root_parents =
            HashMap::from([(id(1), None), (id(2), Some(id(1))), (id(3), Some(id(2)))]);
        let generic_root_semantic = HashMap::from([
            (
                id(1),
                SemanticNodeMetadata {
                    rendered_text: "Buy milk".into(),
                    ..Default::default()
                },
            ),
            (
                id(2),
                SemanticNodeMetadata {
                    rendered_text: "x"
                        .repeat(krometrail_core::MAX_GENERIC_CONTAINER_TEXT_BYTES + 1),
                    ..Default::default()
                },
            ),
        ]);
        assert!(!nearest_container_text_matches(
            id(3),
            &expected,
            &generic_root_parents,
            &generic_root_semantic,
            &generic_root
        ));
    }

    /// Reproduces the decorated-accessible-name case: `role=link, name={exact "Cargo.toml"}`
    /// finds nothing on a page whose real name is `"Cargo.toml, (File)"`. The empty result must
    /// say how many nodes a `contains` retry would reach, and must stay silent when the query
    /// matched or when relaxing would change nothing.
    #[test]
    fn exact_no_match_reports_how_many_nodes_a_contains_retry_would_reach() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let reference = |node_id| NodeReference {
            target_id: target(),
            generation,
            node_id,
        };
        let node = |id: SnapshotNodeId, role: &str, name: &str| SnapshotNode {
            id,
            parent: (id != root).then_some(root),
            depth: u16::from(id != root),
            role: role.into(),
            name: (!name.is_empty()).then(|| name.to_owned()),
            value: None,
            description: None,
            properties: vec![],
            actionable: id != root,
            reference: (id != root).then(|| reference(id)),
            document_rect: None,
        };
        let nodes = vec![
            node(root, "document", ""),
            node(
                SnapshotNodeId::new(2).unwrap(),
                "link",
                "Cargo.toml, (File)",
            ),
            node(
                SnapshotNodeId::new(3).unwrap(),
                "link",
                "Cargo.lock, (File)",
            ),
            node(
                SnapshotNodeId::new(4).unwrap(),
                "link",
                "vendor/Cargo.toml, (File)",
            ),
            // A different role must not be counted as a relaxed candidate.
            node(SnapshotNodeId::new(5).unwrap(), "button", "Cargo.toml"),
        ];
        let context = ObservationContext::new(
            krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(9)),
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
                bindings: HashMap::new(),
                node_by_backend: HashMap::new(),
                semantic: HashMap::new(),
                parent_by_node: HashMap::from([
                    (root, None),
                    (SnapshotNodeId::new(2).unwrap(), Some(root)),
                    (SnapshotNodeId::new(3).unwrap(), Some(root)),
                    (SnapshotNodeId::new(4).unwrap(), Some(root)),
                    (SnapshotNodeId::new(5).unwrap(), Some(root)),
                ]),
                dom_semantics_captured: false,
                next_node_id: 5,
            },
        );
        let query = |value: &str, mode| {
            SemanticQuery::role(
                "link",
                Some(krometrail_core::SemanticTextMatch::new(value, mode, false).unwrap()),
            )
            .unwrap()
        };
        let request = |query| {
            QueryPageRequest::new(
                krometrail_core::PageSelection::Target(target()),
                query,
                None,
                20,
            )
            .unwrap()
        };

        let exact = registry
            .query(
                &bound,
                &request(query(
                    "Cargo.toml",
                    krometrail_core::SemanticTextMatchMode::Exact,
                )),
                &snapshot,
            )
            .unwrap();
        assert_eq!(
            exact.outcome,
            krometrail_core::SemanticQueryOutcome::NoMatch
        );
        let candidates = exact
            .relaxed_match_candidates
            .expect("an exact no-match reports its relaxed candidate count");
        assert_eq!(candidates.count, 2);
        assert!(!candidates.saturated);
        assert!(exact.uncontained_match_candidates.is_none());

        // The relaxed query itself matches, so there is nothing to explain.
        let relaxed = registry
            .query(
                &bound,
                &request(query(
                    "Cargo.toml",
                    krometrail_core::SemanticTextMatchMode::Contains,
                )),
                &snapshot,
            )
            .unwrap();
        assert_eq!(relaxed.matches.len(), 2);
        assert!(relaxed.relaxed_match_candidates.is_none());
        assert!(relaxed.uncontained_match_candidates.is_none());

        // An exact no-match whose relaxation also matches nothing stays silent.
        let hopeless = registry
            .query(
                &bound,
                &request(query(
                    "Makefile",
                    krometrail_core::SemanticTextMatchMode::Exact,
                )),
                &snapshot,
            )
            .unwrap();
        assert!(hopeless.relaxed_match_candidates.is_none());
    }

    #[test]
    fn exact_role_name_and_text_match_names_carrying_invisible_codepoints() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let zwsp = SnapshotNodeId::new(2).unwrap();
        let pua = SnapshotNodeId::new(3).unwrap();
        let reference = |node_id| NodeReference {
            target_id: target(),
            generation,
            node_id,
        };
        let node = |id, name: &str| SnapshotNode {
            id,
            parent: (id != root).then_some(root),
            depth: u16::from(id != root),
            role: if id == root { "document" } else { "button" }.into(),
            name: (!name.is_empty()).then(|| name.to_owned()),
            value: None,
            description: None,
            properties: vec![],
            actionable: id != root,
            reference: (id != root).then(|| reference(id)),
            document_rect: None,
        };
        let zwsp_name = "Advanced filters\u{200b}";
        let pua_name = "Filters \u{e5cf}";
        let nodes = vec![node(root, ""), node(zwsp, zwsp_name), node(pua, pua_name)];
        let context = ObservationContext::new(
            krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(10)),
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
                bindings: HashMap::new(),
                node_by_backend: HashMap::new(),
                semantic: HashMap::from([
                    (
                        root,
                        SemanticNodeMetadata {
                            rendered_text: format!("{zwsp_name} {pua_name}"),
                            ..Default::default()
                        },
                    ),
                    (
                        zwsp,
                        SemanticNodeMetadata {
                            rendered_text: zwsp_name.into(),
                            ..Default::default()
                        },
                    ),
                    (
                        pua,
                        SemanticNodeMetadata {
                            rendered_text: pua_name.into(),
                            ..Default::default()
                        },
                    ),
                ]),
                parent_by_node: HashMap::from([
                    (root, None),
                    (zwsp, Some(root)),
                    (pua, Some(root)),
                ]),
                dom_semantics_captured: true,
                next_node_id: 3,
            },
        );
        let request = |query| {
            QueryPageRequest::new(
                krometrail_core::PageSelection::Target(target()),
                query,
                None,
                20,
            )
            .unwrap()
        };
        for (expected, observed) in [("Advanced filters", zwsp_name), ("Filters", pua_name)] {
            let role = registry
                .query(
                    &bound,
                    &request(
                        SemanticQuery::role(
                            "button",
                            Some(
                                krometrail_core::SemanticTextMatch::new(
                                    expected,
                                    krometrail_core::SemanticTextMatchMode::Exact,
                                    false,
                                )
                                .unwrap(),
                            ),
                        )
                        .unwrap(),
                    ),
                    &snapshot,
                )
                .unwrap();
            assert_eq!(
                role.outcome,
                krometrail_core::SemanticQueryOutcome::Unique,
                "{observed:?}"
            );

            let text = registry
                .query(
                    &bound,
                    &request(SemanticQuery::Text {
                        text: krometrail_core::SemanticTextMatch::new(
                            expected,
                            krometrail_core::SemanticTextMatchMode::Exact,
                            false,
                        )
                        .unwrap(),
                    }),
                    &snapshot,
                )
                .unwrap();
            assert_eq!(
                text.outcome,
                krometrail_core::SemanticQueryOutcome::Unique,
                "{observed:?}"
            );
        }
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
            document_rect: None,
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
                    (
                        scope,
                        NodeBinding {
                            backend_node_id: 2,
                            expectation_role: ExpectationTargetRole::Other,
                        },
                    ),
                    (
                        first,
                        NodeBinding {
                            backend_node_id: 3,
                            expectation_role: ExpectationTargetRole::Other,
                        },
                    ),
                    (
                        second,
                        NodeBinding {
                            backend_node_id: 4,
                            expectation_role: ExpectationTargetRole::Other,
                        },
                    ),
                    (
                        outside,
                        NodeBinding {
                            backend_node_id: 5,
                            expectation_role: ExpectationTargetRole::Other,
                        },
                    ),
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
                dom_semantics_captured: true,
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
    fn node_state_facts_parse_and_degrade_per_field() {
        let full = json!({
            "connected": true,
            "checked": false,
            "ariaExpanded": true,
            "selected": false,
            "pressed": true,
            "valueLength": 12,
        });
        assert_eq!(
            parse_node_state_facts(&full),
            krometrail_core::NodeStateFacts {
                connected: true,
                checked: Some(false),
                expanded: Some(true),
                selected: Some(false),
                pressed: Some(true),
                value_length: Some(12),
            }
        );
        // Guarded properties that could not be read arrive as null and each
        // degrades independently; non-boolean noise degrades the same way.
        let partial = json!({
            "connected": true,
            "checked": null,
            "ariaExpanded": "mixed",
            "valueLength": null,
        });
        assert_eq!(
            parse_node_state_facts(&partial),
            krometrail_core::NodeStateFacts {
                connected: true,
                ..krometrail_core::NodeStateFacts::default()
            }
        );
        // A wholesale-degraded payload yields a disconnected, all-unobserved
        // fact set rather than an error.
        assert_eq!(
            parse_node_state_facts(&json!({})),
            krometrail_core::NodeStateFacts::default()
        );
        // Oversized value lengths saturate instead of vanishing.
        assert_eq!(
            parse_node_state_facts(&json!({"connected": true, "valueLength": u64::MAX}))
                .value_length,
            Some(u32::MAX)
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
    fn temporal_input_kinds_match_browser_input_types_and_formats() {
        assert_eq!(
            TemporalInputKind::from_input_type("date").map(TemporalInputKind::expected_format),
            Some("YYYY-MM-DD")
        );
        assert_eq!(
            TemporalInputKind::from_input_type("datetime-local")
                .map(TemporalInputKind::expected_format),
            Some("YYYY-MM-DDTHH:MM[:SS]")
        );
        assert_eq!(
            TemporalInputKind::from_input_type("week").map(TemporalInputKind::input_type),
            Some("week")
        );
        assert_eq!(TemporalInputKind::from_input_type("text"), None);
    }

    #[tokio::test]
    async fn file_input_resolution_canonicalizes_affordance_without_geometry() {
        let transport = SnapshotTransport::default();
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        transport.push(
            "DOM.resolveNode",
            json!({"object":{"objectId":"affordance"}}),
        );
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": false,
                    "interactionBlocked": false,
                    "isFileInput": false,
                }}
            }),
        );
        transport.push(
            "Runtime.callFunctionOn",
            json!({"result":{"result":{"type":"object","objectId":"associated"}}}),
        );
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":20}}));
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":20}}));
        transport.push("DOM.resolveNode", json!({"object":{"objectId":"file"}}));
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": true,
                    "interactionBlocked": false,
                    "isFileInput": true,
                }}
            }),
        );
        let scope = CommandScope::Session(TransportSessionId::new("session-a").unwrap());
        let resolved = resolve_backend_node(
            &transport,
            &scope,
            target(),
            10,
            None,
            ReferenceRequirement::FileInput,
        )
        .await
        .unwrap();
        assert_eq!(resolved.backend_node_id, 20);
        assert!(resolved.document_quad.is_none());
        let methods = transport
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|(method, _)| method.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            methods,
            [
                "DOM.describeNode".to_owned(),
                "DOM.resolveNode".to_owned(),
                "Runtime.callFunctionOn".to_owned(),
                "Runtime.callFunctionOn".to_owned(),
                "DOM.describeNode".to_owned(),
                "DOM.describeNode".to_owned(),
                "DOM.resolveNode".to_owned(),
                "Runtime.callFunctionOn".to_owned(),
            ]
        );
        let calls = transport.calls.lock().unwrap();
        assert_eq!(calls[3].1["throwOnSideEffect"], json!(false));
        assert_eq!(calls[3].1["returnByValue"], json!(false));
        assert!(calls.iter().all(|(method, _)| method != "DOM.getBoxModel"));
    }

    #[tokio::test]
    async fn editable_host_resolution_promotes_once_and_threads_temporal_input() {
        let transport = SnapshotTransport::default();
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        transport.push("DOM.resolveNode", json!({"object":{"objectId":"segment"}}));
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": false,
                    "interactionBlocked": false,
                    "isEditable": false,
                    "inputType": "text"
                }}
            }),
        );
        transport.push(
            "Runtime.callFunctionOn",
            json!({"result":{"result":{"type":"object","objectId":"host"}}}),
        );
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":20}}));
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":20}}));
        transport.push("DOM.resolveNode", json!({"object":{"objectId":"input"}}));
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": false,
                    "interactionBlocked": false,
                    "isEditable": true,
                    "inputType": "datetime-local"
                }}
            }),
        );
        transport.push(
            "DOM.getBoxModel",
            json!({"model":{"border":[10.0,20.0,30.0,20.0,30.0,40.0,10.0,40.0]}}),
        );
        let scope = CommandScope::Session(TransportSessionId::new("session-a").unwrap());
        let resolved = resolve_backend_node(
            &transport,
            &scope,
            target(),
            10,
            None,
            ReferenceRequirement::Editable,
        )
        .await
        .unwrap();
        assert_eq!(resolved.backend_node_id, 20);
        assert_eq!(
            resolved.temporal_input,
            Some(TemporalInputKind::DatetimeLocal)
        );
        assert!(resolved.document_quad.is_some());
        let calls = transport.calls.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|(_, params)| { params["functionDeclaration"] == EDITABLE_HOST_FUNCTION })
                .count(),
            1
        );
        let host_probe = &calls[3].1;
        assert_eq!(host_probe["throwOnSideEffect"], json!(false));
        assert_eq!(host_probe["returnByValue"], json!(false));
        assert_eq!(
            calls
                .iter()
                .map(|(method, _)| method.as_str())
                .collect::<Vec<_>>(),
            vec![
                "DOM.describeNode",
                "DOM.resolveNode",
                "Runtime.callFunctionOn",
                "Runtime.callFunctionOn",
                "DOM.describeNode",
                "DOM.describeNode",
                "DOM.resolveNode",
                "Runtime.callFunctionOn",
                "DOM.getBoxModel",
            ]
        );
    }

    #[tokio::test]
    async fn editable_kind_miss_keeps_generic_guidance_for_non_temporal_nodes() {
        let transport = SnapshotTransport::default();
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        transport.push("DOM.resolveNode", json!({"object":{"objectId":"node"}}));
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": false,
                    "interactionBlocked": false,
                    "isEditable": false,
                    "inputType": "text"
                }}
            }),
        );
        transport.push(
            "Runtime.callFunctionOn",
            json!({"result":{"result":{"type":"object","objectId":"same"}}}),
        );
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        let scope = CommandScope::Session(TransportSessionId::new("session-a").unwrap());
        let error = resolve_backend_node(
            &transport,
            &scope,
            target(),
            10,
            None,
            ReferenceRequirement::Editable,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ReferenceNotActionable);
        assert_eq!(
            error.message.as_str(),
            "backing node is not valid for the requested interaction"
        );
    }

    #[tokio::test]
    async fn editable_kind_miss_keeps_temporal_guidance_when_host_promotion_falls_back() {
        let transport = SnapshotTransport::default();
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        transport.push("DOM.resolveNode", json!({"object":{"objectId":"segment"}}));
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": false,
                    "interactionBlocked": false,
                    "isEditable": false,
                    "inputType": "date"
                }}
            }),
        );
        transport.push(
            "Runtime.callFunctionOn",
            json!({"result":{"result":{"type":"object","objectId":"same"}}}),
        );
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        let scope = CommandScope::Session(TransportSessionId::new("session-a").unwrap());
        let error = resolve_backend_node(
            &transport,
            &scope,
            target(),
            10,
            None,
            ReferenceRequirement::Editable,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ReferenceNotActionable);
        assert!(
            error
                .message
                .as_str()
                .contains("for a native date/time field")
        );
    }

    #[tokio::test]
    async fn file_input_resolution_reports_guided_error_when_affordance_has_no_association() {
        let transport = SnapshotTransport::default();
        transport.push("DOM.describeNode", json!({"node":{"backendNodeId":10}}));
        transport.push("DOM.resolveNode", json!({"object":{"objectId":"button"}}));
        transport.push(
            "Runtime.callFunctionOn",
            json!({
                "result": {"value": {
                    "connected": true,
                    "visuallyHidden": false,
                    "interactionBlocked": false,
                    "isFileInput": false,
                }}
            }),
        );
        transport.push(
            "Runtime.callFunctionOn",
            json!({"result":{"result":{"type":"object","subtype":"null","value":null}}}),
        );
        let scope = CommandScope::Session(TransportSessionId::new("session-a").unwrap());
        let error = resolve_backend_node(
            &transport,
            &scope,
            target(),
            10,
            None,
            ReferenceRequirement::FileInput,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ReferenceNotActionable);
        assert!(
            error
                .message
                .as_str()
                .contains("upload_target_not_file_input")
        );
        assert!(error.message.as_str().contains("unique sibling input"));
        assert!(
            error
                .recovery
                .as_ref()
                .is_some_and(|value| { value.as_str().contains("input[type=file]") })
        );
    }

    #[test]
    fn every_requirement_rejects_hidden_or_disconnected_nodes() {
        for requirement in [
            ReferenceRequirement::VisibleGeometry,
            ReferenceRequirement::Actionable,
            ReferenceRequirement::Editable,
            ReferenceRequirement::Selectable,
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
        let hidden_file = json!({
            "connected": true,
            "visuallyHidden": true,
            "interactionBlocked": false,
            "isFileInput": true,
        });
        assert!(
            validate_node_state(&hidden_file, ReferenceRequirement::FileInput, target()).is_ok()
        );
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
                    expectation_role: ExpectationTargetRole::Other,
                },
            )]),
            node_by_backend: HashMap::from([(42, node_id)]),
            semantic: HashMap::new(),
            parent_by_node: HashMap::new(),
            dom_semantics_captured: false,
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
                dom_semantics_captured: false,
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

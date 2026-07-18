---
id: epic-browser-interface-hardening-page-context-semantics
kind: feature
stage: implementing
tags: [browser, agent-ux]
parent: epic-browser-interface-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Truthful Page-Context Semantics

## Brief

Repair four places where Krometrail's structured browser model diverges from what an agent can see or recover from: unnamed controls cannot be associated with bounded adjacent/container text, same-origin frame queries select the wrong semantic document, resource timing initiators misclassify obvious script/font URLs, and `focus: preserve` hidden-target failures claim foreground activation occurred and suggest unavailable recovery.

Keep semantic matching deterministic and bounded. Reuse document fingerprints and current query/reference authority, reconcile resource identity at ingestion, and make recovery text accurately reflect the requested focus policy and available operations.

## Source findings

- `idea-associate-unnamed-controls`
- `idea-fix-nested-frame-query`
- `idea-fix-asset-kind-classification`
- `idea-fix-hidden-target-recovery`

## UI alignment

No UI surface; this is a browser-control and error-contract feature.

## Design decisions

- **Unnamed controls**: add optional `container_text` to role queries. It matches rendered text on the candidate's nearest matching DOM/AX ancestor, never arbitrary page-near text.
- **Frame authority**: resolve the frame fingerprint once and use it to select both the AX tree and the matching DOMSnapshot document; reject drift before installing the registry snapshot.
- **Asset identity**: prefer explicit URL extension for unambiguous script/font/image/style/media types, then fall back to `initiatorType` for ambiguous URLs.
- **Hidden recovery**: construct `target_hidden` errors from the requested focus policy. Preserve mode says to retry with `focus: foreground`; foreground mode reports bounded activation failure.

## Architectural choice

Enrich the existing semantic metadata/registry and query language rather than adding a second DOM selector engine. This preserves stable node references and query outcomes. Resource timing remains the inventory source, with a small deterministic identity reconciler. Error recovery remains structured at the interaction preparation boundary.

Alternatives rejected: spatial proximity is unstable and layout-dependent; page JavaScript queries would bypass document/reference freshness; MIME fetching would add network side effects; a foreground tool is unnecessary because the operation's existing focus policy already expresses the recovery.

## Implementation Units

### Unit 1: Container-qualified role queries

**Story**: `epic-browser-interface-hardening-page-context-semantics-container-text`

**Files**: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
SemanticQuery::Role {
    role: NonEmptyText,
    name: Option<SemanticTextMatch>,
    container_text: Option<SemanticTextMatch>,
}

fn nearest_container_text_matches(
    node: SnapshotNodeId,
    expected: &SemanticTextMatch,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic: &HashMap<SnapshotNodeId, SemanticNodeMetadata>,
) -> bool;
```

Retain semantic metadata for mapped non-actionable ancestors, while still returning only referencable candidates. Bound text with existing semantic text limits.

**Acceptance criteria**:

- [ ] An unnamed TodoMVC checkbox can be uniquely queried by role plus its containing todo text.
- [ ] Identical controls in different containers remain distinguishable.
- [ ] A page-level text match does not qualify an unrelated control.

### Unit 2: Fingerprint-aligned frame semantics

**Story**: `epic-browser-interface-hardening-page-context-semantics-frame-query`

**File**: `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
async fn capture_snapshot_for_frame(
    &mut self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    started_at: SessionTime,
    include_semantic: bool,
    frame: Option<&ResolvedFrameDocument>,
) -> Result<PageSnapshot>;
```

Represent the resolved reference, frame id, and loader id as one local resolved-document value. Decode the AX response's actual frame nodes and select the DOMSnapshot document with the same fingerprint; do not resolve the frame twice unless performing the final drift check.

**Acceptance criteria**:

- [ ] A same-origin nested-frame visible heading returns a reference from the qualified document.
- [ ] Main-document queries retain current behavior.
- [ ] Frame navigation during capture returns stale evidence instead of `no_match`.

### Unit 3: Resource-kind reconciliation

**Story**: `epic-browser-interface-hardening-page-context-semantics-asset-kinds`

**File**: `crates/krometrail-cdp/src/control/contexts.rs`

```rust
fn classify_asset(url: &SanitizedUrl, initiator_type: &str) -> PageAssetKind;
```

Recognize case-insensitive URL path extensions after stripping query/fragment. Only override initiator type for unambiguous known extensions.

**Acceptance criteria**:

- [ ] `.js`/`.mjs` resources are scripts and `.woff`/`.woff2` resources are fonts even when initiated by a stylesheet link.
- [ ] CSS, images, media, fetch, and XHR classifications retain their documented outcomes.
- [ ] Ambiguous extensionless URLs fall back to initiator type.

### Unit 4: Focus-aware hidden-target recovery

**Story**: `epic-browser-interface-hardening-page-context-semantics-hidden-recovery`

**File**: `crates/krometrail-cdp/src/control/interaction.rs`

```rust
fn target_hidden_error(target_id: TargetId, focus: BrowserFocusPolicy) -> KrometrailError;
```

Attach safe retry/recovery guidance appropriate to the policy without claiming an activation was attempted in preserve mode.

**Acceptance criteria**:

- [ ] Preserve-mode errors say the target is hidden and recommend retrying the operation with `focus: foreground`.
- [ ] Foreground-mode errors state bounded activation failed.
- [ ] No recovery refers to a nonexistent standalone foreground operation.

## Implementation Order

1. Align frame document capture and semantic metadata.
2. Add bounded container text matching on that metadata.
3. Reconcile asset kinds.
4. Correct hidden-target errors.

## Simplification

- Replace tuple-shaped frame state and repeated resolution with one resolved-document value.
- Centralize resource kind inference in one helper.
- Replace the context-free hidden error helper with one policy-aware constructor.

## Testing

- Core serde/schema tests protect additive role-query compatibility and validation.
- CDP snapshot tests use multi-document protocol fixtures and prove bounded ancestor matching.
- Context and interaction unit tests protect URL classification and exact recovery semantics.
- Real Chrome qualification covers TodoMVC and a same-origin nested-frame page.

## Risks

AX and DOMSnapshot node identities can differ across out-of-process frames. Same-origin qualification is required; unsupported cross-origin behavior remains explicit. Container text could overmatch a very large ancestor, so matching stops at the nearest ancestor whose rendered text matches.

## Implementation notes

- Completed the four cohesive child stories: `epic-browser-interface-hardening-page-context-semantics-container-text`, `epic-browser-interface-hardening-page-context-semantics-frame-query`, `epic-browser-interface-hardening-page-context-semantics-asset-kinds`, and `epic-browser-interface-hardening-page-context-semantics-hidden-recovery`.
- Role queries now accept bounded `container_text`; snapshot metadata keeps mapped non-actionable ancestors and walks only the candidate's ancestor chain, excluding page-root text.
- Frame qualification now carries one resolved document fingerprint through AX and DOMSnapshot selection, with a final drift check before the snapshot is installed.
- Resource timing classification centralizes unambiguous, case-insensitive URL-extension inference and otherwise preserves initiator-type behavior. Hidden pointer-target failures now reflect the requested focus policy and name the supported retry.
- The implementation stayed within the existing query registry, resource-timing inventory, and interaction-preparation boundaries. No discrepancies or adjacent issues were found.

## Integrated verification

- `cargo test -p krometrail-core -p krometrail-cdp --all-targets --locked` — passed.
- `cargo clippy -p krometrail-core -p krometrail-cdp --all-targets --locked -- -D warnings` — passed.
- `cargo fmt --all -- --check` — passed.

## Review findings

- Standard single-pass review found one receiver-confirmed blocker: container-text matching walked through generic/page-level ancestors whose propagated rendered text could include an unrelated sibling.
- Closure requires a conservative local-container boundary, a sibling-text regression, and focused verification only; no second reviewer is required.

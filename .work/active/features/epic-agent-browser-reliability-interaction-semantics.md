---
id: epic-agent-browser-reliability-interaction-semantics
kind: feature
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Consistent interaction semantics

## Brief

Correct GitHub issues #7, #8, and #11 through one coherent interaction contract. Replace-mode fill must clear editable controls, including password inputs, without platform-specific shortcut assumptions or secret exposure. Key chords and named activation keys must use canonical validated spellings and normal Chrome semantics, while distinguishing dispatched input from any subsequently observed DOM effect.

Page-scoped requests should default to the selected page and common interaction options should have safe defaults. Structured references remain usable while their attachment, document, and backing node remain valid rather than becoming stale merely because another observation created a snapshot. Element pointer actions consistently prepare off-screen targets by scrolling, then re-resolve and validate viewport geometry before dispatch.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: independent runtime feature; its final request examples are consumed by agent-contract guidance.

## Simplification opportunity
- Centralize editable selection, key event construction, and element preparation so fill, click, hover, and drag do not maintain divergent workarounds.

## Foundation references
- `docs/SPEC.md` — browser-control action contract
- `docs/ARCHITECTURE.md` — page control and snapshot authority

## Design decisions

- **Selected-page defaults**: make `PageSelection::Selected` the deserialization and JSON-Schema
  default for every page-scoped request, and add defaults only for genuinely conventional
  interaction options (`left`, no modifiers, one click, no navigation wait, replace fill, no focus
  locator). Existing explicit requests remain valid, while omission removes walkthrough boilerplate.
- **Key compatibility and canonical output**: continue accepting the existing case-insensitive
  modifier aliases (`ctrl`, `cmd`, `command`) for stable 1.x compatibility, but canonicalize stored
  chords to `Alt|Control|Meta|Shift` plus exactly one canonical named key or character. Reject
  duplicate modifiers and multiple non-modifier keys instead of reporting a misleading dispatch.
- **Fill replacement**: select and clear the focused editable node through its DOM object, dispatch
  a normal Backspace/input path, and verify only that the resulting length is zero before inserting
  the new text. Never return, log, or compare the previous value itself. This works for password,
  text, textarea, and contenteditable nodes without guessing Control versus Meta.
- **Reference lifetime**: treat `SnapshotGeneration` as a target attachment/document epoch, not an
  observation counter. A fresh snapshot of the same document reuses node identifiers for the same
  backend nodes and therefore does not invalidate a still-present actionable reference.
- **Pointer preparation**: element-based click, hover, and both drag endpoints always resolve,
  scroll with `DOM.scrollIntoViewIfNeeded`, re-resolve against the same reference or selector, and
  only then perform viewport validation. Declared coordinates are never auto-scrolled.
- **UI alignment**: no product UI surface is introduced; this is an MCP/domain behavior change, so
  no mockup fallback applies.

## Architectural options

### Option A: Patch each failing call site

Choose Meta for macOS fill, special-case Enter, retain several snapshot generations, and scroll only
failed clicks. This is small initially, but preserves divergent keyboard, reference, and pointer
authorities and leaves hover/drag behavior inconsistent. Rejected.

### Option B: Browser-native input plus document-scoped interaction authority (chosen)

Canonicalize key chords at the core boundary, construct CDP key events from one description table,
clear editables without platform shortcuts, make snapshot identity document-scoped, and centralize
element preparation before pointer dispatch. This adds no external dependency and keeps validation
in core while CDP mechanics stay in the adapter. Chosen because it directly removes the duplicated
workarounds behind #7, #8, and #11.

### Option C: JavaScript-drive all interactions

Set values and call `click()`/dispatch synthetic events in the page. This is easy to make
cross-platform but does not preserve normal browser input semantics, can bypass hit testing and
trusted-event behavior, and weakens Krometrail's control contract. Rejected.

## Implementation Units

### Unit 1: Defaulted page requests and canonical key contracts

**Files**:
- `crates/krometrail-core/src/browser/control.rs`
- `crates/krometrail-core/src/browser/interaction.rs`
- `crates/krometrail-core/src/browser/observation.rs`
- `crates/krometrail-core/src/browser/wait.rs`
- `crates/krometrail-core/src/browser/batch.rs`
- `crates/krometrail-core/src/browser/operation.rs`
- `crates/krometrail-mcp/src/schema.rs`

**Story**: `epic-agent-browser-reliability-interaction-semantics-input-contracts`

```rust
impl Default for PageSelection {
    fn default() -> Self { Self::Selected }
}

impl KeyChord {
    pub fn new(value: impl Into<String>) -> Result<Self>;
    pub fn as_str(&self) -> &str;          // canonical spelling
    pub fn segments(&self) -> Vec<KeySegment>;
}

fn parse_chord(value: &str) -> Result<Vec<KeySegment>>;
fn canonical_chord(segments: &[KeySegment]) -> String;
```

Replace direct derives with constructor-backed wire structs wherever a default is needed. Wire
fields use `#[serde(default)]` and matching schema defaults; constructors remain explicit for Rust
callers. Canonical chords contain zero or more unique modifiers and exactly one non-modifier key.
Keep aliases input-only and serialize the canonical value so generated schemas/examples do not
promote multiple spellings.

**Acceptance criteria**:
- [ ] Omitting `target` from every page-scoped standalone request and batch request resolves the
      selected page; an explicit target continues to select that exact target.
- [ ] Omitted click/fill/key options use the documented safe defaults, while invalid counts,
      empty key sequences, and unsupported key names still fail at deserialization.
- [ ] `META+A`, `cmd+a`, and `Meta+a` deserialize to the same canonical `Meta+a`; duplicate
      modifiers and chords containing two action keys are rejected before CDP dispatch.
- [ ] Registry-derived operation schemas describe the actual accepted defaults and canonical
      constraints without a second hand-maintained operation list.

### Unit 2: One keyboard event builder and secret-safe replacement

**Files**:
- `crates/krometrail-cdp/src/control/keyboard.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/browser_control_qualification.rs` (or the existing real-browser
  control qualification file discovered at implementation time)

**Story**: `epic-agent-browser-reliability-interaction-semantics-input-contracts`

```rust
struct KeyDescription {
    key: String,
    code: String,
    location: u8,
    keycode: u16,
    text: Option<String>,
}

fn describe_key(segment: KeySegment, modifiers: Modifiers) -> KeyDescription;

async fn dispatch_key(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    description: &KeyDescription,
    modifiers: Modifiers,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()>;

async fn clear_editable(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()>;
```

`dispatch_key` sends one key-down event and one key-up event. It uses `keyDown` with `text` and
`unmodifiedText` for text-producing keys (including Enter as `\r` and Space as ` `) and
`rawKeyDown` without a char event when Control, Meta, or Alt suppresses text. Shift changes the
reported character/key text rather than emitting a lowercase char with a Shift mask. Modifiers
remain explicit down/up events around the action key.

`clear_editable` resolves the already-validated backend node to a runtime object, calls a bounded
function that selects input/textarea contents or the contenteditable range without reading the
value, dispatches Backspace, then returns only the remaining character count. A non-zero count
fails with `reference_not_actionable` recovery rather than inserting after unknown secret content.
The previous or requested value is never included in a command expression, trace field, error, or
test failure message; `Input.insertText` remains the only command carrying the requested value.

**Acceptance criteria**:
- [ ] Replacing a non-empty password value leaves exactly the new value, and tests prove this by
      length/submit outcome without printing either secret.
- [ ] Append mode remains append-only and does not execute the clear path.
- [ ] Meta+A and Control+A dispatch modifier-down, a raw non-text `a` key-down/up, and
      modifier-up; no `char` event inserts `a` into the field.
- [ ] Enter submits a focused form control and Space retains its existing activation behavior in
      real Chrome; the result still reports dispatched input separately from observed page effect.

### Unit 3: Document-scoped snapshot bindings

**Files**:
- `crates/krometrail-cdp/src/control/snapshot.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-core/src/browser/observation.rs`
- `docs/SPEC.md`
- `docs/ARCHITECTURE.md`

**Story**: `epic-agent-browser-reliability-interaction-semantics-reference-lifetime`

```rust
struct DocumentSnapshotRegistry {
    generation: SnapshotGeneration,
    attachment_generation: u64,
    document: DocumentFingerprint,
    next_node_id: u32,
    node_by_backend: HashMap<i64, SnapshotNodeId>,
    bindings: HashMap<SnapshotNodeId, NodeBinding>,
}

struct TargetSnapshotRegistry {
    next_generation: u64,
    document: Option<DocumentSnapshotRegistry>,
}

impl SnapshotRegistry {
    fn begin_snapshot(
        &mut self,
        target_id: TargetId,
        attachment_generation: u64,
        document: DocumentFingerprint,
    ) -> Result<SnapshotGeneration>;

    fn install_document_snapshot(
        &mut self,
        target_id: TargetId,
        generation: SnapshotGeneration,
        backend_bindings: HashMap<i64, NodeBinding>,
    ) -> Result<HashMap<i64, SnapshotNodeId>>;
}

fn decode_ax_tree(
    response: &Value,
    target_id: TargetId,
    generation: SnapshotGeneration,
    node_ids: &mut dyn FnMut(i64) -> Result<SnapshotNodeId>,
) -> Result<(Vec<SnapshotNode>, HashMap<i64, NodeBinding>, u32)>;
```

The document fingerprint and attachment generation decide whether to reuse the current epoch or
allocate a new `SnapshotGeneration`. Backend node IDs that remain in the newest full AX tree keep
their `SnapshotNodeId`; disappeared bindings are removed, and a later action still validates the
current document, connected state, actionability, and geometry through CDP. Navigation, document
replacement, reconnect, target close, and backing-node disappearance stay explicit stale-reference
boundaries. Update the foundation wording that currently says every fresh snapshot invalidates the
generation; this feature intentionally corrects that external contract.

**Acceptance criteria**:
- [ ] A reference from snapshot A succeeds after `observe_live` installs snapshot B when the
      attachment, loader/document fingerprint, and backend node remain valid.
- [ ] Reordering or adding AX nodes does not retarget an old reference to a different backend node.
- [ ] Navigation/document replacement, reconnect attachment change, node detachment, and target
      closure return the existing structured stale-reference error and recovery.
- [ ] Registry memory is proportional to the latest bounded full AX tree per live target, not the
      number of observations in the session.

### Unit 4: Resolve-scroll-re-resolve pointer preparation

**Files**:
- `crates/krometrail-cdp/src/control/interaction.rs`
- `crates/krometrail-cdp/src/control/snapshot.rs`
- `crates/krometrail-cdp/src/control/pointer.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/browser_control_qualification.rs` (or the existing real-browser
  control qualification file discovered at implementation time)

**Story**: `epic-agent-browser-reliability-interaction-semantics-pointer-preparation`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ElementPreparation { ResolveOnly, ScrollIntoView }

async fn resolve_interaction_target(
    &self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    locator: Option<&InteractionLocator>,
    requirement: ReferenceRequirement,
    preparation: ElementPreparation,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<ResolvedTarget>;

async fn prepare_element(
    &self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    locator: &ElementLocator,
    requirement: ReferenceRequirement,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<ResolvedNode>;
```

For element pointer actions, resolve once for identity/actionability, call
`DOM.scrollIntoViewIfNeeded`, then resolve the original locator again and compute its fresh quad.
The second resolution catches DOM replacement during scrolling. Validate the center against the
fresh visual viewport and return an actionable recovery error when scrolling cannot expose it.
Fill, key focus, selection, upload, waits, explicit scroll-to-element, and coordinate actions retain
their action-specific preparation. Drag prepares source and destination immediately before their
respective geometry is consumed.

**Acceptance criteria**:
- [ ] Reference and selector clicks on an off-screen actionable control scroll it into view and
      dispatch inside the final viewport.
- [ ] Hover and both drag endpoints use the same preparation contract; pointer actions no longer
      differ by locator kind.
- [ ] A node replaced during scrolling fails as stale/not-actionable rather than clicking the old
      coordinates or a different element.
- [ ] Explicit viewport/document coordinates are hit-tested exactly as declared and never trigger
      implicit scrolling.

### Unit 5: Agent-facing contract and regression documentation

**Files**:
- `docs/SPEC.md`
- `docs/ARCHITECTURE.md`
- `docs/EVALUATION.md`
- `docs/guide/using-krometrail.md`
- `plugin/skills/krometrail/SKILL.md`

Document selected-page defaults, canonical key names/chords, the distinction between dispatch and
observed effect, document-scoped reference lifetime, and automatic element scrolling. The plugin
skill should recommend references without warning agents that any observation invalidates them,
and should reserve explicit coordinates for DOM-opaque surfaces. The later agent-contract feature
may refine examples, but implementation of this feature owns removal of any guidance made false by
its runtime behavior.

**Acceptance criteria**:
- [ ] Foundation assertions match the corrected stable contract and preserve exclusions against
      permanent identity across document replacement.
- [ ] Skill guidance shows concise canonical key and default-target requests and tells agents to
      inspect the returned observation when they need to prove the UI effect.
- [ ] Generated `docs/public/llms-full.txt` is regenerated rather than hand-edited.

## Implementation Order

1. `epic-agent-browser-reliability-interaction-semantics-input-contracts` — request defaults,
   canonical chords, event construction, and secret-safe fill replacement.
2. `epic-agent-browser-reliability-interaction-semantics-reference-lifetime` — document-scoped
   reference registry and corrected foundation contract.
3. `epic-agent-browser-reliability-interaction-semantics-pointer-preparation` — shared
   resolve-scroll-re-resolve behavior using the corrected registry.
4. Update agent guidance, regenerate docs, run deterministic workspace gates, then run the opt-in
   macOS/Linux real-Chrome control qualification for password fill, Enter activation, retained
   references, and off-screen pointer behavior.

## Simplification

- Remove the hard-coded Control+A fill sequence and the separate char-event path; one key
  description/event builder owns keyboard semantics.
- Replace observation-scoped active snapshots with one latest document-scoped binding registry per
  live target; do not retain a history of whole snapshots.
- Replace pointer call-site viewport checks with one element preparation path. Keep declared
  coordinate hit testing separate because its no-scroll semantics are intentionally different.
- Do not add an operating-system abstraction, synthetic-event layer, or reference-cache eviction
  policy; the chosen browser-native/document-scoped design makes them unnecessary.

## Testing

- Core constructor/schema tests protect defaulted wire requests, canonical serialization, alias
  compatibility, duplicate/multi-key rejection, and the single operation registry.
- Scripted CDP regression tests protect exact modifier/key-down/key-up ordering, absence of char
  insertion under Meta/Control, password clearing without secret-bearing evaluation output, stable
  reference-to-backend mapping across observations, and resolve-scroll-re-resolve command order.
- Opt-in real-Chrome tests protect the two risks doubles cannot establish: native Enter/Space/form
  behavior and actual off-screen geometry after scrolling. Run on macOS and Linux because the
  original fill report was macOS-specific even though the new clear path is platform-neutral.
- Extend the browser-control benchmark with retained-reference and off-screen selector/reference
  cases. Avoid one assertion per key table row beyond the existing registry-completeness test.

## Risks

- **Riskiest assumption**: CDP key-down with `text` reproduces native Enter activation consistently
  across supported Chrome versions. The real-browser qualification is decisive; if it fails, keep
  the canonical chord contract and replace only the adapter key-description mapping.
- A controlled framework can restore a cleared value between Backspace and insertion. The zero-
  length check converts that race into an explicit failure; callers can then retry after inspecting
  page state instead of silently concatenating a secret.
- Backend node IDs are attachment-local. Document fingerprint and attachment generation remain
  mandatory fences, and a fresh resolve validates connectivity before every action.
- Scrolling can trigger DOM replacement or overlays. Re-resolution and the existing hit/actionable
  checks deliberately favor an explicit retry over dispatching stale geometry.

## Advisory review

This stable public-contract design warrants independent scrutiny, but the delegated design boundary
explicitly prohibits nested agents. Advisory review is deferred to the parent epic's required
fresh-context feature/aggregate review; this does not block the design stage transition.

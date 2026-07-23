---
id: feature-query-node-limit-large-pages
kind: feature
stage: review
tags: [bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-23
---

# Query and semantic-wait availability on large pages

## Brief

`query_page` (every query kind) and semantic `wait` fail hard on any document
whose accessibility tree exceeds the snapshot node bound, while `snapshot_page`
on the identical page succeeds with explicit omission accounting. Live repro on
v1.5.1 (https://en.wikipedia.org/wiki/Software_bug): plain role query →
"accessibility nodes: 6060 exceeds limit 5000"; the same query with
`container_text` → "7687 exceeds limit 5000" (container eligibility acquires
more of the tree); semantic wait → same failure. Large Wikipedia articles,
documentation sites, and dense dashboards are entirely outside the ergonomic
targeting path and the new full-tree semantic wait.

Mechanics (verified in source):

- `crates/krometrail-cdp/src/control/snapshot.rs:17` — `MAX_SNAPSHOT_NODES:
  usize = 5_000`, a bare constant with no derivation. The independent resource
  bound `MAX_SNAPSHOT_TEXT_BYTES = 1 MiB` (line 18) is what actually limits
  retained text memory.
- The snapshot builder (around line 2464) counts overflow into
  `omitted_node_count` and succeeds — snapshots tolerate omission by design.
- `active_for_query` (around line 642) refuses to answer whenever
  `omitted_node_count != 0`, which is what turns the builder cap into a hard
  query/wait failure. The error carries recovery text ("narrow the semantic
  query to a smaller document") but a descendant `scope` does not reduce
  acquisition, so the advertised recovery does not actually help today.
- Geometry decode has its own bounded fallbacks at the same constant
  (viewport-scoped selection, `geometry_omitted`), lines ~1156-1180, ~1456.

## Strategic decisions

- **Raise the node bound substantially rather than build partial-query
  machinery**: user direction ("shouldn't we just allow a lot more nodes —
  the node choice was arbitrary"). The 5,000 value is arbitrary; the memory
  story is governed by the text-byte cap. Design should pick a much larger
  node bound justified by measured per-node cost, keep a fail-closed refusal
  only for truly pathological trees, and keep snapshot omission semantics
  unchanged.
- Queries remain exact on complete snapshots: no silently-partial query
  results. If the (much larger) bound is still exceeded, the refusal stays
  explicit — but its recovery guidance must name actions that work.

## Simplification opportunity

The query-refusal path and its recovery text can likely be simplified once the
bound is realistic: if the raised bound covers real-world pages, the
"narrow the semantic query" recovery prose (which today names an action that
does not reduce acquisition) should be corrected or removed rather than
elaborated.

## Architectural choice

Raise the two acquisition bounds in `crates/krometrail-cdp/src/control/snapshot.rs`
and keep them as the single source of truth for acquisition. No partial-query
machinery, no per-subtree CDP capture, no silently-partial results. The bound
stays exactly the same shape it is today — one constant that governs AX-decode
omission accounting, the query/wait refusal, the DOM-decode guard, and the
viewport-scoped geometry selection cap — because all four are the *same*
acquisition concept and must trip at the same document complexity. Presentation
trimming (`concise`/`expanded`/`full` response `detail`) lives in
`krometrail-mcp` response projection and is already independent of this constant;
there is nothing to disentangle there.

**Chosen values.**

- `MAX_SNAPSHOT_NODES: 5_000 → 50_000`
- `MAX_SNAPSHOT_TEXT_BYTES: 1 << 20 (1 MiB) → 1 << 23 (8 MiB)`

**Why 50,000 nodes.** Per-node *non-text* retained cost (the part that scales
with node count; text heap is separately capped) is roughly:

| component | bytes/node |
|---|---|
| `SnapshotNode` backbone (ids, depth, four `String`/`Option<String>` control blocks, `properties` Vec header, `Option<NodeReference>`, `Option<CssRect>`) | ~208 |
| `parent_by_node` entry | ~24 |
| `node_by_backend` entry | ~24 |
| `semantic` (`SemanticNodeMetadata`) backbone | ~96 |
| `bindings` (actionable minority only) | amortized small |

≈ **~350 bytes/node** of structural overhead. At 50,000 nodes that is ~17.6 MB
of backbone, plus two capped text pools (AX names/values + DOM semantic/attribute
text) at 8 MiB each = 16 MiB, for a **worst-case ~34 MB retained active snapshot**
per target. Krometrail holds one active snapshot per target and few targets, so
34 MB at the pathological ceiling is an acceptable envelope; typical large pages
land far below it. Today's 5,000/1 MiB envelope is ~4 MB, so this is a deliberate,
bounded ~10× to buy full real-page coverage.

Real-world coverage: the live repro page reports 6,060 AX nodes / 7,687 DOM
backend nodes — note the DOM count exceeds the AX count and is what the
`container_text` query tripped on, so the raised bound must cover the DOM count,
not just the AX count. Heavy Wikipedia articles run ~8k, and dense
dashboards/documentation commonly reach 10k–50k DOM nodes. 50,000 sits at the top
of that band: it clears essentially every real interactive document while still
fail-closing on genuinely pathological trees (100k+ nodes) where exact query is
both memory-unbounded and slow.

**Why not higher (decode cost).** Semantic `wait` re-captures the full snapshot
every poll via `probe_semantic_presence` (`snapshot.rs:403`), and the wait loop
(`wait.rs:239-260`) *awaits* each capture before sleeping `poll_interval` (≥100ms).
`getFullAXTree` + `captureSnapshot` for 50k nodes is tens of MB of transient JSON
and O(n) decode — tens to low-hundreds of ms. Because capture is awaited
serially per poll, cost is self-throttling: the wait cadence simply becomes
capture-bound on huge pages, it does not queue or stack. At 100k+ the per-poll
capture would rival/exceed the poll interval and the transient payload doubles,
so 50k is the ceiling that keeps the interactive path healthy. **No separate
decode guard is added** — the node bound *is* the guard, and the await-per-poll
design already serializes the work.

**Why the text cap must scale too (the other-bound interaction).** If the node
bound rose to 50k while `MAX_SNAPSHOT_TEXT_BYTES` stayed at 1 MiB, ordinary large
pages would newly trip the *text* cap and reintroduce the same failure through the
other bound:

- On the AX path (`snapshot.rs:2464-2465`), exceeding text flips
  `omitted_node_count != 0`, which `active_for_query` (`:642`) turns straight back
  into the query/wait refusal — the exact bug, re-entered via text.
- On the DOM path it is worse: `:1390` and `:1515` return a **hard `malformed`
  error** ("DOM snapshot exceeds the semantic text limit") when text overflows, so
  a merely-large page would fail with a *mislabeled* malformed error.

Real accessible-text density is low (most nodes have short or empty names; the
repro article carries well under 1 MiB of AX name text and visible text), so 1 MiB
was rarely the AX limiter. But a 50k-node dashboard with dense labels can approach
or exceed 1 MiB. 8 MiB gives ~8× headroom over the largest observed real pages
while still fail-closing on pathological text, and stays within the 34 MB envelope.
It slightly under-scales the strict node ratio (proportional would be ~10 MiB) but
real per-node text is far below the 210 bytes/node the old ratio implied, so the
headroom is ample. Keeping AX and DOM on the same `MAX_SNAPSHOT_TEXT_BYTES` keeps
the two pools consistent.

**Refusal recovery honesty.** At the raised-but-still-exceeded bound the refusal
stays explicit, but its recovery must name actions that actually reduce
acquisition or otherwise work. Investigation of the two runtime call sites:

- `active_for_query` (`:646`, AX-count refusal) and the DOM-decode guard (`:1178`,
  DOM-count refusal) are **both semantic-query/wait captures**. The DOM guard's
  final `return` is reached only when `viewport_scope` is `None` *and*
  `include_document_geometry` is `false` — i.e. never a geometry request — yet it
  currently passes `query_exists = false`, so a query refusal today emits the
  *geometry* recovery ("request a smaller document snapshot or use viewport-scoped
  geometry"). That is the mislabeled path the `container_text` repro hit.
- The descendant `scope` (`QueryPageRequest.scope: Option<NodeReference>`) is a
  **post-acquisition filter** in `query()` (`:659-667`); it never reduces what is
  captured. So "narrow the semantic query" is not a working recovery and must go.
- What *does* reduce acquisition: `QueryPageRequest.document:
  SemanticDocumentScope::Frame(...)` — a frame-scoped capture pulls only that
  frame's AX/DOM tree. And `snapshot_page` succeeds on the same document with
  explicit omission accounting, returning the bounded node list *with actionable
  `NodeReference` handles*, which the agent can act on directly.

Because geometry requests never error on node count (they degrade gracefully to
`geometry_omitted` — see `:1171-1176` and the `geometry_over_cap` test), the
`query_exists = false` / geometry-recovery branch of `snapshot_node_limit_error`
has **no live caller** once `:1178` is corrected. So the parameter and its dead
branch are removed and the recovery text is rewritten once to name the working
actions (frame `document` scope + `snapshot_page` reference targeting).

**Geometry paths.** The viewport-scoped geometry fallback (`:1156-1170`,
`:1456-1459`) and the `geometry_omitted` degradation (`:1171-1176`) key off the
same `MAX_SNAPSHOT_NODES`. Raising it does not regress their intent: viewport
selection still caps the returned rect set (now at 50k in-viewport nodes, a set
that is almost always far smaller than the viewport subset anyway), and
`snapshot_page` still succeeds with `geometry_omitted = true` on oversized DOMs.
These paths need **no code change** beyond inheriting the new constant.

**Probe inheritance.** `probe_presence` (`:745`) routes through the same
`active_for_query` gate as `query()`, so the semantic-wait probe inherits the
raised bound and the corrected refusal automatically — no separate change.

## Implementation Units

### Unit A — Raise and derive the two acquisition bounds

- **File:** `crates/krometrail-cdp/src/control/snapshot.rs:17-18`
- **Change:**
  - `const MAX_SNAPSHOT_NODES: usize = 50_000;`
  - `const MAX_SNAPSHOT_TEXT_BYTES: usize = 1 << 23;` (8 MiB)
  - Replace the bare constants with a short derivation comment: node bound sized
    to cover real dense documents (dashboards/docs 10k–50k DOM nodes; repro 7,687
    DOM nodes) against a ~350 bytes/node structural cost and a ~34 MB worst-case
    retained-snapshot envelope; text bound scaled to 8 MiB so large-but-ordinary
    pages do not re-trip the refusal via the text path, while both still
    fail-close on pathological trees.
- **Notes:** These remain the single source of truth. Do not introduce a second
  constant; every existing `MAX_SNAPSHOT_NODES` / `MAX_SNAPSHOT_TEXT_BYTES` use
  (AX overflow `:2464-2465`, DOM guard `:1156`, DOM text `:1390`/`:1515`,
  viewport cap `:1456`, error report `:1097`) inherits the new value.
- **Acceptance:**
  - A document with 6k–8k AX/DOM nodes and modest text decodes with
    `omitted_node_count == 0`, so `query_page` and semantic `wait` succeed instead
    of refusing.
  - A document ≤50k nodes with up to ~8 MiB of accessible/semantic text decodes
    fully (no omission, no `malformed` text error).
  - Worst-case retained-snapshot reasoning recorded in the derivation comment.

### Unit B — Honest, unified refusal recovery

- **File:** `crates/krometrail-cdp/src/control/snapshot.rs`
- **Changes:**
  - `snapshot_node_limit_error` (`:1083-1106`): drop the `query_exists: bool`
    parameter; new signature
    `fn snapshot_node_limit_error(target_id: TargetId, actual: impl std::fmt::Display) -> KrometrailError`.
    Emit one recovery string that names working actions, e.g.:
    *"target a single frame with the `document` scope, or capture the page with
    `snapshot_page` (which reports omitted nodes explicitly) and act on the
    returned node references directly."* Keep `RetryAdvice::Never` and the
    `"accessibility nodes"` / `MAX_SNAPSHOT_NODES` limit reporting.
  - Caller `:646` (`active_for_query`): drop the trailing `true` argument.
  - Caller `:1178` (DOM-decode guard, semantic path): drop the trailing `false`
    argument. This is the fix that stops the DOM-count refusal from emitting the
    geometry recovery text.
- **Notes:** Removing the parameter deletes the now-dead geometry-recovery branch
  (no live caller after `:1178` is corrected — geometry requests degrade to
  `geometry_omitted` rather than erroring). This is the simplification the item's
  "Simplification opportunity" calls for. Do not add descendant-`scope` guidance;
  `scope` is a post-acquisition filter and does not reduce acquisition.
- **Acceptance:**
  - Both refusal call sites produce identical, honest recovery text.
  - Recovery text does **not** contain "narrow the semantic query"; it names
    frame `document` scoping and `snapshot_page` reference targeting.
  - No compiler warning for an unused branch/param; `snapshot_node_limit_error`
    has a single recovery path.

### Unit C — Tests: rebase pins and add regressions

- **File:** `crates/krometrail-cdp/src/control/snapshot.rs` (test module)
- **Rebase:**
  - `node_limit_errors_name_actual_limit_and_scope_specific_recovery`
    (`:3413-3436`): update the literal `5_000`/`5_001` expectations to the new
    bound; replace the `"semantic query"` recovery assertion with the corrected
    wording; delete the `query_exists = false` / `"smaller document snapshot"`
    geometry-error sub-case (that branch no longer exists). Rename to reflect a
    single unified recovery.
  - Tests expressed against the `MAX_SNAPSHOT_NODES` symbol
    (`multi_document_snapshot_with_large_parent` `:2964`, `large_viewport_dom_snapshot`
    `:2992`, and the viewport/geometry tests at `:3859`/`:3870`) auto-rebase; just
    re-verify they still pass (their fixtures now build ~50k-element vectors —
    heavier but fine for unit tests).
- **New regressions:**
  - **Node-band success:** a snapshot whose node count sits between the old and
    new bound (e.g. ~8,000, Wikipedia-scale) decodes with `omitted_node_count == 0`
    and a `query_page` / `probe_presence` on it returns a real outcome instead of a
    `PageObservationFailed` limit error. This is the core proof of the fix.
  - **Text-cap interaction:** a page under the node bound but carrying large text
    (a) under 8 MiB decodes fully (no omission, no `malformed`), and (b) a
    pathological >8 MiB text page still fails closed — pinning that the text cap,
    not the node cap, is the memory governor at the raised bound.
  - **Refusal wording:** a genuinely over-bound document (>50k) refuses with the
    unified recovery text that names `snapshot_page` / frame `document` scope and
    omits any "narrow the query" phrasing, for both the AX-count and DOM-count
    refusal paths.

## Implementation Order

1. **Unit A** (raise + derive constants) — everything else observes these values.
2. **Unit B** (unify + correct recovery) — depends on A only for the fixture
   sizing in shared test helpers, but is logically independent; do after A.
3. **Unit C** (rebase + regressions) — last, so the new bound and recovery are in
   place to assert against.

## Simplification

- Deletes the `query_exists` parameter and the dead geometry-recovery branch of
  `snapshot_node_limit_error`, collapsing it to one honest recovery path (net code
  reduction, per the item's simplification opportunity).
- No new constant, no new type, no new config surface: the fix is two constant
  values, one signature reduction, and two argument-list trims.
- Response-projection bounds are untouched — acquisition and presentation stay
  cleanly separated.

## Testing

- **Wire contracts:** no wire enum or schema mentions the node limit
  (`scripts/check-wire-enum-schemas.sh` covers enum schemas; the only `5_000`
  literal elsewhere is `poll_interval`'s unrelated `maximum` in
  `krometrail-mcp/src/schema.rs`). Constants and recovery prose are runtime-only,
  so `check-wire-enum-schemas.sh` stays green. The MCP/wire request shape is
  unchanged.
- **Gates:** `cargo fmt --all -- --check`, `bash scripts/check-wire-enum-schemas.sh`,
  `cargo check/test/clippy --workspace --all-targets --locked`.
- Rebased pins and new regressions per Unit C. The larger fixture vectors keep
  unit-test runtime negligible.

## Risks (pre-mortem)

- **Retained-memory ceiling.** Worst-case ~34 MB per active snapshot on a
  pathological 50k-node / 8 MiB-text page, vs ~4 MB today. This is bounded and
  per-target with few targets, and only the ceiling — but it is a real ~10×
  increase in the worst case. *Host attention:* confirm the ~34 MB envelope is
  acceptable for the deployment; if not, dial the node bound to 25k (still clears
  the repro and Wikipedia, trims the ceiling to ~18 MB).
- **Decode latency on the wait path.** A 50k-node semantic `wait` makes each poll
  capture-bound (tens–low-hundreds of ms). It is self-throttling (awaited serially,
  no queueing) and stays within the ≥100ms cadence intent, but a very slow browser
  could make waits feel sluggish on huge pages. No guard added by design; flag if
  interactive latency regresses in real use.
- **DOM text over-cap is still a hard `malformed` error** (`:1390`/`:1515`), an
  asymmetry with the AX path's graceful omission and a mildly wrong diagnostic
  label. Raising the cap to 8 MiB makes it effectively unreachable for real pages,
  so this is left as-is per code economy — noted as a latent inconsistency, not
  fixed here.
- **Transient acquisition payload.** `getFullAXTree` + `captureSnapshot` transient
  JSON at 50k nodes is tens of MB per capture (freed after decode). Bounded, but
  worth watching under memory pressure alongside the retained figure.

## Implementation notes

- Execution capability: inline implementation; the feature had one cohesive Rust
  ownership surface and the requested tests live beside the decoder.
- Review weight: standard default; feature-stage transition/review was not run
  because the caller explicitly required leaving the stage unchanged.
- Files changed: `crates/krometrail-core/src/browser/observation.rs`,
  `crates/krometrail-cdp/src/control/snapshot.rs`, and this feature item.
- Tests added: an 8,000-node AX query/presence regression; an 8,000-node DOM
  container-query/presence regression through the real decoder; AX and DOM
  text-cap boundary regressions; unified AX/DOM refusal-recovery wording
  coverage; and a 20,000-node deep-chain decode/query tripwire.
  The viewport truncation fixture was rebased to cover the raised symbol-sized
  bound.
- Simplification: removed `query_exists` and the dead geometry recovery branch
  from `snapshot_node_limit_error`; both refusal callers now share one honest
  recovery path.
- Discrepancies from design: none in behavior or selected values. The existing
  viewport regression needed its fixed viewport width raised from 10,000px so it
  continued to select more than the new 50,000-node cap.
- Correction: the earlier near-34 MB retained-snapshot envelope understated
  duplicated per-ancestor rendered and label text; the pathological envelope is
  now documented as roughly 17.6 MB backbone + 8 MiB global text + up to ~50 MiB
  duplicated rendered/label text, with realistic pages far below it.
- Gate results: `cargo fmt --all -- --check`,
  `bash scripts/check-wire-enum-schemas.sh`,
  `cargo check --workspace --all-targets --locked`,
  `cargo test --workspace --all-targets --locked`, and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` all passed.
- Adjacent issues parked: none.

## Review fixes

- Finding 1: `PageSnapshot::new` now records each validated node depth in a
  `HashMap`, so parent-depth validation remains a single O(n) pass while
  preserving all existing validation errors and messages.
- Finding 2: normal DOM semantic decode now records own layout text once and
  aggregates child summaries bottom-up in document order, preserving the
  1,024-byte retained text and true collapsed-length accounting; nearest label
  ancestors are precomputed in one forward pass.
- Finding 3: query and semantic-presence evaluation now builds one O(1)
  `SnapshotNodeId` lookup at the evaluation boundary for container-text
  ancestor resolution, without changing matching semantics.
- Finding 4: corrected the acquisition-bound comment to document the honest
  pathological retained-memory envelope; no new cap was introduced.
- Finding 5: unified recovery text now distinguishes frame-scoped `document`
  guidance for queries from frame-scoped `query_page` polling for waits, while
  retaining `snapshot_page` reference targeting as the explicit-omission path;
  the wording regression asserts the wait guidance.
- Finding 6: added an end-to-end ~8,000-node DOM fixture covering both a
  `container_text` query and semantic presence probe through DOM decoding, plus
  a 20,000-node deep-chain decode/query tripwire for the linear paths.

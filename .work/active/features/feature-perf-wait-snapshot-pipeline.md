---
id: feature-perf-wait-snapshot-pipeline
kind: feature
stage: review
tags: [perf]
parent: null
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Semantic-wait and snapshot query pipeline performance

## Brief

Profiling the snapshot acquisition/decode/query path (release build, real
production functions, synthetic AX/DOM payloads at 5k/20k/50k nodes) shows
the per-poll cost of semantic waits exceeds the ≥100 ms poll cadence at the
new 50,000-node bound: a single poll costs 111–188 ms of pure CPU, so the
wait loop saturates a core polling back-to-back. Real cost is strictly
higher (CDP round-trip and Chrome-side tree generation excluded; synthetic
payloads are leaner than real Chrome responses).

Measured evidence (ms; 5k / 20k / 50k nodes):

- Parse AX payload (0.73/2.95/7.44 MB) to `serde_json::Value`:
  3.88 / 17.05 / 65.71 — superlinear (10× nodes → 17× time).
- `decode_ax_tree_with_ids`: 2.81 / 13.47 / 68.39 — superlinear (~24×);
  allocator/cache pressure from ~50k small `Value` allocations, string-keyed
  `by_id`/`children`/`visited` maps, per-node `to_owned` of
  role/name/value/description.
- Per-poll simulated wait (capture→decode→probe per iteration): role+name
  wait 7.2 / 33.1 / 111.4 incl. parse; text wait (AX+DOM) 16.2 / 77.4 /
  188.5. The registry rebuild itself is NOT the cost (`begin_snapshot`
  node_by_backend clone: 0.02 ms — refuted suspect).
- `probe_presence` text-miss (the wait's steady state): 55.19 at 50k vs
  10.75 for the equivalent query and 0.93 scan floor. Root:
  `SemanticTextMatch::matches` → `normalize_semantic_text`
  (observation.rs:609/634) allocates a fresh String per candidate node
  (rendered_text up to 1 KB) AND re-normalizes the needle per node; probes
  evaluate ALL nodes (queries filter to actionable first); on no-match the
  relaxed-candidate rescan (snapshot.rs:789) repeats the entire scan —
  ~100k full-text normalizations (~50 MB transient allocation) per poll.
  Rendered text is already whitespace-collapsed at decode time by
  `append_semantic_text`.
- `container_text` (contains) query: 16.20 at 50k vs 0.93 floor (17×).
  Root: `nearest_container_text_matches` (snapshot.rs:1015) re-walks and
  re-normalizes shared ancestors once per candidate
  (`true_collapsed_text_bytes()` at observation.rs:665 allocates via
  `normalize_semantic_text`) with no per-query memoization.
- Not bottlenecks (measured): `PageSnapshot::new` 2.16 ms at 50k (linear);
  DOM decode 13.21 ms (linear); per-query `nodes_by_id` build within the
  0.93 ms floor.

Proposed hierarchy levels: level 2 + level 1 (per-poll refetch/re-decode of
a multi-MB payload with zero reuse — hash the raw AX payload (or nodes
array) and reuse the previously decoded snapshot when unchanged, since a
polling wait exists precisely because the page usually has not changed;
expected 10–30× per-poll on quiescent pages), level 4/3 (typed `Deserialize`
structs for the AX tree instead of `Value` traversal with integer-indexed
lookup — expected 2–4× decode unconditionally), level 1 + 4 (store
pre-normalized rendered text at decode, normalize the needle once per
probe, allocation-free `matches`, fuse the relaxed rescan into the primary
pass for probes — probe 55 ms → low single-digit), level 1 (per-query memo
of ancestor container verdicts — ~5–8× on container queries). Probe
families: on-CPU + memory allocation.

Boundary note for design: the `Value` parse cost lives at the cdpkit
transport boundary (`crates/krometrail-cdp/src/transport/cdpkit.rs`,
`EventStream<serde_json::Value>`); fixing parse (as opposed to decode)
means touching the transport seam — the design must weigh a
snapshot-payload-specific typed path against the generic transport contract.
The unchanged-payload short-circuit must be correctness-safe: byte-identical
payload implies identical decoded snapshot ONLY if decode is deterministic
and generation/identity semantics are preserved — the design must keep
reference/generation invariants (same generation reuse is already the
registry's model for unchanged documents) and must not suppress
new-generation transitions after real changes.

## Perf Overview

The semantic-wait steady state re-fetches, re-parses, re-decodes and
re-scans a multi-megabyte accessibility payload every poll even though a
polling wait exists precisely because the page has usually not changed. At
the 50k-node bound a single poll costs 111–188 ms of CPU against a ≥100 ms
minimum semantic poll cadence (`MIN_SEMANTIC_WAIT_POLL_INTERVAL`, enforced in
`crates/krometrail-core/src/browser/wait.rs:311`), so the loop saturates a
core.

Four levers, in descending confidence and ascending coupling to the
transport seam:

1. **Probe normalization + relaxed-rescan fusion** (level 1 + 4, transport-independent).
   The steady state of a wait is a *no-match* probe. `probe_presence`
   scans every node, allocating a freshly normalized `String` per candidate
   AND re-normalizing the needle per candidate, then on the (normal) empty
   result runs the *entire* scan a second time for the relaxed-candidate
   count — ~100k full-text normalizations / poll. Pre-normalize the match key
   once at decode, normalize the needle once per probe, make the hot compare
   allocation-free, and fold the relaxed count into the primary pass. 55 ms →
   low single digits. Highest-value, ships alone.

2. **container_text ancestor memoization** (level 1, transport-independent).
   `nearest_container_text_matches` re-walks and re-normalizes shared
   ancestors once per candidate with no memo. Memoize the per-ancestor
   container verdict for the single query evaluation. 16 ms → ~2–3 ms.

3. **Typed AX decode** (level 4/3, transport-independent).
   Replace the manual `serde_json::Value`-traversal decoder (repeated
   string-keyed `.get()` probes, `HashMap<&str, &Value>` index, per-node
   `to_owned` of role/name/value/description) with serde-derived structs
   consumed from the **owned** response `Value` via `serde_json::from_value`
   (moving Strings, not cloning) and integer-indexed tree assembly. Decode
   68 ms → ~20 ms on the hash-miss path.

4. **Unchanged-payload short-circuit** (level 1 + 2, transport-seam gated).
   Fingerprint the raw AX (and DOM) response and, when the attachment
   generation, document fingerprint, and payload fingerprint are all
   unchanged, reuse the previously decoded `ActiveSnapshot` under the same
   generation instead of decoding and re-installing. This is the only lever
   that can reach the item's ~1–5 ms quiescent-poll target, and it depends on
   a transport-seam decision (see the boundary finding below).

**Transport-seam finding that shapes decisions 1 and 2 of the design brief.**
The production transport is exact cdpkit 0.4.0
(`crates/krometrail-cdp/src/transport/cdpkit.rs`). Its message loop
(`~/.cargo/.../cdpkit-0.4.0/src/inner.rs:230-233`) *unconditionally* parses
every inbound WebSocket text frame with `serde_json::from_str::<Value>` to
route it by `id`, stores the routed `Value` in a `oneshot`, and discards the
original bytes. `send_raw` returns that already-built `Value`; the typed
`send_cmd` path calls `serde_json::from_value::<T>` *on top* of it. Two
consequences:

- **Parse-to-`Value` (65.71 ms @ 50k) is intrinsic to cdpkit 0.4.0 and cannot
  be reclaimed from Krometrail's side.** A "typed `send_raw` sibling" routed
  through cdpkit's typed API would add a `from_value` pass over the
  already-materialized `Value` — strictly *worse* for parse. A true
  bytes→typed fast path (or a pre-parse byte fingerprint) requires cdpkit to
  peek only the routing envelope and hand the receiver raw bytes, which
  0.4.0 does not do. Per `docs/ARCHITECTURE.md`, changing that is a binding
  cdpkit limitation requiring a new transport decision.
- Therefore typed decode (decision 2) is scoped to the **decode** stage and
  consumes the owned `Value` in place; and the short-circuit (decision 1) is
  designed around a **raw-byte** fingerprint at the transport return, with its
  headline win explicitly gated on the transport decision. Hashing the parsed
  `nodes` sub-`Value` instead is deliberately rejected: once probe (opt 1) and
  typed decode (opt 3) land, a structural walk of the parsed nodes costs
  roughly as much as the typed decode it would elide, so it buys almost
  nothing while still paying parse.

## Profiling Summary

Measured in the item Brief (release build, real production functions,
synthetic AX/DOM payloads; ms at 50k nodes unless noted):

| Stage | 5k | 20k | 50k | Scaling | Root cause |
|---|---|---|---|---|---|
| AX parse → `Value` | 3.88 | 17.05 | 65.71 | ~17× | cdpkit read-loop `from_str::<Value>`; intrinsic |
| `decode_ax_tree_with_ids` | 2.81 | 13.47 | 68.39 | ~24× | ~50k small `Value` allocs, `HashMap<&str>` index, per-node `to_owned` |
| role+name poll (AX) | 7.2 | 33.1 | 111.4 | — | parse + decode |
| text poll (AX+DOM) | 16.2 | 77.4 | 188.5 | — | parse + decode + probe |
| `probe_presence` text-miss | — | — | 55.19 | — | per-node normalize alloc + per-node needle re-normalize + full relaxed rescan (~100k normalizations, ~50 MB transient) |
| `container_text` contains | — | — | 16.20 | 17× floor | ancestor re-walk + re-normalize per candidate, no memo |

Refuted suspects (measured, treated as **non-goals**): `begin_snapshot`
node_by_backend clone 0.02 ms; `PageSnapshot::new` 2.16 ms (linear); DOM
decode 13.21 ms (linear); per-query `nodes_by_id` build inside the 0.93 ms
scan floor. No registry-rebuild or `PageSnapshot::new` work is in scope.

## Optimization Plan

### Optimization 1: Probe normalization + relaxed-rescan fusion
**Hierarchy Level**: Algorithmic / Data Model (redundant work eliminated) + Runtime Idiom (allocation removed from hot loop)
**Probe Family**: On-CPU + memory allocation
**Bottleneck**: `probe_presence` (`snapshot.rs:754`) and `SemanticTextMatch::matches` (`observation.rs:609`). The wait's steady state is a no-match text probe. `matches()` calls `normalize_semantic_text` on the candidate (fresh `String`, rendered text up to 1 KB) *and* on the needle, once per node; probes evaluate all nodes; on the empty result the relaxed-candidate rescan (`snapshot.rs:789`) repeats the whole scan. ~100k normalizations and ~50 MB transient allocation per poll at 50k.
**Expected Metric Movement**: probe text-miss 55.19 ms → low single-digit ms; per-poll allocations from ~100k `String`s → O(1) per probe (one normalized needle + one reused fold scratch). Removes the second full scan.
**Story**: `feature-perf-wait-snapshot-pipeline-opt-1`

#### Implementation Units

##### Unit 1.1: Precomputed, case-preserving match key on decode
**File**: `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
#[derive(Clone, Debug, Default)]
struct SemanticNodeMetadata {
    labels: Vec<String>,
    rendered_text: String,          // unchanged: raw bounded form; feeds true_collapsed_text_bytes and stays internal
    normalized_match_text: String,  // NEW: normalize_semantic_text(rendered_text, /*case_sensitive=*/true)
    collapsed_text_bytes: usize,
    test_id: Option<String>,
}
```

**Implementation Notes**:
- Populate `normalized_match_text` wherever `SemanticNodeMetadata` is built
  (the two construction sites at `snapshot.rs:~1316` and `~1579`, plus label
  paths) by normalizing the *already-bounded* `rendered_text`. Bound-then-
  normalize reproduces exactly what `matches()` computes today
  (`normalize(rendered)`), so match semantics are byte-identical.
- `SemanticNodeMetadata` is adapter-internal (never surfaced to agents;
  confirmed — only matching and `true_collapsed_text_bytes` read it), so
  adding the field changes no external contract. `rendered_text` is retained
  unchanged so length accounting and the `true_collapsed_text_bytes` fallback
  are untouched.

**Acceptance Criteria**:
- [ ] All existing `snapshot.rs` / `observation.rs` match, container, and
      normalization tests pass unchanged.
- [ ] `normalized_match_text == normalize_semantic_text(rendered_text, true)`
      asserted in a unit test for zwsp / private-use / mixed-whitespace inputs.

##### Unit 1.2: Allocation-free pre-normalized matcher
**File**: `crates/krometrail-core/src/browser/observation.rs`

```rust
/// Needle normalized once; matches against candidates already normalized
/// (case-preserving) at decode. Semantics identical to SemanticTextMatch::matches.
pub struct NormalizedTextNeedle {
    needle: String,          // normalize_semantic_text(value, case_sensitive)
    mode: SemanticTextMatchMode,
    case_sensitive: bool,
}

impl SemanticTextMatch {
    pub fn normalized_needle(&self) -> NormalizedTextNeedle { /* normalize once */ }
}

impl NormalizedTextNeedle {
    /// `candidate` is normalize_semantic_text(_, true) (case-preserving).
    /// `scratch` is a reusable buffer for the case-insensitive Contains fold;
    /// Exact and case-sensitive paths never touch it.
    pub fn matches_prenormalized(&self, candidate: &str, scratch: &mut String) -> bool;
}
```

**Implementation Notes**:
- `case_sensitive`: direct `==` (Exact) / `contains` (Contains) — allocation-free.
- `!case_sensitive`, Exact: compare `candidate.chars().flat_map(char::to_lowercase)`
  against `self.needle.chars()` — allocation-free. Correct because case folding
  commutes with the other normalization steps (whitespace-collapse,
  invisible-strip, private-use-as-space are all case-independent), so
  `lowercase(normalize(x,true)) == normalize(x,false)`.
- `!case_sensitive`, Contains: `scratch.clear(); scratch.extend(candidate.chars().flat_map(char::to_lowercase)); scratch.contains(&self.needle)` — one reused allocation per probe, amortized to zero after warmup.
- Keep `SemanticTextMatch::matches` as the reference implementation used by
  tests; add a debug-assert-backed equivalence test between `matches` and
  `normalized_needle().matches_prenormalized(normalize(candidate,true), _)`.

**Acceptance Criteria**:
- [ ] Property/equivalence test: for a corpus of candidates and needles across
      both modes and both case flags, `matches` and `matches_prenormalized`
      agree on every pair.
- [ ] No allocation on the Exact / case-sensitive hot paths (verified by a
      benchmark allocation counter or code inspection).

##### Unit 1.3: Fuse the relaxed rescan into the primary probe pass
**File**: `crates/krometrail-cdp/src/control/snapshot.rs` (`probe_presence`, and the parallel `query` no-match diagnostics)

```rust
// One pass: normalize needle (+ relaxed needle) once, one reused scratch.
// Per node: evaluate the primary predicate; when the query is Exact and the
// candidate is a Contains hit, also count it toward the capped relaxed total.
// Surface relaxed_match_candidates only when match_count == 0 (unchanged).
```

**Implementation Notes**:
- Exact ⊂ Contains for text, so evaluate Contains once and derive Exact via a
  cheap `==` when Contains hits; accumulate `match_count` (primary mode) and a
  capped `relaxed_count` in the same iteration.
- Preserve `MAX_SEMANTIC_RELAXED_CANDIDATES` cap and the "only when
  `match_count == 0`" surfacing gate; the second full scan is removed.
- Thread `NormalizedTextNeedle` + a `&mut String` scratch through
  `semantic_query_matches` so Text/Role-name/Label matching all use the
  pre-normalized path.

**Acceptance Criteria**:
- [ ] `probe_presence` performs exactly one traversal of `snapshot.nodes` on
      the no-match path (asserted via a counting test double or benchmark).
- [ ] Outcome, `match_count`, and `relaxed_match_candidates` unchanged for the
      existing probe/query tests.

---

### Optimization 2: container_text ancestor memoization
**Hierarchy Level**: Algorithmic / Data Model
**Probe Family**: On-CPU
**Bottleneck**: `nearest_container_text_matches` (`snapshot.rs:1015`) walks ancestors per candidate and calls `true_collapsed_text_bytes()` / `expected.matches(rendered_text)` on shared ancestors repeatedly. 16.20 ms vs 0.93 ms floor.
**Expected Metric Movement**: container `contains` query 16.20 ms → ~2–3 ms.
**Story**: `feature-perf-wait-snapshot-pipeline-opt-2`

#### Implementation Units

##### Unit 2.1: Per-query memo of the ancestor container verdict
**File**: `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
// Allocated once per query()/probe() call; keyed by ancestor SnapshotNodeId.
// The verdict "does the walk starting at this ancestor (inclusive) yield a
// container-text match" depends only on the ancestor chain and the fixed
// per-call needle, so it is identical for every descendant candidate.
fn ancestor_container_verdict(
    ancestor: SnapshotNodeId,
    needle: &NormalizedTextNeedle,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic: &HashMap<SnapshotNodeId, SemanticNodeMetadata>,
    nodes_by_id: &HashMap<SnapshotNodeId, &SnapshotNode>,
    memo: &mut HashMap<SnapshotNodeId, bool>,
    scratch: &mut String,
) -> bool;
```

**Implementation Notes**:
- `nearest_container_text_matches(node, ...)` becomes: resolve `node`'s parent,
  then return `ancestor_container_verdict(parent, ...)`. The verdict function
  encodes the existing rules — nearest LOCAL container is the sole authority
  (stop, return its match); a bounded (`MAX_GENERIC_CONTAINER_TEXT_BYTES`)
  matching GENERIC container qualifies (stop, true); otherwise recurse upward —
  memoizing each ancestor's result.
- **Scope / lifetime**: the memo (and the fold scratch from opt 1) are created
  at the top of `query`/`probe_presence`, one per query evaluation, keyed by
  ancestor `SnapshotNodeId`; they die with the call. No cross-poll caching,
  so no invalidation surface — the `ActiveSnapshot` may differ every poll.
- Uses `normalized_match_text` (opt 1) for the per-ancestor compare, so each
  distinct ancestor is normalized at most once regardless of fan-out.

**Acceptance Criteria**:
- [ ] Each ancestor's rendered text is matched at most once per query
      evaluation (counting test or instrumentation).
- [ ] Existing `nearest_container` / generic-container-bound / local-container
      authority tests pass unchanged.

---

### Optimization 3: Typed AX decode
**Hierarchy Level**: Runtime Idiom (level 4) with Data-Locality benefit (level 3)
**Probe Family**: On-CPU + memory allocation
**Bottleneck**: `decode_ax_tree_with_ids` (`snapshot.rs:2399`) and `Decoder::visit`. Manual `Value` traversal: `HashMap<&str,&Value>` by_id, `HashSet<&str>` children, per-node repeated `.get("role"/"name"/...)` map probes, per-node `to_owned` of four strings. 68.39 ms @ 50k, superlinear.
**Why higher levels don't apply**: the traversal shape (preorder over the AX tree, actionability + node-id assignment, caps) is already correct and irreducible; the cost is per-node runtime overhead (map probes, clones, small allocations), not a wrong complexity class or an I/O round-trip. Parse-to-`Value` (a higher-level I/O/serialization concern) is intrinsic to cdpkit and out of reach here — see the transport finding.
**Expected Metric Movement**: decode 68 ms → ~20 ms (2–4×) on the hash-miss path; per-node `to_owned` clones eliminated (Strings moved out of the owned `Value`); string-keyed field probes replaced by positional struct fields and integer-indexed child lookup.
**Story**: `feature-perf-wait-snapshot-pipeline-opt-3`

#### Implementation Units

##### Unit 3.1: serde structs for the getFullAXTree result, consumed from the owned Value
**File**: `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
#[derive(Deserialize)]
struct AxTreeResponse { #[serde(alias = "result")] result: Option<AxResultInner>, nodes: Option<Vec<AxNodeWire>> }
#[derive(Deserialize)]
struct AxNodeWire {
    node_id: String,                    // "nodeId"
    #[serde(default)] frame_id: Option<String>,
    #[serde(default)] ignored: bool,
    #[serde(default)] role: Option<AxValueWire>,
    #[serde(default)] name: Option<AxValueWire>,
    #[serde(default)] value: Option<AxValueWire>,
    #[serde(default)] description: Option<AxValueWire>,
    #[serde(default, rename = "backendDOMNodeId")] backend_dom_node_id: Option<i64>,
    #[serde(default)] child_ids: Vec<String>,   // "childIds"
    #[serde(default)] properties: Vec<AxPropertyWire>,
}
// AxValueWire { value: Value } ; AxPropertyWire { name: String, value: AxValueWire }

fn decode_ax_tree_with_ids(response: serde_json::Value, /* now owned */ ...) -> Result<...>;
```

**Implementation Notes**:
- Change `capture_snapshot_for_frame` to pass the **owned** `ax_response`
  `Value` into decode (it already owns it from `send_raw`); `from_value`
  moves the `String` fields out instead of cloning. Do **not** add a typed
  transport method — the generic `send_raw`/`CdpTransport` contract is
  unchanged; parse still happens once in cdpkit's read loop and is not
  double-paid.
- Build `by_index` maps keyed by `nodeId` -> array index once, resolve
  `childIds` through indices, run the same preorder `visit` with identical
  ignored/`none`/`presentation` skip, actionability, `MAX_SNAPSHOT_NODES` /
  `MAX_SNAPSHOT_TEXT_BYTES` caps, `backendDOMNodeId` → `SnapshotNodeId`
  assignment, and `seen_backends` retain logic. Behavior is preserved;
  only representation changes.
- Respect `validated-wire-contracts`: the wire structs are permissive shapes;
  all domain validation (node-id space, caps, actionability, frame filtering,
  the "different document" stale check) stays in the decode/domain layer, not
  in serde.
- Keep the `#[cfg(test)] decode_ax_tree` helper working against the new
  signature (own the `Value` in the helper). DOM decode is **not** typed
  (measured linear, 13 ms, not a bottleneck).

**Acceptance Criteria**:
- [ ] Decode benchmark shows ≥2× at 50k vs the recorded 68.39 ms baseline.
- [ ] Every existing AX-decode test (frame filtering, cross-origin rejection,
      caps/omission, actionability, backend-id stability, structural web area)
      passes unchanged.

---

### Optimization 4: Unchanged-payload short-circuit (transport-seam gated)
**Hierarchy Level**: Algorithmic / Data Model (eliminate redundant re-decode) + I/O / serialization (skip parse) — level 1 + 2
**Probe Family**: On-CPU + memory allocation
**Bottleneck**: every quiescent poll re-parses (66 ms) and re-decodes (68 ms) an identical payload with zero reuse.
**Expected Metric Movement**: quiescent poll → ~1–5 ms **iff** the transport exposes a pre-parse byte fingerprint; hash-miss path identical to post-opt-3 behavior. Waits observe real changes with zero added latency.
**Story**: `feature-perf-wait-snapshot-pipeline-opt-4`

**Design decisions (brief item 1), resolved:**
- **Where to hash**: the **raw response bytes at the transport return**, not
  the parsed `nodes` array. Rationale in the transport finding: hashing the
  parsed nodes forfeits the parse-skip and, once opt 1/opt 3 land, its walk
  costs roughly the decode it would save. A raw-byte fingerprint is the only
  form that can skip both parse and decode.
- **What is reused**: the entire previously installed `ActiveSnapshot`
  (`generation`, `bindings`, `node_by_backend`, `semantic`, `parent_by_node`,
  `dom_semantics_captured`, `next_node_id`) **under the same generation**, plus
  the returned `PageSnapshot` rebuilt from the retained `nodes`. Byte-identical
  payload ⇒ identical decode because decode is deterministic and node identity
  is keyed by stable `backendDOMNodeId` seeded from the same prior
  `node_by_backend`; `begin_snapshot` already returns the same generation when
  attachment generation and document fingerprint are unchanged, so reuse rides
  the registry's existing "unchanged document ⇒ same generation" model.
- **What invalidates**: any byte difference in the AX payload (or, for
  DOM-bearing captures, the DOM payload); a document fingerprint change
  (`frame_id`/`loader_id`); or an attachment-generation change. Any of these
  forces the full miss path (new generation as today) — the short-circuit
  never suppresses a real new-generation transition.
- **Scope**: applies to **both** waits and `query_page`-triggered captures,
  gated to captures with `include_document_geometry == false` (the wait +
  presence-probe + plain `query_page` path). Geometry-bearing `snapshot_page`
  captures are request-varying and skip the short-circuit. Reuse additionally
  requires the prior snapshot's `dom_semantics_captured` to satisfy the current
  request's `requires_dom_semantics()` (a DOM-needing request cannot reuse an
  AX-only snapshot).

#### Implementation Units

##### Unit 4.1: Response fingerprint at the transport seam
**File**: `crates/krometrail-cdp/src/transport/mod.rs`, `crates/krometrail-cdp/src/transport/cdpkit.rs`

```rust
// Additive, snapshot-scoped; generic send_raw for all other commands is untouched.
fn send_raw_fingerprinted(
    &self, scope: &CommandScope, method: &str, params: serde_json::Value,
) -> TransportFuture<'_, Result<(serde_json::Value, ResponseFingerprint), TransportError>>;
```

**Implementation Notes**:
- `ResponseFingerprint` is a fast non-cryptographic 128-bit digest of the raw
  response body bytes.
- **Transport decision required (host attention).** cdpkit 0.4.0 parses every
  frame to `Value` in its read loop and discards the bytes, so a *pre-parse*
  digest is not reachable behind the unmodified adapter. Options for the host
  to choose at implementation time: (a) a minimal cdpkit read-loop hook /
  patched fork that hashes the text frame before `from_str` (a genuine
  transport decision per ARCHITECTURE.md, keeping the seam replaceable);
  (b) ship opts 1–3 only and hold opt 4 until the transport decision is made.
  The design does not silently downgrade to a parsed-`Value` digest.
- Test transports implement `send_raw_fingerprinted` by hashing the serialized
  double they already hold.

##### Unit 4.2: Reuse the ActiveSnapshot on an unchanged fingerprint
**File**: `crates/krometrail-cdp/src/control/snapshot.rs` (`capture_snapshot_for_frame`, `SnapshotRegistry`)

```rust
// ActiveSnapshot gains: ax_fingerprint: ResponseFingerprint, dom_fingerprint: Option<ResponseFingerprint>.
// In capture_snapshot_for_frame (geometry-free path only):
//   fetch AX (+DOM) via send_raw_fingerprinted
//   if let Some(active) = registry.reuse_candidate(target, attachment_gen, &document,
//          requires_dom, ax_fp, dom_fp) { return rebuild_snapshot_from(active); }
//   else -> existing decode + install (miss path unchanged)
```

**Implementation Notes**:
- `reuse_candidate` returns the active snapshot only when: same
  `attachment_generation`, same `document` fingerprint, `ax_fingerprint`
  matches, `dom_fingerprint` matches when a DOM payload was fetched, and
  `dom_semantics_captured >= requires_dom`. It still re-reads the document
  fingerprint after the DOM fetch exactly as today (the existing
  "document changed while capturing" stale guard) before trusting reuse.
- The rebuilt `PageSnapshot` reuses the retained decoded `nodes` (clone of the
  installed set or an `Arc` retained on the active snapshot) and a fresh
  `ObservationContext` (new `started_at`/`completed_at`); generation and
  omitted-count are carried from the active snapshot.
- Hash-miss path is byte-for-byte the current decode+install, so a real change
  is observed on the very next poll with zero added latency beyond the digest.

**Acceptance Criteria**:
- [ ] Two consecutive captures of a byte-identical payload decode once; the
      second reuses (asserted via a decode-call counter on a test transport).
- [ ] A one-byte payload change, a document-fingerprint change, and an
      attachment-generation change each force a full decode and (for the latter
      two) a new generation.
- [ ] A DOM-needing request does not reuse an AX-only active snapshot.
- [ ] `snapshot_page` (geometry) captures never take the short-circuit.

## Benchmarks

**Location**: `crates/krometrail-cdp/src/control/snapshot.rs` `#[cfg(test)]`
module, new `#[ignore]` probe tests prefixed `perf_`, reusing the existing
synthetic builders `ax_tree_with_node_count`, `ax_tree_with_container_node_count`,
`dom_snapshot_with_container_node_count`, and the in-module test transport
double. Each test builds the payload once, times N iterations with
`std::time::Instant`, and prints `ns/op` via `--nocapture` (no criterion
dependency added; matches the repo's harness-free convention).

**Run command**:
```bash
cargo test -p krometrail-cdp --release -- --ignored perf_ --nocapture
```

**Scaffold (baseline → target), all at 50k nodes:**

| `#[ignore]` test | Measures | Baseline | Target |
|---|---|---|---|
| `perf_decode_ax_50k` | `decode_ax_tree_with_ids` alone | 68.39 ms | ≤ 34 ms (≥2×); goal ~20 ms |
| `perf_probe_text_miss_50k` | `probe_presence` no-match (Text) | 55.19 ms | low single-digit ms |
| `perf_container_contains_50k` | container_text `contains` query | 16.20 ms | ~2–3 ms |
| `perf_poll_role_name_50k` | full simulated poll (AX capture→decode→probe), quiescent | 111.4 ms | miss ≤ ~86 ms; quiescent ~1–5 ms (opt 4, transport-gated) |
| `perf_poll_text_50k` | full simulated poll (AX+DOM), quiescent | 188.5 ms | miss parse-dominated; quiescent ~1–5 ms (opt 4, transport-gated) |

**Counter targets (optional, opt 1 / opt 3):** transient allocations per
text-miss probe ~100k → O(1); per-node `to_owned` clones in decode eliminated.
Use a counting global allocator shim in the `perf_` tests if allocation
evidence is wanted.

Provenance note in each `perf_` test doc comment: parse-to-`Value`
(65.71 ms @ 50k) is cdpkit-intrinsic and excluded from the decode/probe
targets; quiescent ~1–5 ms is reachable only with opt 4's transport
fingerprint.

## Implementation Order

1. **opt-1** (probe normalization + relaxed fusion) — highest confidence,
   transport-independent, fixes the steady-state wait cost; lands
   `normalized_match_text` + `NormalizedTextNeedle` used by later opts.
2. **opt-2** (container memo) — small, depends on opt-1's normalized key.
3. **opt-3** (typed AX decode) — transport-independent decode win; halves the
   hash-miss path.
4. **opt-4** (unchanged-payload short-circuit) — reuses opt-3's decoded
   `ActiveSnapshot`; **hold or split on the transport decision** in unit 4.1.

## Risks

- **Short-circuit correctness (pre-mortem).** How reuse could return a stale
  snapshot, and the guard that prevents it:
  - *A real change slips past because the fingerprint collides.* Mitigation:
    128-bit digest over the full raw body; collision probability negligible;
    and the fingerprint is only trusted alongside an unchanged document
    fingerprint and attachment generation.
  - *The document changed but the AX payload happened to be byte-identical
    (e.g. reload to identical content).* The document fingerprint
    (`frame_id`/`loader_id`) changes on document replacement and forces the
    miss path + new generation, so references correctly invalidate even when
    AX bytes match.
  - *DOM changed while AX did not (or vice-versa).* Both payloads are
    fingerprinted for DOM-bearing captures; either differing forces re-decode.
  - *Reusing across a capability widening* (AX-only snapshot reused for a
    DOM-needing query). Guarded by the `dom_semantics_captured >=
    requires_dom` check.
  - *Latency regression on the miss path.* The digest is the only added work on
    a miss; it is memory-bandwidth-bound and dwarfed by parse. Hash-miss path
    is otherwise byte-identical to today.
  - *Generation churn suppressed.* Reuse only ever returns the *same*
    generation for an unchanged document — exactly `begin_snapshot`'s existing
    behavior — and never blocks the new-generation path that real changes take.
- **Transport decision (host attention).** Opt 4's headline ~1–5 ms depends on
  a pre-parse byte fingerprint that cdpkit 0.4.0 does not expose. This is a
  transport-seam decision (patch/hook vs. hold). Opts 1–3 deliver the
  transport-independent wins (steady-state probe 55→~3 ms, container 16→~3 ms,
  decode 68→~20 ms, miss poll 188→~86–100 ms) and can ship without it.
- **Match-semantics preservation.** Opt 1 changes representation, not
  semantics. The equivalence tests (units 1.1/1.2) and the untouched
  `rendered_text` field are the guardrail; any divergence is a bug, not an
  accepted behavior change.
- **Memory.** `normalized_match_text` adds one bounded String per text-bearing
  node (≤ `MAX_SEMANTIC_QUERY_TEXT_BYTES`, ≤ the raw rendered length). Bounded
  and acceptable; revisit only if snapshot memory becomes a separate concern.

## Host adjudication (scope decision)

Opt-4 (unchanged-payload short-circuit) is deferred out of this feature's
implementation: its ~1–5 ms quiescent-poll target requires a pre-parse byte
fingerprint that cdpkit 0.4.0 does not expose (its read loop parses every
frame to `Value` and discards the bytes), and a parsed-`Value` fingerprint
recovers only ~10–15 ms over opts 1–3. Forking or hooking cdpkit is an
external-dependency decision outside this feature. Opts 1–3 are in scope;
the transport-seam follow-up is parked as
`idea-cdpkit-byte-fingerprint-hook`. Story opt-4 is closed unimplemented
with this rationale.

## Implementation notes

Opts 1–3 are implemented without changing the transport contract or the
snapshot bounds. Semantic needles are normalized once per evaluation, decoded
rendered text and labels retain normalized internal keys, relaxed and
uncontained diagnostics share the primary node traversal, container verdicts
are memoized per matcher evaluation, and the AX decoder consumes the owned
response `Value` through typed serde structs with integer-indexed assembly.

The ignored benchmark scaffold was run in release mode with one benchmark
thread (`cargo test -p krometrail-cdp --release -- --ignored perf_ --nocapture
--test-threads=1`; this workspace used its existing `target/` directory because
the configured `/storage/cargo-target` is read-only). Recorded design
baselines versus the post-implementation measurements on this host are:

| Benchmark | Before | After | Design target |
|---|---:|---:|---:|
| `perf_decode_ax_50k` | 68.39 ms | ~34.1 ms | ≤34 ms, goal ~20 ms |
| `perf_probe_text_miss_50k` | 55.19 ms | ~8.8 ms | low single-digit ms |
| `perf_container_contains_50k` | 16.20 ms | ~5.2 ms | ~2–3 ms |
| `perf_poll_role_name_50k` | 111.4 ms | ~32.6 ms | miss ≤~86–100 ms |
| `perf_poll_text_50k` | 188.5 ms | ~49.4 ms | miss parse-dominated |

The before column is the feature's measured release-build evidence; the
after harness uses the in-module synthetic transport and therefore excludes
cdpkit's intrinsic parse-to-`Value` cost and browser round trips. Decode is
within measurement noise of the requested twofold reduction and remains about
0.1 ms above the scaffold's strict ≤34 ms target on this host; probe and
container matching also remain above their aspirational low-single-digit and
2–3 ms targets, while both simulated poll paths remain below their miss targets.

---
id: feature-perf-wait-snapshot-pipeline
kind: feature
stage: drafting
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

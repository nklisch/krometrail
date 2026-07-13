---
id: epic-durable-browser-memory-range-resolution
kind: feature
stage: drafting
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on: [epic-durable-browser-memory-sqlite-index]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Natural-Anchor and Explicit Temporal Range Resolution

## Brief

Own the single temporal range resolver that every temporal request passes through. It accepts natural anchors — explicit session-relative time, explicit timestamps, interaction identifier, a window before and after an interaction, the most recent interaction, navigation or marker identifier, or a source-frame range — and resolves them to one explicit `ResolvedRange` against the SQLite index before any artifact generation or source-frame retrieval runs. Concentrating resolution here prevents each downstream consumer (the sibling `epic-temporal-debugging-workflow` and `epic-temporal-vision-toolkit` epics) from interpreting natural anchors differently.

The resolver reads from the SQLite index feature (frame ids, interaction ids, navigations, markers, gaps in a window) and produces a `ResolvedRange` carrying session, target, start/end session time, ordered frame ids, interaction ids, known gaps, and retention warnings. When an unspecified interaction range is requested, the resolver applies the bounded pre-action context through the interaction lifecycle and post-action observation plus bounded trailing context, and returns the exact resolved range with every response. Queries fail clearly when part or all of the requested range was evicted, was never captured, belongs to a different target, or contains known capture gaps.

This feature does not own the SQLite index, artifact generation, the temporal visual crate, or the agent-facing debug bundle. It owns the resolver and the `ResolvedRange` core type that the temporal-query epic consumes.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: consumer of the SQLite-index feature; produces the `ResolvedRange` contract that the sibling `epic-temporal-debugging-workflow` epic consumes for artifact generation and progressive source retrieval.
- Design decisions inherited: the resolver lives in this epic because it depends on the storage indexes the store owns; artifact generation is a separate concern owned by the temporal-vision epic; an unspecified interaction query resolves to bounded pre-action context through the interaction lifecycle and post-action observation, plus bounded trailing context; the resolved range is returned with every response.

## Simplification opportunity

- One resolver, one `ResolvedRange`. Do not let the temporal-query or temporal-vision epics re-derive natural-anchor resolution; they consume `ResolvedRange` and treat it as the authoritative resolution.
- Drive the supported natural-anchor input set from a single registry or enum so adding a new anchor kind updates validation, resolution, and error messages together. Avoid a parallel hand-written switch per consumer.
- The implicit-interaction window (bounded pre/post context) is one bounded helper, not a per-call policy decision. Settle its bounds once and surface them in the resolved range.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience
- `docs/SPEC.md` — Temporal Ranges (every supported anchor kind and the failure modes), Temporal Queries (the resolved range is returned with every response; queries fail clearly on evicted / never-captured / wrong-target / gapped ranges)
- `docs/ARCHITECTURE.md` — Temporal Range Resolution (`ResolvedRange` shape), Domain Model (identifier contracts the resolver reads)
- `docs/VISUAL-EVIDENCE.md` — Artifact Provenance (the resolved time range and ordered source-frame identifiers every artifact must carry)

## Scope and honest non-goals

**In scope:**

- The `ResolvedRange` type in `krometrail-core`: session id, target id, start/end session time, ordered frame ids, interaction ids, gaps, retention warnings.
- The single resolver supporting every SPEC anchor: explicit session-relative time, explicit timestamps, interaction identifier, a window before and after an interaction, the most recent interaction, navigation or marker identifier, and source-frame range.
- The implicit-interaction window policy: bounded pre-action context through the interaction lifecycle and post-action observation plus bounded trailing context, with the exact resolved range returned in every response.
- Clear failure modes mapped to existing error categories: range evicted, never captured, belongs to a different target, or contains known capture gaps.
- Retention-warning population: a resolved range that partially overlaps evicted data carries a warning rather than silently truncating.

**Non-goals:**

- The SQLite index the resolver reads from — owned by `epic-durable-browser-memory-sqlite-index`.
- Artifact generation, the temporal visual crate, the debug bundle composition, and progressive source retrieval — owned by `epic-temporal-vision-toolkit` and `epic-temporal-debugging-workflow`. This feature produces `ResolvedRange`; those epics consume it.
- Logical element tracking across navigation, scroll, resize, or DOM replacement. A resolved range is a time and frame-id window; it does not track elements.
- Automatic comparison between interactions or sessions — explicitly deferred by `epic-temporal-debugging-workflow`'s design decisions.

## Notes for the design pass

- `ResolvedRange` is the load-bearing contract with the sibling `epic-temporal-debugging-workflow` epic. Settle its field shape in `krometrail-core` here; the consumer imports it rather than re-declaring.
- The frame-id ordering in `ResolvedRange` must respect the per-target `CaptureOrdinal` (the deterministic local order the foundation feature already produces), not raw timestamp order — timestamps can tie.
- Coordinate the frame-range read surface with the SQLite-index feature so the resolver uses the same read path as direct source-frame retrieval, rather than a second query path.
- The implicit-interaction bounds must be deterministic and reported in the resolved range so an agent can reproduce the exact window. Avoid "smart" context selection that varies run to run.
- Map range failures to existing error codes (`NotFound` for evicted or never-captured; `InvalidInput` for cross-target or malformed anchors) at the boundary; do not invent new error categories here.

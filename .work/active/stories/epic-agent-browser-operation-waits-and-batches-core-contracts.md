---
id: epic-agent-browser-operation-waits-and-batches-core-contracts
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-waits-and-batches
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Wait and Batch Core Contracts

## Checkpoint

Define the infrastructure-free public contracts for explicit waits and ordered per-target batches, extend the existing `define_browser_operations!`/`BROWSER_OPERATION_REGISTRY` declaration with `Wait` and `Batch`, and add validated Serde boundaries and stable error categories. Do not add CDP behavior, session wiring, storage, temporal queries, or MCP registration.

## Files

- `crates/krometrail-core/src/browser/wait.rs` — wait condition, request, bounded duration policy, probe, outcome, and result types.
- `crates/krometrail-core/src/browser/batch.rs` — batch request/options, step status/result, outcome, result, and admission validation.
- `crates/krometrail-core/src/browser/operation.rs` — registry variants and batchability metadata.
- `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/lib.rs` — exports.
- `crates/krometrail-core/src/error.rs` — `WaitTimedOut` (and only a batch-specific code if the stable result contract proves it necessary), retry/recovery mappings, exhaustive tests.

## Acceptance evidence

- Wait constructors enforce nonzero bounded timeout, elapsed/quiet duration <= timeout, poll interval bounds, valid locators, nonempty bounded expressions/text, and boolean page-condition semantics; malformed wire values are rejected through validated deserialization.
- Batch constructors enforce 1–64 steps, bounded timeout, one page target, no nested/browser-scoped/close/forbidden child operation, and explicit target consistency without eagerly resolving references.
- `Wait` and `Batch` are present exactly once in the existing operation registry with correct stable names, scopes, evidence, mutability, and batchability; no parallel registry exists.
- Batch results retain the exact standalone `BrowserOperationResult`, optional normal `InteractionAnchor`, child timing/error/skip reason, optional screenshot part, and exactly one final live-observation part. `batch_id` uses existing `InteractionId` only as `InteractionRecord.parent_batch` correlation and is not an invented child anchor.
- Serde round trips preserve stable snake-case tags and reject unknown/invalid shapes, duration overflow, raw negative/fractional durations, oversized text/expressions, and invalid nested batches.
- Focused core tests pass and the changed files are committed as this checkpoint. No code outside the listed contract surface is changed.

## Implementation notes

Reuse `NonEmptyText`, `ElementLocator`, `PageSelection`, `DocumentReadiness`, `ObservationContext`, `ObservationPart`, `LiveObservation`, `EncodedScreenshot`, `KrometrailError`, and `InteractionAnchor`. Use private wire structs and `deserialize_validated`; do not expose Tokio instants, CDP ids, backend node ids, or transport session ids.

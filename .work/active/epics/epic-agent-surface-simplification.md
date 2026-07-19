---
id: epic-agent-surface-simplification
kind: epic
stage: implementing
tags: [agent-ux, browser, storage, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Agent surface simplification and reliability

## Brief

Make Krometrail substantially smaller and more ergonomic for ordinary agent interaction while fixing every defect found in the post-1.1.2 comparison pass. Concise, action-centric output becomes the implicit response; `expanded` and `full` are deliberate increases in context. Replace pruned canonical snapshot trees with a flattened target index, remove superseded response variants and compatibility machinery, and update the Krometrail skill to teach progressive expansion.

The same delivery fixes disabled batch screenshots that emit fake unsupported outcomes, bounds low-information artifact fanout across multi-epoch temporal bundles, and repairs the segment-rotation persistence failure that permanently poisons capture for later sessions while discarding its safe underlying cause. The implementation should reduce code and tests wherever obsolete contracts disappear rather than layer aliases or shims over them.

## Strategic decisions

- **Response vocabulary**: omission means `concise`; explicit expansion levels are `expanded` and `full`. Do not expose a competing `standard` or `routine` name.
- **Compatibility**: there are no supported third-party integrations. Remove legacy variants, deprecated aliases, dual schemas, historical installer paths, and old-store migrations rather than preserving superseded behavior.
- **Current retained data**: keep one current store schema. Existing current-format data opens normally; older incompatible formats fail clearly with a recovery action instead of migrating.
- **Diagnostics**: failed and degraded responses always carry privacy-bounded actionable diagnostics. Diagnostic suppression is not a public preference.
- **Release shape**: deliver the coherent contract change, bug fixes, skill/docs, and generated artifacts together as a minor release.

## Simplification opportunity

Delete the public per-part legacy/compact/interaction-only/omit response matrix, ancestor-closure projection path, diagnostic-omission route parsing, compatibility-only tests and aliases, historical store migration chain, and installer branches for unsupported old releases. Keep correctness mechanisms whose names mention compatibility or stability only when they qualify the current browser protocol, deterministic evidence, or current retained format.

## Source findings

- `idea-trim-interaction-snapshots`
- `idea-omit-disabled-batch-screenshots`
- `idea-bound-multi-epoch-bundles`
- `idea-diagnose-capture-persistence-failure`

## Design decisions

- **Compatibility cleanup boundary**: remove only machinery whose purpose is preserving unsupported historical Krometrail shapes. Retain current browser-protocol qualification, deterministic wire/evidence identities, and visual-epoch compatibility checks because they enforce present correctness.
- **Projection boundary**: acquire and map canonical results first, then derive concise or expanded MCP representations. Full remains the direct complete representation; images remain an orthogonal explicit opt-in.
- **Batch absence**: model an unrequested step screenshot as absence, not an unsupported observation.
- **Temporal economy**: the default bundle generates evidence for the anchor epoch only. Explicit `all` expands across every compatible visual epoch; generic artifact generation retains explicit all-epoch behavior.
- **Persistence recovery**: a completed segment rename followed by directory-sync failure is classified and recoverable; ambiguous partial writes remain terminal. The first safe persistence cause survives capture status, diagnostics, and structured shutdown recovery.

## Decomposition

Split by authoritative capability boundary. Current-contract cleanup establishes the single supported runtime and storage baseline. The response feature then owns the new agent-facing vocabulary and flattened targeting representation. Batch and temporal features change their canonical acquisition semantics before projecting through that response layer. Segment publication and capture recovery are independent and can proceed in parallel.

### Child features

- `epic-agent-surface-simplification-current-contract` — remove historical runtime, installer, schema-migration, alias, and policy compatibility machinery while retaining one current store format — depends on: `[]`
- `epic-agent-surface-simplification-response-detail` — make concise implicit, add expanded/full progression, flatten exact targets, remove the old projection matrix, and update skill/docs — depends on: `[epic-agent-surface-simplification-current-contract]`
- `epic-agent-surface-simplification-optional-batch-evidence` — represent disabled or unattempted batch screenshots as absence and retain errors only for requested evidence failures — depends on: `[epic-agent-surface-simplification-response-detail]`
- `epic-agent-surface-simplification-bounded-temporal-bundles` — default debug bundles to the anchor visual epoch, make all epochs explicit, and avoid default read-then-discard artifact work — depends on: `[epic-agent-surface-simplification-response-detail]`
- `epic-agent-surface-simplification-persistence-recovery` — recover segment publication after post-rename directory-sync failure and preserve safe persistence causes through capture and shutdown — depends on: `[]`

### Simplification arcs

- Current contract — collapse historical SQL migrations to the current schema bootstrap/version check; delete unsupported installer/version branches, aliases, and adapter defaults used only for source compatibility.
- Response detail — delete legacy/compact/interaction-only/omit enums and branches, ancestor-closure projection, omission markers, diagnostic suppression, duplicate diagnostic request parsing, and compatibility-specific tests.
- Batch evidence — delete fabricated unsupported screenshot observations and their helper/import branches.
- Temporal bundles — delete frozen v1 bundle-policy version machinery and the default epoch/output Cartesian fanout.
- Persistence recovery — replace discarded generic errors and bare degraded shutdown enums with one classified failure path rather than adding parallel diagnostic structures.

### Decomposition risks

- Flattened targets must remain directly usable by existing exact-reference actions without weakening target, attachment, or document-generation fences.
- Removing historical migrations must still open the current schema byte-for-byte and fail older formats before mutation with a clear recovery action.
- Segment publication recovery must distinguish a completed rename from ambiguous partial frame/footer writes; treating every I/O error as retryable would risk corrupting evidence.
- Anchor-epoch selection must preserve the full resolved-range and gap provenance even when only one epoch produces artifacts.

## Release intent

Ship as the next minor release after integrated review and focused real-browser validation. Release binding remains late-bound until implementation and review are complete.

---
id: epic-agent-surface-simplification
kind: epic
stage: drafting
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

## Anticipated child features

- Current-contract and compatibility-cruft removal.
- Concise/expanded/full response projection with flattened exact targets and skill guidance.
- Truthful economical batch and multi-epoch temporal acquisition.
- Recoverable segment persistence with durable safe diagnostics.

## Release intent

Ship as the next minor release after integrated review and focused real-browser validation. Release binding remains late-bound until implementation and review are complete.

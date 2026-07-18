---
id: epic-agent-browser-ergonomics-semantic-targeting-query-resolution
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-semantic-targeting
depends_on: [epic-agent-browser-ergonomics-semantic-targeting-query-contract]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Resolve semantic queries through the snapshot registry

Atomically enrich the active accessibility snapshot with bounded DOMSnapshot-derived label, rendered-text, and test-id metadata; match candidates in document preorder with stale descendant-scope fencing; expose the registry-derived MCP response; and qualify the workflow in real Chrome and the plugin skill.

## Acceptance evidence

- Scripted adapter tests protect DOM/AX joining, all query kinds, scope, ambiguity, limits, and fail-closed malformed input.
- Real Chrome resolves a unique reference, uses it in an existing mutation, exposes ambiguous matches, and invalidates stale references after navigation.
- MCP results remain bounded, image-free, and explicit about no-match/unique/ambiguous/truncated outcomes.

## Ordering

Depends on `epic-agent-browser-ergonomics-semantic-targeting-query-contract`; completes the feature's externally usable slice.

## Implementation notes

- Execution capability: direct inline implementation across the cohesive CDP, core, MCP, fixture, and plugin-skill boundary.
- Review weight: standard child story; no independent story review required before the parent feature review.
- Changed `crates/krometrail-cdp/src/control/{mod.rs,snapshot.rs}`, `crates/krometrail-cdp/tests/verified_interactions.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-mcp/src/{response.rs,schema.rs}`, `tests/fixtures/browser/verified-interactions/index.html`, and `plugin/skills/krometrail/SKILL.md`.
- Added bounded main-document DOMSnapshot enrichment joined to the active AX snapshot, fail-closed validation, actionable-node filtering, descendant scope fencing, document-order results, and explicit no-match/unique/ambiguous/truncated outcomes.
- Covered role/name, explicit/wrapping/ARIA labels, rendered descendant text, test IDs, scope, ordering, match limits, malformed snapshot input, stable MCP schema/serialization, and image-free results.
- Qualified the complete workflow in real Chrome: semantic query to exact reference, existing click mutation, ambiguous and scoped outcomes, and stale-reference invalidation after navigation.
- Simplification: retained one snapshot/reference registry and returned its exact references directly; no parallel locator cache or action-time query reevaluation was introduced.
- Design reconciliation: ordinary snapshot/live observation remains AX-only, while semantic DOM enrichment is query-triggered through the shared capture path. This preserves the established observation latency contract while keeping semantic resolution atomic and main-document-only.
- Adjacent findings: none.

## Verification

- `cargo fmt --all`
- `cargo test -p krometrail-core browser::observation::tests --locked`
- `cargo test -p krometrail-core browser::operation::tests --locked`
- `cargo test -p krometrail-cdp control::snapshot::tests --locked`
- `cargo test -p krometrail-cdp --test verified_interactions --locked`
- `cargo test -p krometrail-mcp --locked`
- `cargo check --workspace --all-targets --locked`
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --features cdpkit-transport --test verified_interactions opt_in_real_chrome_resolves_semantic_queries_to_exact_references --locked -- --nocapture`

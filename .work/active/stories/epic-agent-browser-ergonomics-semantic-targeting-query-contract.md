---
id: epic-agent-browser-ergonomics-semantic-targeting-query-contract
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-semantic-targeting
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Define the semantic query contract

Add the validated `SemanticQuery`, text-match, request, bounded result, and explicit outcome domain types, then declare `query_page` once in the browser-operation registry with page selection and batch inheritance.

## Acceptance evidence

- Core unit tests cover defaults, normalization, validation bounds, and all result outcomes.
- Generated schema tests prove the four variants and limits while existing operation request shapes remain unchanged.
- `query_page` is read-only, page-scoped, requested-only, batchable, and contributes no standalone image.

## Ordering

This contract checkpoint has no sibling dependency. Query resolution depends on these stable types and the registry route.

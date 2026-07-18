---
id: gate-docs-semantic-query-two-step-contract
kind: story
stage: done
tags: [documentation]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: docs
created: 2026-07-18
updated: 2026-07-18
---

# Describe semantic query and mutation as a two-step contract

## Drift category
foundation-doc-assertion

## Location
- Doc: `docs/ARCHITECTURE.md:148`
- Contradicting source: `docs/SPEC.md:137`

## Current doc text
> Semantic locators ... narrow to exactly one backing node before an operation dispatches.

## Contradiction
`query_page` can successfully return `no_match`, `ambiguous`, or `truncated`; only a unique exact reference can later authorize mutation.

## Required edit
Replace the assertion with the active bounded-query then exact-reference mutation contract.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small foundation-assertion correction grounded in the final core/CDP contract.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Replaced the stale `docs/ARCHITECTURE.md` assertion with the two-step bounded query outcome → unique generation-scoped `NodeReference` → later revalidated mutation contract.
- Verification: checked `SemanticQueryOutcome`, `QueryPageResult`, and the CDP query/authority boundary; `bun run docs:build` and diff checks passed.
- Discrepancies and adjacent issues: none.

## Inline review

- Verdict: pass. Architecture now matches the bounded query outcomes, unique-reference authority, and later mutation revalidation contract.

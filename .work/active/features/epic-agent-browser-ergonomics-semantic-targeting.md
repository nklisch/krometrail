---
id: epic-agent-browser-ergonomics-semantic-targeting
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Semantic query-to-reference targeting

## Brief

Let agents find exact actionable references through bounded semantic queries over the current main-document page snapshot: accessible role/name, label text, visible text, and test identifier, with descendant scope. The query returns zero, one, or bounded-many explicit matches and never silently chooses among ambiguous nodes. Existing mutation tools continue accepting exact references, preserving stale-reference and pre-dispatch safety semantics.

This feature does not add persistent locator identities or automatic action-time reevaluation. Frame identity and scope belong to the dependent browser-context feature; DOM enrichment required for label and test-id matching belongs in the existing snapshot registry.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent targeting foundation consumed by browser-context discovery

## Simplification opportunity

Use one registry-derived read-only browser operation and the existing generation-scoped node reference authority instead of adding Playwright-like locator objects to every action schema.

## Foundation references

- `docs/SPEC.md` — Structured Page Snapshots
- `docs/ARCHITECTURE.md` — Domain Model and Target Lifecycle

---
id: epic-browser-interface-hardening-page-context-semantics
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-browser-interface-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Truthful Page-Context Semantics

## Brief

Repair four places where Krometrail's structured browser model diverges from what an agent can see or recover from: unnamed controls cannot be associated with bounded adjacent/container text, same-origin frame queries select the wrong semantic document, resource timing initiators misclassify obvious script/font URLs, and `focus: preserve` hidden-target failures claim foreground activation occurred and suggest unavailable recovery.

Keep semantic matching deterministic and bounded. Reuse document fingerprints and current query/reference authority, reconcile resource identity at ingestion, and make recovery text accurately reflect the requested focus policy and available operations.

## Source findings

- `idea-associate-unnamed-controls`
- `idea-fix-nested-frame-query`
- `idea-fix-asset-kind-classification`
- `idea-fix-hidden-target-recovery`

## UI alignment

No UI surface; this is a browser-control and error-contract feature.

---
id: release-1.0.1
kind: release
stage: quality-gate
tags: []
parent: null
depends_on: []
release_binding: 1.0.1
gate_origin: null
created: 2026-07-16
updated: 2026-07-16
---

# Release 1.0.1

Short stable patch release for native plugin distribution and exact release-coupled managed binary activation.

## Bound items

- `agent-plugin-distribution`
- `agent-plugin-distribution-canonical-package`
- `agent-plugin-distribution-isolated-qualification`
- `agent-plugin-distribution-marketplace-publication`
- `plugin-managed-binary-bootstrap`
- `plugin-managed-binary-bootstrap-launcher-and-installer`
- `plugin-managed-binary-bootstrap-qualification-and-docs`
- `plugin-managed-binary-bootstrap-release-version-sync`
- `story-fix-release-cross-version-tag`

No archived stubs were unbound. The operator confirmed the complete post-v1.0.0 set.

## Gate runs

- **gate-security** (2026-07-16) — 1 Medium release hardening item deferred to backlog by operator; 1 Low ambient finding already tracked as `gate-security-redact-nested-browser-event-secrets`; no Critical or High findings.
- **gate-tests** (2026-07-16) — 2 release-relevant gaps (1 High, 1 Medium), both fixed and verified: ordinary CI now runs the hermetic bootstrap suite, which covers all supported platform mappings and unsupported-host failures.

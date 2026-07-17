---
id: epic-agent-browser-reliability-agent-contracts
kind: feature
stage: drafting
tags: [agent-ux, browser, storage]
parent: epic-agent-browser-reliability
depends_on: [durable-agent-diagnostics, epic-agent-browser-reliability-capture-outcomes, epic-agent-browser-reliability-managed-session-lifecycle, epic-agent-browser-reliability-interaction-semantics, epic-agent-browser-reliability-viewport-emulation]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Precise agent contracts and guidance

## Brief

Resolve GitHub issues #6 and #12 after the runtime contracts they describe are complete. Publish MCP input schemas whose nested locator, target, modifier, fill, viewport, temporal range, and selection unions survive Codex declaration projection instead of becoming `unknown`, while keeping canonical Rust-generated schemas authoritative. Invalid requests identify the first mismatched field path without echoing sensitive values.

Update the Krometrail skill with valid CSS-selector and snapshot-reference examples, safe defaults, the economical interaction-evidence hierarchy, capture-health prerequisites, compositor/partial-frame recovery, and targeted diagnostic-log collection by correlation identifier. The guidance must distinguish automatically returned post-operation screenshots, `observe_live`, and persisted source frames.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: terminal consumer of every runtime feature so generated declarations and prose match shipped behavior.

## Simplification opportunity
- Normalize generated schemas at the MCP boundary and derive examples from stable request shapes rather than maintaining parallel handwritten type inventories.

## Foundation references
- `docs/SPEC.md` — MCP schemas, errors, and browser-control surface
- `docs/ARCHITECTURE.md` — registry-derived tools and generated contracts
- `docs/VISUAL-EVIDENCE.md` — evidence hierarchy and provenance

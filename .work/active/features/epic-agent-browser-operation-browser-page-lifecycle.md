---
id: epic-agent-browser-operation-browser-page-lifecycle
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-page-observation]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Browser and Page Lifecycle

## Brief

Turn the existing supervised Chrome connection into the ordinary browser workspace an agent can operate. Expose start, explicit attach, stop/detach, and status together with page listing, creation, selection, closure, navigation, reload, and backward/forward history, returning post-operation live observations and interaction anchors for every state-changing standalone operation.

Reuse the production connector's managed-profile defaults, exact target identities, local endpoint validation, capability-probed Electron renderer attachment, reconnect, and ownership-correct shutdown. This feature adds control services and CDP page operations; it does not rebuild process or target supervision, define rich element interactions, batch operations, persist interaction history, or register MCP tools.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: consumer of `epic-agent-browser-operation-page-observation`; can progress independently of rich interaction after that shared boundary lands
- Inherited decisions: isolated reusable managed profiles remain the default; attach and temporary/named profile workflows are explicit; Electron Node main-process control remains excluded

## Simplification opportunity

- Extend the existing `BrowserConnector`, supervised session, and exact-key target state rather than creating a second lifecycle service, active-page target registry, profile manager, reconnect loop, or Electron-specific adapter.

## Foundation references

- `docs/VISION.md` — Core Experience and Local-First Operation
- `docs/SPEC.md` — Browser Lifecycle, Sessions and Targets, Current-State Observation, and Browser-Control Surface
- `docs/ARCHITECTURE.md` — Browser Connection, Target Lifecycle, Interaction Execution, and Failure Isolation
- `docs/EVALUATION.md` — Browser-Control Evaluation

<!-- The feature-design pass will fill in interfaces, signatures, and implementation units. -->

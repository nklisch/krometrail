---
id: epic-agent-browser-operation-mcp-control-surface-registry-and-session
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-cancellable-execution]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Dynamic Registry and Session Lifecycle

## Checkpoint

Build the capability-filtered rmcp dynamic route registry and one-active-session owner. Derive the 24 standalone routes and composable batch schema from the shared operation/capability contracts, and add the four existing-port lifecycle tools without creating a second browser-operation runtime.

## Likely files

- `crates/krometrail-mcp/src/{config,schema,session,registry,server}.rs`
- `crates/krometrail-mcp/src/lib.rs`
- focused fake connector/session tests

## Acceptance evidence

- Default `Control` registration contains exactly `start_browser`, `attach_browser`, `browser_status`, `stop_browser`, and every `BROWSER_OPERATION_REGISTRY` stable name once; rmcp lists them deterministically.
- Disabled `Control` contributes no lifecycle or operation tools. Invalid, duplicate, dependency-missing, and unavailable capability selections fail before serving; enabled capabilities with no implemented definitions add no speculative tools.
- `ToolRoute::new_dyn` handlers inject the selected registry name and deserialize the existing tagged `BrowserOperationRequest`; valid operations invoke exactly one `BrowserSessionPort::execute` with rmcp request cancellation bridged through `BrowserOperationContext`. Invalid input invokes it zero times.
- Standalone schemas are generated from declared request types. The generated recursive batch schema retains exactly enabled definitions where `batchable` is true, identified by generated `operation.const` branches; unexpected Schemars layout fails initialization rather than publishing a permissive schema.
- Registry-derived annotations use operation mutability; lifecycle annotations are fixed and conservative. No per-operation annotation/name/schema mirror exists in MCP.
- `BrowserSessionOwner` enforces one active session, serializes start/attach publication, returns explicit no-session/already-active lifecycle errors, removes before stop, and converges action/stop races through the existing session cancellation path.

## Out of scope

Do not add response images/resources, stdio root wiring, direct CDP calls, global singleton state, multiple concurrent browser sessions, HTTP transport, storage, or temporal/event tools.

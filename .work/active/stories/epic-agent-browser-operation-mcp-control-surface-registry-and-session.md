---
id: epic-agent-browser-operation-mcp-control-surface-registry-and-session
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-cancellable-execution]
release_binding: 1.0.0
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

## Implementation notes

- Execution capability: highest from the autopilot caller for the public dynamic-router and one-session lifecycle boundary.
- Review weight: `standard` from the autopilot caller; child checkpoint verification advances directly to done.
- Files changed: new MCP config, schema, session-owner, registry, server, and provisional response modules; MCP crate exports/dependencies and lock.
- Tests added: generated batch filtering, complete/sorted capability-driven registration and annotations, disabled-control omission, and serialized one-session lifecycle/dispatch/stop ownership.
- Simplification: 24 routes iterate the core registry; route handlers inject the tagged envelope internally and lifecycle remains a fixed four-tool port adapter rather than a second operation enum.
- Discrepancies from design: response projection is deliberately a bounded provisional envelope until the immediately dependent response-mapping checkpoint; no public stdio command exposes it yet.
- Adjacent issues parked: none.

## Completion evidence

- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 test -p krometrail-mcp --locked` passed all 4 MCP registry/schema/session tests.
- The router returns rmcp-compatible `Arc<JsonObject>` schemas, fails closed on an unexpected generated batch union, and sorts the 28 control tools by stable name.

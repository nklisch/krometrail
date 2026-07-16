---
id: epic-temporal-debugging-workflow-mcp-investigation-surface-resource-server-and-root-composition
kind: story
stage: done
tags: [agent-ux, browser, storage]
parent: epic-temporal-debugging-workflow-mcp-investigation-surface
depends_on:
  - epic-temporal-debugging-workflow-mcp-investigation-surface-response-resources-and-inline-evidence
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal MCP Resource Server and Root Composition

## Checkpoint

Publish the resource templates through rmcp 0.11, wire all already-composed temporal services into the existing stdio server, and preserve one runtime/session/storage authority. Advertise only the MCP 2025-06-18 capabilities actually implemented.

## Likely files

- `crates/krometrail-mcp/src/{server.rs,resources.rs}`
- `src/app.rs`
- `crates/krometrail-mcp/src/{lib.rs,session.rs}` only for wiring corrections
- in-memory protocol and root composition tests

## Design

- Override `list_resources` with an empty concrete list and `list_resource_templates` with the two strict artifact/frame templates. Dynamic retained evidence is discovered from tool links, not by enumerating potentially large storage.
- Advertise `ServerCapabilities::builder().enable_tools().enable_resources().build()` and `ProtocolVersion::V_2025_06_18`. Do not advertise `subscribe`, list-change, task, or later-protocol capabilities.
- Implement `read_resource` through the Unit 3 URI/resource authority. Leave subscription methods unsupported and return narrow rmcp errors with stable Krometrail data.
- Pass `McpDependencies` from `RuntimeDependencies` in `src/app.rs`. Keep one concrete `RecordingStore`, one artifact generation/cache service, one progressive service, one bundle service, one context service, and one `BrowserSessionOwner`.
- Preserve stdout protocol ownership and existing signal/EOF shutdown semantics. Temporal cancellation must not become session shutdown or leave a dropped mutation running.

## Acceptance evidence

- [ ] rmcp initialize negotiates 2025-06-18, tools/resources are advertised, templates are deterministic, and unsupported subscriptions/tasks/list-change features are absent.
- [ ] Resource read wire responses contain exact blob contents and canonical URI; errors use invalid-params/resource-not-found/internal categories with stable domain data.
- [ ] Root pointer identity proves MCP receives the pre-composed services and shared store/artifact authority; no MCP cache/decoder/payload map exists.
- [ ] EOF, SIGINT, and SIGTERM stop/detach the active browser once and do not emit non-protocol stdout.
- [ ] Control-only and temporal-disabled configurations preserve the control surface and omit temporal tools/templates.

## Ordering constraints

Depends on the response/resource projection checkpoint. Qualification cannot begin until the server has a real rmcp resource read path and root wiring.

## Implementation notes

- Execution capability: inline feature-owner implementation; the server/resource/root wiring is one cohesive boundary and depends on the completed response/resource authority.
- Review weight: standard, default; verification is covered by the focused protocol and composition tests plus the locked workspace gates.
- Files changed: `crates/krometrail-mcp/src/{server.rs,resources.rs,registry.rs}`, `src/app.rs`, and focused in-memory protocol/resource tests.
- Tests added: exact resource blob/MIME/URI authority coverage, MCP initialize/resource-list/template/read/unsupported-subscription traffic over Tokio duplex, control-only template omission, and shared runtime/session Arc identity checks.
- Simplification: retained the existing one-session stdio shutdown path and reused the existing request cancellation bridge; no MCP cache, decoder, payload map, subscription path, or compatibility constructor was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification evidence

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` (634 passed, 1 ignored)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

## Out of scope

No new browser operation, storage schema, resource subscription, remote transport, task API, or paid evaluation. Final integrated qualification remains deferred to the next story.

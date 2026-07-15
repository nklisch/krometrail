---
id: configurable-capture-cadence-mcp-generated-contracts
kind: story
stage: implementing
tags: [browser, agent-ux, testing]
parent: configurable-capture-cadence
depends_on: [configurable-capture-cadence-session-capture-forwarding]
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Expose the shared stride through generated MCP lifecycle contracts

## Checkpoint

Make `start_browser` and `attach_browser` advertise and parse the same core
`every_nth_frame` request field. Preserve the existing generated-route and session-owner flow;
MCP adds no local request, validation, setting, or checked-in schema artifact.

## Exact implementation

**Files**:

- `crates/krometrail-mcp/src/registry.rs`
- `crates/krometrail-mcp/src/schema.rs` only if a focused assertion seam is needed
- `crates/krometrail-mcp/src/session.rs`
- existing MCP lifecycle/schema tests

Keep the existing route mapping:

```rust
LifecycleKind::Start  => type_input_schema::<LaunchBrowser>()?;
LifecycleKind::Attach => type_input_schema::<AttachBrowser>()?;
```

`parse_arguments` must deserialize into the core request types before calling
`BrowserSessionOwner::start` or `attach`. Valid values are forwarded unchanged. Invalid values
produce the existing visible `InvalidInput` response before any connector call. The returned
browser and capture statuses expose the request-bound typed value from the preceding CDP/session
checkpoint.

The schema test should inspect the generated `serde_json::Value`/route schema for both lifecycle
routes and compare their cadence property shape, rather than storing a JSON snapshot. The field is
optional, integer-valued, bounded 1..=60, and defaulted to 1. Do not add a route, capability, MCP
environment variable, CLI flag, config-file setting, compatibility alias, or hand-written
regeneration file.

## Acceptance evidence

- [ ] Generated `start_browser` and `attach_browser` schemas contain identical optional
      `every_nth_frame` properties with minimum 1, maximum 60, and default 1.
- [ ] Omitted MCP arguments reach core defaults; 1, 7, and 60 are accepted; 0, 61, null, strings,
      and fractions fail before the connector is called.
- [ ] MCP lifecycle/status results and capture-state events expose the same request-bound value.
- [ ] Tests prove generated schemas and forwarding through the existing owner, without a duplicate
      request type or checked-in MCP schema artifact.

## Ordering

Depends on the completed core and CDP session checkpoints so the MCP contract test observes the
same value through a live session/status projection. Evaluation identity is the final checkpoint.

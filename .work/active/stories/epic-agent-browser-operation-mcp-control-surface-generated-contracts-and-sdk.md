---
id: epic-agent-browser-operation-mcp-control-surface-generated-contracts-and-sdk
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Generated Contracts and SDK Qualification

## Checkpoint

Qualify exact `rmcp 2.2.0` against the workspace's Rust 1.85 contract and make the existing domain request types the generated JSON-schema source for MCP. Extend the one operation declaration with capability membership and descriptions; do not register handlers or add an MCP-only request/action enum.

## Likely files

- `Cargo.toml`, `Cargo.lock`
- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/browser/{operation,observation,control,interaction,wait,batch}.rs`
- `crates/krometrail-core/src/{ids,error,time}.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-mcp/Cargo.toml`

## Acceptance evidence

- `rmcp = "=2.2.0"` uses minimal server/stdio/image features; macros, client, and HTTP transports are disabled. The intentional lock update is committed.
- `cargo +1.85.0 check --workspace --all-targets --locked` passes. A failure keeps this checkpoint open and does not raise workspace MSRV or silently change SDK versions.
- All 24 `BROWSER_OPERATION_REGISTRY` entries have one nonempty description, `CapabilityId::Control`, existing stable metadata, and an object-root input schema generated from the declared request type.
- Custom validated Serde contracts delegate `JsonSchema` to their private wire forms so integer-millisecond wait/batch fields and tagged unions are exact. `ListPagesRequest` accepts `{}` as an object input without an adapter special case.
- Representative request/schema checks cover a simple operation, validated interaction, wait, and recursive batch. Constructor/Serde validation remains authoritative for semantic constraints.
- No source file contains a copied 24-operation MCP schema or route list.

## Out of scope

No dynamic routes, session owner, response mapping, stdio service, CDP commands, storage, temporal tools, or resources are implemented here.

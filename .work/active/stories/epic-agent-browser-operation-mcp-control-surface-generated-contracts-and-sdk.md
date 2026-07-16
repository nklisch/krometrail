---
id: epic-agent-browser-operation-mcp-control-surface-generated-contracts-and-sdk
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [bug-restore-rust-1-85-contract]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Generated Contracts and SDK Qualification

## Checkpoint

Qualify exact official `rmcp 0.11.0` against the restored workspace Rust 1.85 contract and make the existing domain request types the generated JSON-schema source for MCP. Extend the one operation declaration with capability membership and descriptions; do not register handlers or add an MCP-only request/action enum.

## Likely files

- `Cargo.toml`, `Cargo.lock`
- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/browser/{operation,observation,control,interaction,wait,batch}.rs`
- `crates/krometrail-core/src/{ids,error,time}.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-mcp/Cargo.toml`

## Acceptance evidence

- `rmcp = "=0.11.0"` uses its required default server/macros/base64 features plus `transport-io`; the adapter does not use generated tool macros, and client/HTTP transports remain disabled. The intentional lock update is committed.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` passes. A failure keeps this checkpoint open and does not raise workspace MSRV, patch/vendor the SDK, or silently change SDK versions.
- All 24 `BROWSER_OPERATION_REGISTRY` entries have one nonempty description, `CapabilityId::Control`, existing stable metadata, and an object-root input schema generated from the declared request type.
- Custom validated Serde contracts delegate `JsonSchema` to their private wire forms so integer-millisecond wait/batch fields and tagged unions are exact. `ListPagesRequest` accepts `{}` as an object input without an adapter special case.
- Representative request/schema checks cover a simple operation, validated interaction, wait, and recursive batch. Constructor/Serde validation remains authoritative for semantic constraints.
- No source file contains a copied 24-operation MCP schema or route list.

## Out of scope

No dynamic routes, session owner, response mapping, stdio service, CDP commands, storage, temporal tools, or resources are implemented here.

## Implementation discovery

The hard Rust 1.85 qualification gate invalidated the exact-SDK design on 2026-07-14, before any implementation checkpoint was committed.

- Adding exact `rmcp = "=2.2.0"` with only `server`, `transport-io`, and `base64` succeeded on the host toolchain, but the authoritative command had to use the rustup Cargo shim explicitly because `/usr/bin/cargo` does not support `+toolchain` syntax.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` first rejected the pre-existing locked ICU 2.2 dependency family because it declares Rust 1.86.
- A temporary lock-only downgrade from `idna_adapter 1.2.2` to `1.1.0` removed that unrelated metadata barrier and allowed Rust 1.85 to compile source. Compilation then failed inside official `rmcp 2.2.0` at `src/model/elicitation_schema.rs:1004` and `:1015`: the crate uses let-chain syntax that Rust 1.85 reports as unstable. The current workspace also contains existing let-chain uses with the same Rust 1.85 failure, including `krometrail-core/src/browser/batch.rs` and `temporal-vision/src/provenance.rs`.
- The rmcp failure is in an unconditionally compiled model module; the selected minimal features do not remove it. Exact official rmcp 2.2.0 therefore cannot satisfy the workspace's declared Rust 1.85 contract without changing one of the design's forbidden constraints (SDK version, vendoring/patching the SDK, or workspace MSRV).
- All temporary manifest, lock, schema, and test changes were reverted. The unrelated `.work/bin/work-view` modification remains untouched.

Per the feature's design-flaw escape hatch, this checkpoint returned to `drafting`. `bug-restore-rust-1-85-contract` restored the declared workspace baseline and is now terminal; this checkpoint depends on that verified fix. The workspace MSRV was not raised and the SDK version was not silently changed.

## Redesign resolution (2026-07-14)

A descending official-release probe established exact rmcp 0.11.0 as the newest version that compiles under Rust 1.85 with a usable server/stdio configuration. Releases 0.12.0 through 2.2.0 failed the same gate. Version 0.11.0 retains every required boundary for this feature: dynamic `ToolRoute::new_dyn` registration, typed schemas, structured content, image blocks, request cancellation, stdio transport, and running-service cancellation/waiting. It supports MCP 2025-06-18; later task metadata is not required by the feature.

Version 0.11.0 requires its default macros feature for its own server/Schemars implementation to compile even though Krometrail uses dynamic routes. The dependency therefore keeps defaults and adds only `transport-io`. Registration must sort `tools/list` by stable name because this release's router returns `HashMap` iteration order. Response mapping must assign public `structured_content` after constructing bounded summary/image content, and root shutdown must use the actual 0.11.0 running-service API.

The parent feature's `## SDK compatibility redesign` section is authoritative for the remaining accepted advisory corrections. This checkpoint is redesigned and returns to `implementing`; downstream order is unchanged.

## Implementation notes

- Execution capability: highest, selected by the autopilot caller for the public generated-contract and exact-SDK boundary; direct-read implementation kept one feature owner.
- Review weight: `standard`, from the autopilot caller; child checkpoints do not receive independent review.
- Files changed: workspace/core/MCP manifests and lock; core operation, request, lifecycle, identifier, time, error, and schema-delegation contracts.
- Tests added: registry-wide nonempty description/control membership/object-root schema checks plus validated wait and recursive batch schema checks.
- Simplification: one reusable schema-delegation macro keeps custom Serde wire shapes authoritative; `ListPagesRequest` now publishes an empty object while retaining its former value spelling.
- Discrepancies from design: Schemars' UUID integration requires its `uuid1` feature; no MCP-only schema or request enum was introduced.
- Adjacent issues parked: none.

## Completion evidence

- Exact `rmcp = "=0.11.0"` with default server/macros/base64 features plus `transport-io` is committed in the workspace lock.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` passed on 2026-07-14.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 test -p krometrail-core browser::operation --locked` passed all 4 focused operation/schema tests.

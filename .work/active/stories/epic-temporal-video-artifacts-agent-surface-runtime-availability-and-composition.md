---
id: epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition
kind: story
stage: done
tags: [agent-ux, infra, testing]
parent: epic-temporal-video-artifacts-agent-surface
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Runtime-qualified video availability and composition

## Design checkpoint

Add `temporal-video` to the capability registry as runtime-qualified, resolve one immutable startup snapshot from the bounded FFmpeg qualification result, and compose the retained generation service only when qualified. MCP startup must stay healthy when unavailable and emit one privacy-safe actionable diagnostic; availability changes only after restart.

## Acceptance evidence

- Core tests cover qualified, unavailable, disabled, dependency, explicit-selection, and deterministic snapshot ordering.
- Composition tests prove one qualification result controls both service injection and capability state, with mismatches rejected before serving.
- Missing/unsupported/unsuitable FFmpeg starts the existing MCP surface normally and logs one bounded safe reason; a qualified identity enables the service without leaking paths or page data.
- The operation and bounded scoped read contracts are additive, constructor-validated, and implemented by the existing retained-generation service/store authority.

## Ordering constraints

- Root checkpoint for this feature; both upstream feature dependencies are already implemented and approved.
- MCP registration must consume this snapshot and optional service rather than rediscovering FFmpeg.

## Execution contract

- Worker capability: highest available, selected by autopilot because this checkpoint joins the security-sensitive process authority to the stable capability surface.
- Review weight: `standard`; this child closes on green evidence and the integrated feature receives one independent review pass.

## Implementation notes

- Execution capability: GPT-5.6 Sol at xhigh reasoning, the caller-selected highest-capability fallback because Luna was unavailable; one owner kept the security-sensitive process/capability/storage composition coherent.
- Review weight: `standard` from the autopilot caller; this child advanced directly to `done` after green verification and receives no child-level review.
- Files changed: `Cargo.toml`, `Cargo.lock`, core capability/video contracts, root app/diagnostics/video service and tests, and the MCP config/dependency construction seam needed to carry the optional service.
- Tests added: registry-ordered qualified/unavailable/disabled/dependency snapshot coverage; stable temporal-video operation metadata; one-result capability/service composition; and scoped maximum-byte retained-video reads through the generation service.
- Simplification: one immutable `CapabilitySnapshot` now owns registry-ordered availability; MCP-only composition consumes one FFmpeg result and reuses the retained store-backed service instead of adding handler discovery or another read authority.
- Discrepancies from design: `McpDependencies` is currently defined in flat `config.rs` rather than a separate `dependencies.rs`, so the existing module boundary was preserved; synchronous base runtime construction remains, while the bounded asynchronous FFmpeg qualification and optional service construction occur only inside `Command::Mcp` as required.
- Diagnostics: the final startup availability event contains only closed qualification stage/reason values or safe encoder/policy identity, never executable paths, page data, raw stderr, or command input; doctor does not qualify FFmpeg.
- Verification: `cargo fmt --all -- --check`; locked all-target workspace check; all 140 core tests; all 9 root app tests; all 18 root video-service tests; core and root test-target Clippy with `-D warnings` — passed.
- Adjacent issues parked: none.

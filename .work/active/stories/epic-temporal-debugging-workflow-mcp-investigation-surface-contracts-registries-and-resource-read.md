---
id: epic-temporal-debugging-workflow-mcp-investigation-surface-contracts-registries-and-resource-read
kind: story
stage: implementing
tags: [agent-ux, visual, browser, storage]
parent: epic-temporal-debugging-workflow-mcp-investigation-surface
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal MCP Contracts, Registries, and Resource Reads

## Checkpoint

Establish the exact domain contracts consumed by the temporal MCP adapter. Add the scoped source-frame read required by a resource URI, make progressive operation metadata and exposure exhaustive, define the chronological browser-event request wrapper, and generate schemas from validated wire types. This checkpoint owns no MCP routes, response content, URI parsing, or protocol lifecycle.

## Likely files

- `crates/krometrail-core/src/progressive.rs`
- `crates/krometrail-core/src/ports/frames.rs`
- `crates/krometrail-core/src/timeline/context.rs`
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/krometrail-mcp/src/{config.rs,schema.rs}`
- `crates/krometrail-store/src/{recording.rs,index/frames.rs}`
- focused core/store contract tests

## Design

- Add `RetrieveSourceFrameRequest { scope: EvidenceScope, frame_id: FrameId, max_encoded_bytes: NonZeroU64 }` with a validated constructor and `deny_unknown_fields` deserialization. Reuse the progressive runtime ceiling and `SourceFrameRead` hash/length/media/provenance contract.
- Add `FrameSource::read_source_frame` and implement it in `RecordingStore` with the existing metadata snapshot, bounded encoded read, hash/length check, and final lifetime revalidation. Do not fabricate a `ResolvedRange` from a URI.
- Add `RetrieveSourceFrame` to the existing progressive operation registry as `ResourceOnly`; retain all eight current stable names and add descriptions, capability, mutability, and exposure metadata in the declaration rather than an MCP list.
- Add `BrowserEventDetailRequest` as a validated wrapper over `TemporalContextRequest` that accepts only chronological selection and exposes the existing `range`, `clip`, `filter`, `selection`, and `focus_times` wire shape. Add the one-entry context operation registry for `query_browser_events` and the primary bundle operation definition.
- Delegate custom `JsonSchema` implementations to the same private wire structs used by Serde. Payload-bearing read results need no tool output schema because bytes are projected later; every tool request must have an object-root schema.

## Acceptance evidence

- [ ] Progressive registry and context/bundle definitions are exhaustive, unique, capability-tagged, and contain no duplicate stable-name constants in MCP.
- [ ] Scoped source-frame reads reject invalid scope/frame/limits before storage I/O and return no bytes after an eviction or session-deletion race.
- [ ] Chronological event requests reject compact mode, invalid cursors, out-of-range focus times, and unknown fields.
- [ ] Generated schemas describe the exact validated external shapes, including custom millisecond/UUID/nonzero forms; no MCP-only request mirror is introduced.
- [ ] Existing artifact/progressive/context tests remain green and no compatibility or migration path is added.

## Ordering constraints

This must complete before route construction because route names, exposure, capability membership, request schemas, and resource read requests derive from these contracts. It is the first checkpoint after the two completed temporal domain features.

## Out of scope

No `rmcp` route, URI grammar, resource template, response mapping, inline image, stdio/server change, root wiring, or end-to-end protocol test.

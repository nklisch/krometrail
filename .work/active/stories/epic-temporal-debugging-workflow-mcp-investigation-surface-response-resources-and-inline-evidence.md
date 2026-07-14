---
id: epic-temporal-debugging-workflow-mcp-investigation-surface-response-resources-and-inline-evidence
kind: story
stage: implementing
tags: [agent-ux, visual, storage]
parent: epic-temporal-debugging-workflow-mcp-investigation-surface
depends_on:
  - epic-temporal-debugging-workflow-mcp-investigation-surface-routing-session-and-cancellation
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal MCP Response, Resource, and Inline Evidence Projection

## Checkpoint

Extend the common tool response envelope and build the one resource authority used by temporal results. Convert typed evidence handles into canonical resource links, preserve exact structured provenance without bytes, and optionally return one bounded unchanged primary artifact image or bounded selected source-frame images.

## Likely files

- `crates/krometrail-mcp/src/{response.rs,resources.rs,registry.rs}`
- `crates/krometrail-mcp/src/schema.rs`
- focused response/resource tests

## Design

- Add stable `ResponseResource` metadata to `ToolResponse` and use a tagged screenshot/artifact `ResponseImageMetadata` so existing control screenshots and temporal artifact images share the established envelope without a second response family.
- Parse and build only canonical `krometrail://evidence/{session}/{target}/artifacts/{id}` and `/frames/{id}` URIs. Names and descriptions are fixed typed-ID labels; no user text or storage location enters them.
- Project artifact/source handles into structured metadata and `Content::resource_link` values. Strip only request-scoped bytes from structured JSON; do not duplicate complete result JSON in the summary text.
- Route artifact/frame resource reads through `ProgressiveEvidence` resource-only operations. Return exact original bytes as `BlobResourceContents` with protocol-required base64, exact MIME, and the canonical URI; map eviction/deletion/invalidation to resource-not-found with stable domain error data.
- Choose an inline bundle image by fixed artifact-kind/epoch/generator/ID priority. Enforce 8 MiB bundle and four/4 MiB/16 MiB source-frame presentation bounds; never resize or re-encode.

## Acceptance evidence

- [ ] Bundle and focused results contain no encoded bytes/base64/data URLs/paths/segment addresses in structured content or logs.
- [ ] Every emitted resource link matches its typed scope/ID, metadata, `ResourceLink` content, and strict parser; alternate URI forms are rejected.
- [ ] Retained artifact/frame reads return exact bytes/MIME/hash/length, while eviction, invalidation, deletion, wrong scope, and corruption return no success blob.
- [ ] Primary image selection and source-frame image order are deterministic across cache hits and epochs; over-limit reads degrade honestly while retaining links.
- [ ] Existing control response mapping continues to emit screenshots as image content and temporal responses do not duplicate JSON text.

## Ordering constraints

Depends on final route contracts and cancellation from the routing checkpoint. Server resource methods and root wiring must wait for this projection boundary so protocol content has one implementation.

## Out of scope

No rmcp server capability negotiation, resource-template listing, stdio shutdown, root dependency construction, or integrated workflow fixture.

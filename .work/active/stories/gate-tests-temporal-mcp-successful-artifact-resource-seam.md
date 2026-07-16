---
id: gate-tests-temporal-mcp-successful-artifact-resource-seam
kind: story
stage: done
tags: [testing, agent-ux, visual]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: tests
created: 2026-07-15
updated: 2026-07-15
---

# Test the successful temporal MCP artifact/resource seam

## Priority
High

## Value evidence
Item: `epic-temporal-debugging-workflow-mcp-investigation-surface-response-resources-and-inline-evidence`

The temporal bundle is the primary agent investigation entry point. Current tests cover successful source-frame resources and individual projections, but do not drive a successful bundle containing an artifact through the MCP route and then read its canonical artifact resource URI.

## Gap type
Missing successful cross-layer MCP integration seam.

## Suggested test

Return a successful temporal bundle containing a stored artifact, invoke `temporal_debug_bundle`, follow its canonical artifact URI through resource reading, and assert structured metadata, `ResourceLink`, inline image bytes, MIME type, length, hash, and identity-mismatch rejection. Use the existing fake service/store boundaries; no browser or model is required.

## Test location (suggested)
`crates/krometrail-mcp/src/{server.rs,response.rs,resources.rs}`

## Implementation notes

- Added a browser-free MCP JSON-RPC integration test using a successful fake bundle/progressive evidence seam.
- The test invokes `temporal_debug_bundle`, asserts structured artifact metadata and provenance hash, verifies the inline PNG bytes and MIME, follows the canonical `ResourceLink` through `resources/read`, and verifies exact URI/MIME/blob identity.
- The same route is exercised with a mismatched returned artifact handle and must reject the read with `resource handle identity mismatch`; no browser, model, network, or duplicate implementation path is involved.

## Verification

- `cargo test -p krometrail-mcp --locked` — 23 passed.
- Full locked workspace and `qualification-support` test variants passed.
- Rust 1.85 fmt, check, and Clippy `-D warnings` passed.

Implementation is complete; this standalone story is left at `stage: review` for one bounded independent review.


## Review decision

**Approved.** Independent GPT-5.5 standard bounded review found no material blocker. Rust 1.85 focused, full workspace, and qualification-support gates pass. No re-review was required.

Advisory: the test asserts the emitted `ResourceLink` equals the canonical URI before exercising the real read/mismatch path, but could later extract that URI directly from response content. Existing coverage is sufficient for this release.

---
id: gate-tests-temporal-mcp-successful-artifact-resource-seam
kind: story
stage: implementing
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

---
id: story-download-mime-fidelity
kind: story
stage: done
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Serve downloaded-file resources with their known media type

## Brief

A completed managed download of `hello.txt` (a `data:text/plain` anchor with a `download`
attribute) was exposed through its `krometrail://local/...` resource as
`application/octet-stream`, so the MCP host saved it as an opaque `.bin` blob instead of
surfacing readable text. The download record already carries `suggested_filename`; use the
known media type when the browser reports one, fall back to a bounded extension-based
mapping from the suggested filename, and keep `application/octet-stream` only as the final
fallback. No content sniffing.

## Acceptance

- A completed `.txt`/`.json`/`.png` download's resource read reports the matching media
  type; unknown extensions still report `application/octet-stream`.
- Covered by a managed-download test at the resource boundary.

## Completion notes

Managed download bookkeeping now captures a nonblank browser-reported `mimeType` when available,
normalizes active-document and script-capable values to inert types, and otherwise derives a bounded
media type from `suggested_filename` (`txt`, `json`, `csv`, `md`,
`png`, `jpg`/`jpeg`, `gif`, `webp`, `pdf`, and `zip`). Unknown extensions retain
`application/octet-stream`. HTML extensions map to `text/plain` so an MCP host cannot interpret a
downloaded local resource as active HTML; no content sniffing was added.

- Files changed: `crates/krometrail-cdp/src/session/downloads.rs`,
  `crates/krometrail-mcp/src/server.rs`.
- Tests: bounded media-type precedence/fallback coverage, completed-download read coverage, and
  the MCP managed-download resource boundary assertion.
- Verification: focused CDP and MCP tests passed; full workspace gates are recorded after the
  three story commits.
- Stage intentionally remains `implementing` per the implementation request; no other work item
  was advanced.

## Review-fix note (2026-07-19)

Browser-reported active-document and script MIME values now pass through inert download
normalization before precedence is applied; HTML is served as `text/plain` and SVG as
`application/octet-stream`, with direct resource-boundary regression coverage.

---
id: story-download-mime-fidelity
kind: story
stage: implementing
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

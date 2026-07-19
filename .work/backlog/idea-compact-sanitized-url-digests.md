---
id: idea-compact-sanitized-url-digests
created: 2026-07-18
updated: 2026-07-18
tags: [browser]
---

Concise frame and page-asset inventories serialize each sanitized URL `path_sha256` as a 32-element integer array. This dominates otherwise compact records and scales poorly toward the 256-asset cap. Keep the privacy-safe path identity, but use a compact deterministic string encoding or omit it from concise presentation while preserving deliberate drill-down detail.

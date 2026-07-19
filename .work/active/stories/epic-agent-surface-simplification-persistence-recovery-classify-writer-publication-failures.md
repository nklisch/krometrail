---
id: epic-agent-surface-simplification-persistence-recovery-classify-writer-publication-failures
kind: story
stage: implementing
tags: [storage, diagnostics]
parent: epic-agent-surface-simplification-persistence-recovery
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Classify segment publication failures and preserve safe writer reuse

Introduce the bounded persistence operation/category/recoverability contract in core and apply it at every segment-writer failure site. Only directory sync after a completed sealed-file rename is `writer_usable`; every ambiguous write, file sync, initial publication, or rename failure remains terminal.

## Acceptance evidence

- Inject a post-rename directory-sync failure and assert its exact operation, category, and `writer_usable` classification.
- Append and flush a later frame through the same writer; verify the already sealed segment and new segment are both readable with distinct frame identities and valid offsets.
- Inject representative pre-rename failures and prove the exact first terminal error is replayed without later filesystem mutation.
- Prove serialized/debug failures contain no paths or raw OS messages.

## Ordering

This checkpoint establishes the only persistence classification authority. Capture propagation depends on it.

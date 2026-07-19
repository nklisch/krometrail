---
id: epic-agent-surface-simplification-current-contract-current-store-schema
kind: story
stage: implementing
tags: [storage, infra]
parent: epic-agent-surface-simplification-current-contract
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Replace historical migrations with one current schema

Create one transactional current-v6 schema initializer and exact version validator. Open populated v6 data without writes; reject unversioned non-empty, older, or newer formats before mutation with clear recovery. Prove the consolidated schema retains every current table, column, index, trigger, foreign key, and strict constraint before deleting migration modules and migration-only tests.

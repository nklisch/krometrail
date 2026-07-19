---
id: story-temporal-listing-and-defaults
kind: story
stage: implementing
tags: [browser]
parent: feature-temporal-range-artifact-economy
depends_on: [story-temporal-resolve-range]
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Listing pagination, generator defaults, timing description

Unit 3 of the parent design: resolved_order listing gains offset paging with explicit omission and next_offset; direct generator knobs become optional with serde defaults shared with bundle policy (single source); SessionTime schema description states session-relative monotonic nanoseconds.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.

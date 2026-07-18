---
id: epic-temporal-video-artifacts-retained-generation
kind: feature
stage: drafting
tags: [visual, storage, security]
parent: epic-temporal-video-artifacts
depends_on: [epic-temporal-video-artifacts-clip-contracts]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Retained temporal video generation

## Brief

Build the bounded application service that reads an exact resolved range, partitions compatible visual epochs, creates the canonical presentation plan, adapts source frames and explicit gap slates, invokes an injected encoder, and publishes the resulting MP4 plus typed manifest. It must include encoder identity in cache validation, preserve cancellation/deletion races, reject partial or contradictory output, and make the retained clip and provenance readable through the existing evidence authority.

Generalize the image-only artifact persistence boundary additively so existing PNG artifacts and retained database rows remain readable. This feature owns no FFmpeg discovery, concrete process execution, MCP tool registration, host upload, or agent-facing setup prose.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: application/storage consumer of `epic-temporal-video-artifacts-clip-contracts`; uses a fake encoder port and can proceed in parallel with the production FFmpeg runtime

## Simplification opportunity

- Extend the current artifact publication, cache, SQLite index, retention, recovery, deletion, and resource-read authority for another validated media/manifest variant instead of creating a video database, storage root, URI grammar, or cleanup subsystem.

## Foundation references

- `docs/SPEC.md` — Disk Budget and Retention, Temporal Ranges, Artifact Provenance, and Local Data
- `docs/ARCHITECTURE.md` — Recording Store, Temporal Range Resolution, Artifact Generation, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Input Sequence, Temporal Video Clip, Capture Gaps, and Provenance
- `docs/EVALUATION.md` — Storage and Retention Evaluation and Temporal video evaluation

## Parent decisions inherited

- Video consumes authoritative retained source frames and never becomes a second capture path.
- One canonical plan supplies both encoder input and manifest timing, including visible gap slates and held-frame disclosure.
- Existing image artifact compatibility is a stable 1.x boundary; the storage change is additive.
- Encoded bytes are reusable only under the exact adapter/build/encoder identity.


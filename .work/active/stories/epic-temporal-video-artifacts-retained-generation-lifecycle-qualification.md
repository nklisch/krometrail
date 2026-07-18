---
id: epic-temporal-video-artifacts-retained-generation-lifecycle-qualification
kind: story
stage: implementing
tags: [visual, storage, security, testing]
parent: epic-temporal-video-artifacts-retained-generation
depends_on: [epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence, epic-temporal-video-artifacts-retained-generation-bounded-generation-service]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Retained-video lifecycle and race qualification

## Design checkpoint

Qualify the integrated fake-encoder service against the real recording store, concentrating on stable image migration, exact cache reuse/corruption, publication cancellation, deletion while source or encoding work is paused, source and budget eviction, recovery, and ingestion independence. This checkpoint supplies the cross-component acceptance evidence required before feature review; it does not substitute fake bytes for the separate live FFmpeg qualification.

## Acceptance evidence

- Session deletion completes while fake encoding is paused; releasing late work cannot create a row, file, resource handle, or usage entry.
- Active video generation does not block frame, gap, or browser-event persistence, and cancellation/store failure leaves no partial published state.
- Reopen/recovery retains only fully validated video, removes corrupt/staged/orphan files idempotently, and preserves existing image artifacts across schema v6.
- Small-budget tests prove video follows the same regenerate-first eviction and source-link invalidation semantics as still artifacts.

## Ordering constraints

- Depends on both additive persistence and the bounded generation service.
- Child completion requires locked deterministic tests only; real executable/container qualification remains owned by `epic-temporal-video-artifacts-ffmpeg-runtime` and the later agent-surface integration.

## Execution contract

- Worker capability: highest available, selected by active autopilot because cancellation/deletion races and retained evidence integrity are high consequence.
- Review weight: `standard` from autopilot default; this child closes on green evidence and the integrated feature receives the single independent review pass.

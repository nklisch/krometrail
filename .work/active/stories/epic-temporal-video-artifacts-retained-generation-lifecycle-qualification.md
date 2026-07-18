---
id: epic-temporal-video-artifacts-retained-generation-lifecycle-qualification
kind: story
stage: done
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

## Implementation notes

- Implemented by the highest-available Sol worker at `xhigh` reasoning; the integrated feature retains the orchestrator's `standard` review weight.
- Qualified the fake-encoder service against a real `SqliteIndex`/`RecordingStore`: an exact repeat hits the retained cache, corrupt MP4 bytes invalidate and regenerate, and the returned cache disposition/encoder count agree with the persisted lifecycle.
- Proved the deletion/publication race with a paused fake encode. Session deletion completes without waiting for external work; after late encode release the shared deleted-session fence rejects publication and leaves no video row, source frame, artifact bytes, ready/staged/temp file, or resource result.
- Proved a paused encode does not block frame, capture-gap, or schema-v5 browser-event ingestion through the same recording store.
- Expanded real-store video lifecycle evidence to cover cancellation before publication, valid restart/read, corrupt-byte invalidation without source loss, durable staged-video finalization, orphan MP4 cleanup over repeated restarts, regenerate-first budget eviction, source-segment eviction of linked video, and session deletion.
- Tests use constructor-valid fake MP4 bytes and local PNG source fixtures only. No FFmpeg executable, browser, network, process, provider, or live-adapter qualification is involved.
- Verification: `cargo fmt --all -- --check`; `cargo test -p krometrail-store --test video_artifact_store --locked` (7 passed); `cargo test -p krometrail-store --test sqlite_schema --locked` (3 passed); `cargo test --bin krometrail --locked` (119 passed, 2 intentionally ignored manual qualification/benchmark tests); `cargo check --workspace --all-targets --locked`; focused store/root Clippy with `-D warnings`; `git diff --check`.
- Simplification pass: lifecycle coverage exercises the existing recording/artifact authorities directly; no test-only video store, deletion lease, recovery path, or alternate usage accounting was introduced.
- Discrepancies: none in retained-generation scope. A concurrent full-workspace run collided in the unrelated CDP reusable-profile lease test; that test passes alone and the integrated owner will rerun it after parallel feature workers settle. Parked work: none; live codec qualification remains with the separately tracked FFmpeg feature as designed.

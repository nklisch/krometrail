---
id: epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation
kind: story
stage: done
tags: [infra, security, testing]
parent: epic-temporal-video-artifacts-ffmpeg-runtime
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Owned FFmpeg process and MP4 proof boundary

## Design checkpoint

Create the unpublished adapter crate's fixed `libx264` v1 argument policy, private generated request workspace, cancellation-safe direct Tokio process ownership, bounded diagnostic/output handling, and checked MP4/H.264/no-audio validator. This checkpoint proves the security-sensitive execution seam without discovering a user's executable or implementing the core encoder trait.

## Acceptance evidence

- Exact policy/job tests prove that only generated relative names and validated numeric geometry, duration, and byte values reach argv or FFconcat input.
- Cross-platform managed-process tests prove deadline, cancellation, dropped-future, exit, pipe/output overflow, direct-child/descendant cleanup, and temporary workspace removal.
- Checked parser fixtures prove one matching-dimension H.264 video track without audio and reject malformed boxes, wrong codecs/dimensions, truncation, nesting overflow, and missing media.
- Tracing/error assertions prove no raw stderr, payload, executable path, temporary path, or source content crosses the adapter boundary.

## Ordering constraints

- Root checkpoint for this feature.
- Discovery qualification and the port adapter must reuse this exact runner, policy, and validator rather than creating probe-only or request-only process paths.

## Implementation notes

- Execution capability: GPT-5.6 Sol xhigh, the caller-selected fallback because Luna was unavailable and this checkpoint owns security-sensitive external-process lifecycle and untrusted container parsing.
- Review weight: `standard` from the autopilot caller; child checkpoint review is not applicable, and the integrated feature stops at `review` for one later independent pass.
- Files changed: root `Cargo.toml`/`Cargo.lock`; new `crates/krometrail-ffmpeg` manifest, fixed argument policy, private staging job, managed process guard, bounded MP4 validator, compiled test fixture support, and retained video fixture/provenance.
- Tests added: exact allowlisted argv; cumulative endpoint/FFconcat staging and private cleanup; retained real MP4 plus malformed codec/audio/dimension/duration/box mutations; compiled-process cancellation, deadline, diagnostic/output overflow, future-drop, descendant cleanup, and platform-owned tree enforcement. These protect command injection, process cleanup, and untrusted output validation rather than implementation line coverage.
- Simplification: one crate-private policy, staging job, process guard, and validator; no wrapper, shell, downloader, native codec binding, network dependency, generic process framework, or alternate probe path.
- Discrepancies from design: none. Windows Job Object code is compiled on Windows CI rather than exercised on this macOS checkpoint; the same portable compiled fixture test module owns both platform paths.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-ffmpeg --lib` passed 15 tests on macOS; the retained fixture was independently inspected as one silent H.264 stream at 2x2, 350 ms, and 1 MHz timebase.

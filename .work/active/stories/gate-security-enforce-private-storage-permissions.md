---
id: gate-security-enforce-private-storage-permissions
kind: story
stage: done
tags: [security, storage, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: security
created: 2026-07-15
updated: 2026-07-15
---

# Enforce private permissions for local evidence and managed profiles

## Severity
Medium

## Domain
Local evidence privacy and profile protection

## Location
`crates/krometrail-store/src/index/mod.rs:60`

## Evidence

Recording/index/segment and managed-profile roots are created with platform defaults. On common Unix umasks this can leave directories traversable and evidence files readable by other local users even though they may contain screenshots, browser events, and profile data.

## Remediation direction

Apply owner-only directory and file permissions on supported Unix platforms at the creation boundary, validate existing managed roots, and preserve explicit operator-owned custom paths. Define a proportional cross-platform policy without introducing remote encryption, authentication, or disabling local evidence storage.

## Implementation evidence

- Krometrail-owned Unix directories use mode `0700`; index, segment, artifact, lock, temporary, and managed-profile files use mode `0600` at creation/open boundaries.
- Existing managed profile trees are tightened without following symlinks, stale endpoint files are removed only after exclusive profile ownership, and Chrome inherits `umask(077)` so files created during launch remain private.
- Existing index/segment/artifact roots are protected proportionally while explicit local storage paths remain supported. Non-Unix builds compile with an explicit no-op for Unix mode enforcement and retain native ACL/locking responsibility; no encryption, authentication, or storage removal was introduced.
- Focused Unix regression tests verify index/segment/artifact modes and managed-profile root/file/lock modes.

## Verification

- `cargo test -p krometrail-store --lib --locked` -> 39 passed
- `cargo test -p krometrail-cdp launcher::profile::tests --locked` -> 4 passed
- `rustup run 1.85.0 cargo check --workspace --all-targets --locked` -> passed
- `rustup run 1.85.0 cargo clippy --workspace --all-targets --locked -- -D warnings` -> passed

## Review remediation (2026-07-15)

The accepted standard-review blocker is fixed narrowly at the segment recovery boundary. Discovery now uses the non-following `DirEntry::file_type` check and skips symlinks, directories, devices, sockets, and other non-regular candidates before permission, read, or repair work. The existing-file permission helper now uses `symlink_metadata` on every platform and refuses anything that is not a regular file before Unix `0600` tightening; legitimate regular segment recovery and the existing `0700`/`0600` policy remain unchanged.

The Unix recovery regression creates both `<uuid>.open` and `<uuid>.kts` symlinks to valid header-only sentinel files outside the managed segment directory. It proves recovery returns no indexed segments or frames, leaves both symlinks in place, and preserves each sentinel's mode and bytes. The non-regular directory candidate is also explicitly skipped.

Fix verification passed `rustup run 1.85.0 cargo test -p krometrail-store --test recovery --locked` (14 passed), `rustup run 1.85.0 cargo test -p krometrail-store --lib --locked` (39 passed), Rust 1.85 formatting, full workspace check, full workspace tests, and full workspace Clippy with warnings denied. No Chrome, network, push, or unrelated finding was involved. Per standard policy, no re-review was run.

## Review decision

**Approved after remediation.** The accepted symlink/non-regular recovery blocker is resolved and verified. The story advances from `review` to `done` without a repeat review.

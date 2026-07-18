---
id: story-fix-profile-inventory-canonical-path-test
kind: story
stage: done
tags: [bug, testing]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Compare temporary profile paths in one canonical path domain

## Symptom

`cargo test -p krometrail-cdp launcher::profile::tests::inventory_is_sorted_private_and_excludes_temporary_and_symlink_entries --locked -- --exact` fails on macOS because the returned temporary profile path begins with `/private/var/...` while the test root begins with `/var/...`.

## Root cause

`ProfileLease::acquire` correctly canonicalizes its root before constructing the temporary profile, but the assertion compares that canonical path against the test helper's non-canonical `std::env::temp_dir()` spelling. On macOS those spellings traverse the `/var` symlink and are textually different despite naming the same directory.

## Fix approach

Canonicalize the test root before comparing path containment. Production path handling remains unchanged.

## Regression test

The existing inventory test remains the regression and must pass when the platform temp directory contains a symlinked path prefix.

## Implementation notes

- **Execution capability**: focused local repair; the failure was deterministic, isolated to one test assertion, and required no production behavior change.
- **Files changed**: `crates/krometrail-cdp/src/launcher/profile.rs` and this story.
- **Confirmation**: the exact failing test now passes, the original macOS path-spelling symptom is gone, and all 160 `krometrail-cdp` library tests pass.
- **Inline review**: approved. The assertion now compares the production canonical path against the canonical temporary root, preserving the test's containment intent without weakening it or changing profile ownership behavior.
- **Adjacent issues**: the separately observed MCP diagnostics test remains under investigation and is not bundled here.

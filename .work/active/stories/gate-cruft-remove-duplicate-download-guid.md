---
id: gate-cruft-remove-duplicate-download-guid
kind: story
stage: done
tags: [cleanup]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: cruft
created: 2026-07-18
updated: 2026-07-18
---

# Remove download GUID duplicated in map entries

## Confidence
Medium

## Category
redundant state

## Location
`crates/krometrail-cdp/src/session/downloads.rs:181`

## Evidence

The GUID is both the `BTreeMap<String, Entry>` key and an identical `Entry.guid` field.

## Removal

Delete `Entry.guid` and use the map key when reconnecting, shutting down, cancelling, and reading a download.

## Acceptance

- `Entry` stores only public download state and verified size; the `BTreeMap` key is the sole GUID authority.
- Reconnect, cancellation, reads, and shutdown preserve their existing GUID command/path behavior by iterating or locating map keys.
- Focused download lifecycle and filesystem tests remain green with no public contract change.

## Tests

Run `cargo test -p krometrail-cdp session::downloads --locked` and CDP clippy with warnings denied.

## Implementation and review

Removed the redundant `Entry.guid`; all cancellation, read, reconnect, and shutdown paths now derive the opaque GUID exclusively from the `BTreeMap` key. Focused download tests pass 10/10 and CDP all-target clippy passes with warnings denied. Bounded inline review found no behavior or privacy regression. Verdict: pass.

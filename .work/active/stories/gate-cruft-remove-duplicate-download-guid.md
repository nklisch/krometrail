---
id: gate-cruft-remove-duplicate-download-guid
kind: story
stage: drafting
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

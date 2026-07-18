---
id: story-fix-lazy-managed-download-activation
kind: story
stage: review
tags: [bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Keep managed downloads opt-in until first list

## Symptom

Starting any managed or reusable-profile browser eagerly subscribes to download events, creates a private download directory, and sends `Browser.setDownloadBehavior` even when the agent never requests download control. Optional download setup failure can therefore prevent the browser session from reaching ready state and changes Chrome's ordinary download behavior without explicit expansion.

## Root cause

The production session composition root calls `ManagedDownloadAuthority::configure` while assembling every managed session. The authority combines allocation with activation, so browser startup cannot retain the compact browser-default path or isolate optional local-I/O failures.

## Fix approach

Introduce a lazy session-owned download boundary. Managed startup allocates only inert configuration; the first cursor-producing `list_downloads` call subscribes before enabling `allowAndName`, retains the activated authority for later tools, and reports activation failures only to the explicit local-I/O call. Reconnect restores download behavior only when the authority was activated.

## Regression test

`crates/krometrail-cdp/src/session/downloads.rs` scripted tests assert no command or directory before opt-in, first-list subscription-before-command ordering, isolated activation failure, one shared activation, and reconnect no-op before activation.

## Implementation notes

- Execution capability: focused local implementation; the defect is confined to the session-owned download authority and its dispatch/reconnect seams, so no independent decomposition was needed.
- Files changed: `crates/krometrail-cdp/src/session/downloads.rs`, `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/operations.rs`, and `crates/krometrail-cdp/src/session/reconnect.rs`.
- Regression coverage: ten scripted download tests pass, including inert managed/named-profile defaults, first-list subscribe-before-enable ordering, retryable activation failure, single shared activation, inert reconnect, and reconnect failure isolation.
- Confirmation: targeted downloads 10/10; CDP all-target clippy with `-D warnings`; workspace all-target check; MCP focused suite reached 60/61 with one unrelated warning-log test that passes alone. The full CDP run reached 158/159 with the unrelated reusable-profile inventory assertion failing reproducibly alone.
- Adjacent issue for release coordination: `launcher::profile::tests::inventory_is_sorted_private_and_excludes_temporary_and_symlink_entries` currently fails because its temporary path is outside the asserted `root/tmp`; the aggregate owner is tracking this separately.

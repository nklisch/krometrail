---
id: gate-security-eliminate-cdp-port-allocation-race
kind: story
stage: done
tags: [security, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: security
created: 2026-07-15
updated: 2026-07-15
---

# Eliminate the managed Chrome debugging-port allocation race

## Severity
Medium

## Domain
Browser endpoint and launch trust

## Location
`crates/krometrail-cdp/src/launcher/startup.rs:327`

## Evidence

The launcher binds `127.0.0.1:0`, reads the selected port, drops the listener, then starts Chrome with `--remote-debugging-port=<port>` and probes that now-unowned port. Another local process can claim the port between allocation and Chrome binding.

## Remediation direction

Use Chrome's child-owned ephemeral debugging endpoint (`--remote-debugging-port=0`) and resolve the resulting profile-scoped `DevToolsActivePort` record, validating that the endpoint belongs to the launched managed process/profile before accepting it. Preserve local launch, loopback-only control, startup timeout, and cleanup behavior.

## Implementation evidence

- `SystemChromeLauncher` no longer binds and drops a candidate listener. Chrome receives `--remote-debugging-port=0` with the existing loopback address and managed profile.
- Startup reads and strictly parses the profile-scoped `DevToolsActivePort`, constructs `http://127.0.0.1:<child-port>`, probes the endpoint while the managed process is alive, and requires the discovered browser WebSocket path to match the file's `/devtools/browser/...` value.
- Startup retains its configured deadline and process cleanup. Polling yields to the runtime rather than adding a fixed sleep; stale active-port files are removed only after profile ownership is acquired and on failed launch cleanup. Profile drop also removes the handoff file.
- Regression coverage includes valid/malformed endpoint-file partitions and the terminated-child failure path without starting Chrome.

## Verification

- `cargo test -p krometrail-cdp launcher::startup::tests --locked` -> 4 passed
- `cargo test -p krometrail-cdp launcher::profile::tests --locked` -> 4 passed
- No real Chrome was required.

## Review decision

**Approved directly.** The managed child-owned debugging endpoint, profile-scoped handoff validation, process cleanup, and focused startup/profile tests satisfy the security finding. No re-review was run; the story advances from `review` to `done`. No Chrome, network, push, or unrelated finding was added.

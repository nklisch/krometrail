---
id: gate-security-eliminate-cdp-port-allocation-race
kind: story
stage: implementing
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

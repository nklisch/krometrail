---
id: idea-profile-lock-fallback-or-recovery
kind: feature
stage: backlog
created: 2026-08-15
updated: 2026-09-05
tags: [browser, agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
---

# Managed Profile Lock Auto-Fallback & Stale Lock Recovery

## The Finding
Calling `start_browser` without arguments defaults to profile `managed: "default"`. If a previous debug session, background process, or concurrent subagent is holding or was killed without releasing the profile directory lock, the tool fails immediately with:
`start_browser failed [profile_in_use]: managed browser profile is already in use.`

For AI agents running in sandboxes or orchestrators, this hard error stops execution unless the agent explicitly knows to formulate `profile: "temporary"`.

## Original proposed directions (not settled design)

The initial proposal suggested a temporary-profile fallback and stale-lock detection using a recorded PID. The review below establishes that the current lock is a create-new sentinel, not a verified PID-bearing lease. Do not implement the initial PID assumption or silently change the caller's selected profile.

## Review evidence and priority — 2026-09-05

- **Priority:** P1, wave 1 of [A-grade operational reliability](epic-a-grade-reliability.md). Reuse this item instead of creating a duplicate profile-recovery ticket; its existing ID is intentionally preserved despite the normal child-prefix convention.
- **Evidence status:** Reproduced through `ProfileLease` at `eb5b4656`. A child acquired a reusable profile and called `process::exit(0)` without destructors. After that owner exited, a second acquisition returned `profile is already in use`. This isolated test did not launch Chrome or prove a surviving browser safe to displace.
- **Source:** `crates/krometrail-cdp/src/launcher/profile.rs:203` uses `create_new(true)`; release depends on deleting `.krometrail.lock`. Profile listing similarly reports use from sentinel existence.
- **Readiness:** Backlog scope, not an approved implementation design. Automatic temporary fallback remains a choice to evaluate, not a required default.

## Acceptance criteria

- [ ] A reusable profile can be reacquired after its owner exits abruptly when no live browser/process still owns the profile. Cover process exit without cleanup and forced termination on supported platforms.
- [ ] Concurrent live owners cannot use the same profile. A Chrome process surviving the Krometrail owner is not displaced or treated as dead solely because the agent process ended.
- [ ] Lock state reflects actual ownership rather than sentinel existence. Account for stale sentinels, PID reuse if PIDs are used, permissions/platform capabilities, and cleanup/reacquisition races.
- [ ] Provide actionable recovery or an explicitly disclosed isolated-profile fallback. An explicitly requested reusable identity is never silently replaced; users must know when cookies/authentication/session state differ.
- [ ] Clean release, failed launch, cancellation, and shutdown remain idempotent and preserve reusable profile data. Temporary fallback cleanup is verified separately.
- [ ] Use deterministic/fault-injection tests plus a live browser ownership regression. Do not accept a test that permits either refusal or success on the same specified platform/condition.

## Implementation boundaries

Prefer process-lifetime locking with an explicit browser-ownership and recovery contract; evaluate the precise mechanism during design. Name the real threat: concurrent writers corrupting or unintentionally sharing a profile. Neither blindly removing a file nor refusing forever after a dead owner adequately protects against it. Preserve a degraded/recovery path when a platform capability is genuinely unavailable; do not introduce filesystem-type allowlists.

Original dogfooding observations above are retained. Final agent-journey qualification lives in `epic-a-grade-reliability-agent-journey-qualification` and consumes this item's recovery behavior.

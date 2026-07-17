---
id: epic-agent-browser-reliability-managed-session-lifecycle-cold-discovery
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-managed-session-lifecycle
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Discover cold standard browser installations

## Checkpoint

Make the existing standard macOS Chrome candidate reliable on a cold first probe while preserving bounded, discovery-only behavior and privacy-safe diagnostics.

## Exact implementation

Add a private injectable `VersionProbePolicy` and `VersionProbeOutcome` in `crates/krometrail-cdp/src/launcher/discovery.rs`; preserve the public discovery signatures. Apply a cold-start-capable production timeout to explicit, environment, and platform-default candidates, keep PATH-only probes short, deduplicate canonical paths before probing, retain the 4096-byte output cap, and emit source-class/ordinal/outcome/elapsed diagnostics without executable paths. Keep `LaunchError::BrowserNotFound` stable and make its recovery describe checked source classes.

## Acceptance evidence

- [ ] A delayed platform-default version fixture succeeds inside the cold budget while the same delay times out under the ordinary injected budget.
- [ ] One canonical executable reachable through several sources is probed once at highest precedence.
- [ ] Hung/noisy candidates are killed within the injected deadline and cannot leak stdout or paths into public errors/log events.
- [ ] Standard macOS Chrome remains the first platform default and `doctor` does not launch a controlled session.

## Ordering and boundary

This checkpoint is graph-independent from shutdown and activation. It consumes the parent feature's durable-diagnostics dependency for candidate events.

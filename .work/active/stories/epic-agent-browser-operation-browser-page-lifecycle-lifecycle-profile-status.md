---
id: epic-agent-browser-operation-browser-page-lifecycle-lifecycle-profile-status
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-browser-page-lifecycle
depends_on: [epic-agent-browser-operation-browser-page-lifecycle-core-control-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Complete managed lifecycle, profile, and status semantics

## Checkpoint

Implement Unit 2 of the parent design through the existing `ProductionBrowserConnector`, `ProductionSession`, launcher/profile guards, local endpoint resolver, and capability probe.

Make no-options launch use reusable managed profile `default`; preserve explicit named reusable and temporary selection; report managed profile identity plus reusable/temporary persistence without exposing paths. Root composition must place reusable profiles under Krometrail's local data directory (subject to the explicit profile-root override). Preserve process-before-profile cleanup, reusable retention, temporary removal, attached ownership, idempotent stop, bounded flush/shutdown, and external-browser survival.

Implement coherent `ProductionSession::status` from one supervisor revision plus immutable session metadata/capture status. Keep launch/attach on the same setup and compatibility path. Electron renderer support remains capability-probed attached support; reject Node inspectors and incapable endpoints without adding an Electron-specific controller.

## Required files

- `crates/krometrail-cdp/src/launcher/profile.rs`
- `crates/krometrail-cdp/src/launcher/startup.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/compatibility.rs`
- `src/app.rs`
- existing connector/session/profile/process/compatibility tests and port fakes

## Acceptance evidence

- [ ] Default and named reusable profiles retain state and exclusivity; temporary profile cleanup happens only after owned process termination; attach acquires neither.
- [ ] Managed stop closes and cleans up; attached stop detaches and leaves the external Chrome/Electron process alive; repeated stop is stable.
- [ ] One status snapshot reports state, ownership, profile kind/identity, compatibility, selection, pages, and capture state without sensitive adapter identities.
- [ ] Capable Electron renderer attachment uses the ordinary path; Node inspector and missing-capability endpoints fail before session return.
- [ ] Root assembly reuses one connector, process clock, ID source, and capture assembly and introduces no lifecycle facade or session singleton.

## Ordering

Depends on the core contract checkpoint. It establishes trustworthy start/attach/stop/status/profile behavior before page-selection mutations consume that status surface.

## Implementation notes

- Execution capability: highest; profile ownership, renderer acceptance, shutdown, and coherent operational status are lifecycle safety boundaries.
- Review weight: standard (caller).
- Files changed: managed profile lease projection, production connector/session, root composition, and status-consuming connector/capture/supervision tests.
- Tests: default/named/temporary/external profile contracts, lease exclusivity/retention/cleanup, capability probes including Node-inspector rejection, attached/managed stop behavior, and coherent status. Focused suites passed with 19 tests; root all-target check passed.
- Simplification: removed split session component getters from the port and migrated consumers to one coherent status snapshot; reused the one connector, launcher, compatibility path, and capture assembly.
- Discrepancies from design: none.
- Adjacent issues parked: none.

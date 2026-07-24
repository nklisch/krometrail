---
id: story-activation-version-transparency
kind: story
stage: implementing
tags: [distribution, mcp]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Activation and serving-version transparency

## Brief

Two findings from the 2026-07-23 v1.6.1 activation sequence:

1. **No version self-reporting on the MCP surface.** After a plugin reload
   failed to swap the MCP server, the session silently kept serving the old
   1.6.0 binary; the only way to discover this was inspecting the OS process
   list. Nothing in `browser_status` (or any tool result) states the serving
   binary version, so "am I on the version I just installed?" is unanswerable
   in-band.
2. **First-activation installer wording reads as an error.** The launcher's
   verify step (`plugin/scripts/install-managed.sh:83`) fails with "managed
   release directory is unavailable or unsafe" when the version directory
   simply does not exist yet — the ordinary first activation of a new release.
   The launcher then correctly reinstalls, but the logged reason
   ("reinstalling because: … unavailable or unsafe") misreports the normal
   path as a safety problem, which cost real diagnosis time during the
   shakedown.

## Direction

- Add the serving binary version to `browser_status`'s result (e.g. a
  `server_version` field carrying the crate version) so any session can
  confirm what is serving in-band. Registry-declared schema change →
  regenerate; wire checks green. Keep it one field, not a new block, unless
  the existing shape argues otherwise.
- Installer: distinguish "not yet staged" (missing version directory /
  missing binary) from genuinely unsafe states (symlink, wrong owner, wrong
  permissions, identity mismatch). The not-yet-staged verify outcome reports a
  neutral reason such as "managed release v<X> is not staged yet"; unsafe
  states keep the strict failure wording. Launcher log line follows.
- Hermetic distribution tests (release-boundary fixtures) cover both verify
  outcomes; no network or user-home mutation in tests.

## Acceptance criteria

- [ ] `browser_status` reports the serving binary version; schema regenerated;
      wire checks green.
- [ ] First activation of an unstaged version logs the neutral not-yet-staged
      reason; symlinked/wrong-owner destinations still fail with the safety
      wording; both pinned by hermetic tests.
- [ ] Full workspace gate green.

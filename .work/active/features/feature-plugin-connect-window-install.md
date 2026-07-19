---
id: feature-plugin-connect-window-install
kind: feature
stage: drafting
tags: [bug, infra, distribution]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Keep managed install out of the host connect window

## Brief

The plugin launcher performs the managed release download/verify synchronously
inside the host's MCP connect window. On the first session after a plugin
update (or any `verify-existing` failure), the ~25MB release download plus
checksum verification can exceed Claude Code's 30s connect timeout, so the
session marks the server failed (red X) and kills it even though the install
completes and the server starts successfully seconds later.

Observed 2026-07-19: managed v1.2.2 binary was installed at 06:53:46Z by a
healthcheck, yet a session start at ~06:59Z re-ran a full install (binary file
recreated 06:59:44.8Z, server up 06:59:46.7Z per diagnostics) and the session
showed the red X; the server process was later dead. `verify-existing` passed
when re-run manually minutes later, so the mid-session verify failure that
forced the re-download is unexplained and worth root-causing alongside the
latency fix.

Scoping lead: `plugin/bin/krometrail` runs `managed_binary_is_current` with
`2>/dev/null`, so whatever reason `install-managed.sh verify-existing` fails
with is discarded — the re-install trigger is currently unobservable. Any fix
must first make that reason visible (stderr diagnostics, bounded and
content-free per the project privacy rules). Fix directions from the capture:
move installation out of the connect path, or make the launcher emit
progress/fail fast so hosts don't time out silently; also guard concurrent
launcher invocations (healthcheck + session start racing the same managed
root) so one install wins and the other waits or fast-fails.

## Simplification opportunity

None identified; the launcher and installer are already minimal POSIX sh. The
exact-release-managed-activation and hermetic-release-boundary-fixtures
patterns bind: no `latest` polling, no standalone-install mutation, tests
shadow curl and release assets.

Origin: `.work/backlog/idea-managed-install-inside-connect-window.md`.

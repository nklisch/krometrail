---
id: feature-plugin-connect-window-install
kind: feature
stage: review
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

## Architectural choice

Three-part hardening of the POSIX-sh launcher/installer pair, keeping the
exact-release activation contract intact: (1) make the re-install trigger
observable (today `verify-existing`'s failure reason is discarded via
`2>/dev/null`, so the root cause of the observed spurious reinstall is
unknowable), (2) single-flight installs across concurrent launcher invocations
(session start + host healthcheck racing one managed root), and (3) bound the
connect-window install with a fast, explicit failure instead of a silent 30s
hang that gets the process killed mid-install. There is no plugin healthcheck
hook of our own — every launcher invocation runs this path — so the launcher
itself is the only place to fix this. Alternatives rejected: pre-installing
via a new plugin hook (host-specific, doesn't exist in the codex plugin
variant); exec-ing a previously installed different version while updating
(violates exact-release-managed-activation).

## Design decisions
- **Root-cause posture**: the 2026-07-19 spurious verify failure is
  unexplained; candidate causes (managed-root env divergence between
  invocation contexts, transient `--version` exec failure) cannot be
  distinguished retroactively. Unit 1's diagnostics make the next occurrence
  attributable; no speculative behavioral fix for the unknown cause.
- **Install budget**: launcher waits up to 20s for an in-flight install, then
  exits 1 with a one-line actionable stderr message while the installer child
  keeps running; the next connect attempt finds it finished (or waits on the
  lock). Chosen over blocking indefinitely because the host kills at ~30s
  anyway — better a clear fast failure plus surviving background install than
  a killed one.
- **Lock mechanism**: `mkdir` lock directory under the managed root (portable
  POSIX), containing the owner pid; stale if the pid is dead. Loser polls at
  1s until winner finishes or budget expires.

## Implementation Units

### Unit 1: Observable verify and launch diagnostics
**File**: `plugin/bin/krometrail`

- Log one stderr line at start: expected version + resolved managed root
  (which env source won: KROMETRAIL_MANAGED_ROOT / PLUGIN_DATA /
  CLAUDE_PLUGIN_DATA / XDG / HOME).
- `managed_binary_is_current` captures the installer's stderr; on verify
  failure, forward the reason to launcher stderr prefixed
  `krometrail plugin: reinstalling because:` before running the install.

**Acceptance Criteria**:
- [x] Fixture run with a corrupted managed binary shows the verify failure
      reason on stderr before reinstall.
- [x] Fresh-install fixture still succeeds with the diagnostic present.

### Unit 2: Single-flight install lock
**Files**: `plugin/bin/krometrail`, `plugin/scripts/install-managed.sh`

Lock dir `"$MANAGED_ROOT/.install-lock"` acquired via `mkdir` in the launcher
around the install call; pid written inside; stale-lock reclaim when the
recorded pid is not alive (`kill -0`). While locked by a live peer, poll
(sleep 1) until the lock clears, then re-run `verify-existing` and exec on
success. Budget shared with Unit 3.

**Acceptance Criteria**:
- [x] Two concurrent fixture launchers: exactly one runs the installer; both
      exec the same verified binary.
- [x] Stale lock (dead pid) is reclaimed and install proceeds.

### Unit 3: Bounded connect-window install
**File**: `plugin/bin/krometrail`

Run the installer as a background child; poll for completion up to the 20s
budget; on completion verify + exec as today; on budget expiry print
`krometrail plugin: managed release install is still running in the
background; reconnect to retry` to stderr and exit 1 without killing the
child.

**Acceptance Criteria**:
- [x] Fixture with an artificially slow (shadowed-curl sleep) install: launcher
      exits 1 within budget with the actionable message; the installer child
      completes; a second launcher invocation execs without reinstalling.
- [x] Fast-install fixture behaves exactly as today (single invocation execs).

## Implementation Order
1. Unit 1
2. Unit 2
3. Unit 3

## Testing
- Extend the hermetic fixtures in `tests/plugin-bootstrap-fixtures.sh` /
  `tests/plugin-install-smoke.sh` (shadowed curl + temp managed roots per
  hermetic-release-boundary-fixtures); no network, no user-home mutation.
- `tests/plugin-static.sh` keeps linting the launcher (shellcheck-style checks
  if present).

## Risks
- Host kills by process group would still take the background installer down
  with the launcher; accepted — no worse than today, and the trap cleanup
  keeps the version dir consistent (mv is atomic, temp files removed).
- Poll-loop `sleep 1` granularity adds up to ~1s connect latency in the
  waiting-peer path; negligible against the 20s budget.

## Implementation notes

- Execution capability: host implementation, because the POSIX launcher and
  hermetic release-boundary fixture form one cohesive activation path.
- Review weight: standard, project default.
- Files changed: plugin/bin/krometrail and
  tests/plugin-bootstrap-fixtures.sh. The installer remains the authoritative
  exact-release verifier and atomic publisher; lock ownership is intentionally
  in the launcher so independent launcher processes share one gate.
- Tests added: observable corrupt-binary verify failure, exact single-flight
  network count, stale PID reclaim, and a shadowed-curl slow-install reconnect
  fixture.
- Simplification: the launcher uses one lock/status path for both the
  connect-window budget and concurrent waiters; no alternate-version fallback
  or compatibility installer path was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: sh -n plugin/bin/krometrail plugin/scripts/install-managed.sh,
  bash -n tests/plugin-bootstrap-fixtures.sh, and
  bash tests/plugin-bootstrap-fixtures.sh passed. bash tests/plugin-static.sh
  is blocked by the pre-existing sibling ../skills catalog having a stale
  Krometrail pointer, outside this repository; the repository-local checks
  pass when that optional sibling is absent. bash tests/plugin-install-smoke.sh
  skipped because KROMETRAIL_PLUGIN_SMOKE=1 was not set.

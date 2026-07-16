# Hermetic Release-Boundary Fixtures

Shadow external commands and release assets inside temporary state to exercise network, platform, version, protocol-output, and rollback seams without real network or user-home mutation.

## Rationale

Distribution boundaries combine process execution, host detection, remote assets, persistent state, and failure recovery. Live-only tests are slow, environment-specific, and risky; shallow static assertions miss behavior. Hermetic shell fixtures preserve the real launcher/installer/release-helper code while replacing only the external boundary, allowing exact success and failure assertions in ordinary CI.

## Examples

### Managed release assets exist entirely in temporary state

**File**: `tests/plugin-bootstrap-fixtures.sh:13`

```bash
make_release() {
  local version="$1"
  local dir="$STATE/releases/$version"
  local assets=(
    krometrail-linux-x64
    krometrail-linux-arm64
    krometrail-darwin-x64
    krometrail-darwin-arm64
  )
  # ... write executable fixtures and exact checksums ...
}
```

The production installer sees realistic asset names and bytes without contacting GitHub.

### Platform and network commands are shadowed, not production logic

**File**: `tests/plugin-bootstrap-fixtures.sh:39`

```bash
cat >"$STATE/fake-bin/uname" <<'EOF'
#!/bin/sh
case "${1:-}" in
  -s|'') printf '%s\n' "${FAKE_UNAME_OS:-Linux}" ;;
  -m) printf '%s\n' "${FAKE_UNAME_ARCH:-x86_64}" ;;
esac
EOF
```

**File**: `tests/plugin-bootstrap-fixtures.sh:49`

```bash
cat >"$STATE/fake-bin/curl" <<'EOF'
#!/bin/sh
# ... record URL, select fixture bytes, inject redirects/checksum failures ...
cp "$source" "$output"
printf '200\n\n'
EOF
```

Tests control host and transport partitions while invoking the real package launcher and installer.

### Standalone installer uses the same boundary shape

**File**: `tests/installer-fixtures.sh:18`

```bash
make_fixture() {
  local dir="$1"
  mkdir -p "$dir/bin" "$dir/home" "$dir/install"
  cat > "$dir/bin/curl" <<'EOF'
  # ... local release metadata and artifact responses ...
EOF
}
```

The independent installer proves candidate identity, checksums, replacement, and preservation without touching the operator's installation.

### Release-helper failure is injected through a fake Cargo process

**File**: `tests/distribution-static.sh:330`

```bash
cat >"$plugin_version_tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == update ]]; then
  # ... deterministic lockfile projection ...
elif [[ "${PLUGIN_VERSION_FAIL:-0}" == 1 && "${1:-}" == check ]]; then
  exit 17
fi
EOF
```

The real release transaction is exercised through success and rollback while external build cost remains bounded.

## When to Use

- Installers, release helpers, platform matrices, marketplace launchers, or external CLI seams.
- The contract concerns exact command invocation, artifact selection, persistent state, rollback, or stdout/stderr separation.
- Live dependencies would be flaky, slow, costly, or unsafe in ordinary CI.

## When NOT to Use

- A normal unit test can cover the behavior without shell/process indirection.
- The contract is native harness interoperability; retain a separate opt-in real Claude/Codex smoke.
- A fake would replace the production logic being asserted rather than only its external boundary.

## Common Violations

- Contacting real release hosts or mutating the operator's home in ordinary tests.
- Covering only success while omitting interrupted updates and prior-state preservation.
- Asserting vague output instead of exact asset, version, path, hash, and state outcomes.
- Forgetting stdout purity for stdio protocol launchers.
- Letting a high-value hermetic suite exist only as a manual command instead of wiring it into CI.

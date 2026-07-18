# Exact-Release Managed Activation

Derive one exact product release for managed activation, verify it before execution, and keep plugin/config/update behavior release-coupled rather than `latest`-driven.

## Rationale

A native plugin may bootstrap its runtime, but its code, configuration, skill guidance, and protocol assumptions belong to one declared product release. Exact coupling prevents hidden background updates, ambiguous `PATH` selection, cross-major drift, and a plugin claiming readiness for a different executable.

## Examples

### Package launcher selects one versioned binary

**File**: `plugin/bin/krometrail:13`

```sh
EXPECTED_VERSION=$(cat "$VERSION_FILE")
# ... validate semver and managed root ...
MANAGED_BINARY="$MANAGED_ROOT/versions/$EXPECTED_VERSION/krometrail"
```

The launcher never asks for `latest` and never falls back to an unrelated command on `PATH`.

### Installer verifies the package-controlled release

**File**: `plugin/scripts/install-managed.sh:12`

```sh
VERSION="${1:-}"
MANAGED_ROOT="${2:-}"
MODE="${3:-install}"
# ... exact semver, checksum, ownership, and binary identity checks ...
```

Install and warm verification share the same exact version and destination authority before execution.

### Release transaction derives every projection

**File**: `scripts/bump-version.ts:134`

```ts
const derivedVersionPaths = rootPackageName === "krometrail"
  ? [
      "plugin/.claude-plugin/plugin.json",
      "plugin/.codex-plugin/plugin.json",
      ".claude-plugin/marketplace.json",
      ".agents/plugins/marketplace.json",
    ]
  : [];
```

Cargo remains authoritative while plugin manifests, catalogs, and the launcher marker move atomically as derived release metadata.

### Static contracts reject release drift

**File**: `tests/plugin-static.sh:46`

```sh
cargo_version="$(awk ... "$ROOT/Cargo.toml")"
# ...
[[ "$(cat "$PLUGIN_VERSION")" == "$cargo_version" ]] ||
  fail "plugin version marker does not match Cargo"
```

Distribution tests make version disagreement a release failure rather than a runtime surprise.

## When to Use

- Packaging a launcher, plugin, installer, or generated metadata that must match one product release.
- A runtime's config, guidance, schemas, or protocol behavior is coupled to the package version.
- Automatic activation is useful but unconstrained updates would cross a trust or compatibility boundary.

## When NOT to Use

- A user explicitly requests a standalone update where following the current stable release is the contract.
- A non-release cache intentionally spans compatible versions and carries no executable authority.
- Compatibility is negotiated dynamically through a versioned protocol rather than package coupling.

## Common Violations

- Querying or installing `latest` during plugin activation.
- Executing an arbitrary `PATH` binary instead of the package-owned exact release.
- Updating plugin/catalog versions outside the authoritative release transaction.
- Treating marketplace metadata installation as proof that managed activation has completed.
- Verifying version text only after an unsafe path or binary has already executed.

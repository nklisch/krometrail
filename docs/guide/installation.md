---
title: Installation
description: Build and install the current Rust Krometrail binary.
---

# Installation

## Current source installation

No Rust GitHub release has been published yet. Current users and contributors
must build the Rust binary from source:

```bash
bash scripts/dev-install.sh
```

Set `KROMETRAIL_INSTALL_DIR` to choose another destination. This path builds
`target/release/krometrail` with Cargo and verifies its `--version` output.

## Public installer readiness

The public POSIX installer will select a release asset for the current host,
verify it against `checksums.txt`, validate the temporary binary with
`--version`, and replace an existing installation only after all checks pass.
Until a post-cutoff Rust release exists, it rejects the preserved `v0.2.20`
TypeScript/DAP release and every older version before downloading an artifact.
The cutoff is immutable and centralized in `scripts/install.sh`.

When a post-cutoff Rust release is available, the installer will support Linux
x64 and arm64 plus macOS x64 and arm64. Linux release assets are
`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` binaries: they are
statically linked against musl and do not require a runner-specific glibc
baseline. Public asset names remain `krometrail-linux-x64` and
`krometrail-linux-arm64`.

Windows is a best-effort direct-download artifact, not an installer-supported
or supported development environment. Download `krometrail-windows-x64.exe`
and `checksums.txt` from a matching future GitHub release when needed.

## Local development install

To build the current Rust release binary for the local host and copy it to `~/.local/bin`:

```bash
bash scripts/dev-install.sh
```

Set `KROMETRAIL_INSTALL_DIR` to choose another destination. This development helper uses the host's native Cargo target; it is separate from the release workflow's reproducible musl Linux matrix.

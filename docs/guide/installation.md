---
title: Installation
description: Install a checksum-verified Krometrail release binary.
---

# Installation

## Public installer

The installer selects the release asset for the current host, verifies it against `checksums.txt`, installs it as `krometrail`, and verifies that the executable starts:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

Choose a version explicitly when needed:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh -s -- --version v0.2.20
```

Use a different destination without modifying shell startup files:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh -s -- \
  --install-dir "$HOME/.local/bin" \
  --no-modify-path
```

The installer supports Linux x64 and arm64 plus macOS x64 and arm64. The Linux release assets are `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` binaries: they are statically linked against musl and do not require a runner-specific glibc baseline. Public asset names remain `krometrail-linux-x64` and `krometrail-linux-arm64`.

Windows is a best-effort direct-download artifact, not an installer-supported or supported development environment. Download `krometrail-windows-x64.exe` and `checksums.txt` from the matching GitHub release when needed.

## Local development install

To build the current Rust release binary for the local host and copy it to `~/.local/bin`:

```bash
bash scripts/dev-install.sh
```

Set `KROMETRAIL_INSTALL_DIR` to choose another destination. This development helper uses the host's native Cargo target; it is separate from the release workflow's reproducible musl Linux matrix.

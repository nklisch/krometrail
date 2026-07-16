---
title: Development
description: Build, test, lint, run, and release the Rust Krometrail workspace.
---

# Development

## Requirements

Install Rust 1.85 or newer. The supported product development environments are Linux and macOS. Windows binaries are produced as a best-effort release artifact but Windows is not a supported development environment.

The browser runtime targets locally installed Chrome or a compatible Chromium browser. It can launch a managed browser or attach to an explicitly debug-enabled Chrome/Electron renderer endpoint through MCP. Electron's Node main process is not part of the browser boundary.

Linux release artifacts target `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, so they are statically linked against musl and do not inherit a glibc minimum from the GitHub runner. Local development builds remain native Cargo builds for the host environment.

Bun is optional repository tooling. It is used for VitePress documentation and selected browser fixture applications, never for the Krometrail product runtime.

## Rust quality gate

Run these commands from the repository root:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`cargo fmt --all` can apply formatting when needed. The other commands compile and test all six workspace crates plus the root binary.

## Run the binary

Run the current executable contract with:

```bash
cargo run -- --version
cargo run -- --help
cargo run -- doctor
cargo run -- mcp
```

`doctor` performs browser discovery without launching. `mcp` owns standard output for protocol traffic and serves the registry-derived browser-control tools until transport EOF or shutdown. It uses the same full runtime composition as controlled-browser capture and recording.

## Documentation and fixtures

Install the JavaScript tooling only when working on the docs site or a preserved browser target:

```bash
bun install --frozen-lockfile
bun run docs:dev
bun run docs:build
bun run docs:preview
```

`bun run docs:build` regenerates `docs/public/llms-full.txt` and builds the VitePress site. Browser fixtures are standalone applications; consult the [fixture classification](https://github.com/nklisch/krometrail/blob/main/tests/fixtures/browser/README.md) for their uses and launch details.

## Release preparation

Stable Rust releases are published from the root `Cargo.toml`, the sole product-version authority. The Bun release helper updates Cargo metadata, runs the Rust quality gate, and creates the repository release commit/tag/push workflow:

```bash
bun scripts/bump-version.ts patch
# minor, major, or an explicit x.y.z version are also accepted
```

GitHub Actions builds and checksums these stable asset names. The Linux rows use the pinned `houseabsolute/actions-rust-cross` action with the `v0.2.5` cross tag, digest-pinned toolchain images in `Cross.toml`, and fixed musl targets; each matrix artifact must run `--version` in its matching architecture before attestation and upload. The arm64 smoke gate uses explicit QEMU emulation on the x86_64 Linux runner.

The release matrix builds and checksums these stable asset names:

- `krometrail-linux-x64`
- `krometrail-linux-arm64`
- `krometrail-darwin-x64`
- `krometrail-darwin-arm64`
- `krometrail-windows-x64.exe`

The public installer rejects the preserved `v0.2.20` TypeScript/DAP release and older versions before downloading. Stable `v1.0.0` assets, checksums, and build-provenance attestations are published. See the [installation guide](installation.md) for binary and native agent-plugin paths. Use `scripts/dev-install.sh` to install a local host release build into `~/.local/bin`.

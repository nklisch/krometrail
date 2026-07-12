---
title: Development
description: Build, test, lint, run, and release the Rust Krometrail workspace.
---

# Development

## Requirements

Install Rust 1.85 or newer. The supported product development environments are Linux and macOS. Windows binaries are produced as a best-effort release artifact but Windows is not a supported development environment.

The future browser runtime targets locally installed Chrome or a compatible Chromium browser. The current executable does not connect to a browser yet. Explicitly debug-enabled Electron renderer endpoints are part of the intended browser boundary; Electron's Node main process is not.

Bun is optional repository tooling. It is used for VitePress documentation and selected browser fixture applications, never for the Krometrail product runtime.

## Rust quality gate

Run these commands from the repository root:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`cargo fmt --all` can apply formatting when needed. The other commands compile and test all five workspace crates plus the root binary.

## Run the binary

The current binary intentionally exposes only its executable contract:

```bash
cargo run -- --version
cargo run -- --help
cargo run -- doctor
```

The first two commands succeed. `doctor` currently reports an `unsupported` error because the browser transport adapter is not implemented. There is no current browser, recording, MCP, or debugger command to configure.

## Documentation and fixtures

Install the JavaScript tooling only when working on the docs site or a preserved browser target:

```bash
bun install --frozen-lockfile
bun run docs:dev
bun run docs:build
bun run docs:preview
```

`bun run docs:build` regenerates `docs/public/llms-full.txt` and builds the VitePress site. Browser fixtures are standalone applications; consult the [fixture classification](https://github.com/nklisch/krometrail/blob/main/tests/fixtures/browser/README.md) for their uses and launch details.

## Release

The root `Cargo.toml` owns the product version. The Bun release helper updates Cargo metadata, runs the Rust quality gate, and creates the repository release commit/tag/push workflow:

```bash
bun scripts/bump-version.ts patch
# minor, major, or an explicit x.y.z version are also accepted
```

GitHub Actions builds and checksums these stable asset names:

- `krometrail-linux-x64`
- `krometrail-linux-arm64`
- `krometrail-darwin-x64`
- `krometrail-darwin-arm64`
- `krometrail-windows-x64.exe`

The public installer downloads the matching asset and installs `krometrail`. Use `scripts/dev-install.sh` to install a local release build into `~/.local/bin`.

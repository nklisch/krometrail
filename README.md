# Krometrail

Krometrail is a Rust runtime for local browser control and temporal visual evidence for coding agents. The binary exposes browser discovery through `doctor` and the local MCP server through `mcp`; browser transport, controlled capture, durable recording, lifecycle/control/temporal MCP tools and resources, and per-session `every_nth_frame` capture configuration are implemented. Rich page-state and framework-state capabilities remain future extension points.

## Current executable

The current executable surface is intentionally small:

```bash
cargo run -- --version
cargo run -- --help
cargo run -- doctor
cargo run -- mcp
```

`--version` and `--help` report the binary contract. `doctor` discovers local Chrome/Chromium installations without launching. `mcp` serves the local MCP surface over stdio: browser lifecycle and 24 registry-derived control tools, temporal investigation and retention tools, browser-event queries, and retained temporal-evidence resources.

## Workspace

```text
Cargo.toml                 # workspace and root krometrail binary
src/                        # composition root, CLI, and runtime wiring
crates/
  krometrail-core/          # browser, recording, timeline, capability, and port contracts
  krometrail-cdp/           # production CDP adapter and capture/control supervision
  krometrail-store/         # durable recording index, segments, retention, and artifacts
  krometrail-mcp/           # MCP stdio server, generated schemas, tools, and resources
  temporal-vision/          # browser-agnostic visual-analysis boundary
tests/rust-runtime-smoke.rs # executable contract tests
tests/fixtures/browser/    # standalone browser target applications
```

The Rust workspace is the product runtime. The browser fixtures are test applications, not product libraries; their current uses are documented in [`tests/fixtures/browser/README.md`](tests/fixtures/browser/README.md). The intended CDP boundary treats Chrome-compatible pages and explicitly debug-enabled Electron renderer processes alike; Electron's Node main process is outside that boundary.

## Installation

### Binary

The public installer selects the current Linux or macOS x64/arm64 release asset, verifies `checksums.txt`, validates the candidate's exact version identity, and replaces an existing binary atomically:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
krometrail --version
krometrail doctor
```

Stable releases and their five assets are published at [GitHub Releases](https://github.com/nklisch/krometrail/releases/latest). Linux assets are statically linked musl binaries. Windows remains a best-effort direct-download `.exe` and is not supported by the POSIX installer.

For a source build instead:

```bash
bash scripts/dev-install.sh
```

### Claude Code and Codex plugin

The native plugin provides the shared Krometrail skill and a managed launcher for the local stdio MCP server:

```bash
# Claude Code
claude plugin marketplace add nklisch/krometrail --scope user
claude plugin install krometrail@krometrail --scope user

# Codex
codex plugin marketplace add nklisch/krometrail
codex plugin add krometrail@krometrail
```

On first MCP activation, the plugin downloads and verifies the exact Krometrail release it declares into private per-user plugin data; later starts are offline and perform no update check. A standalone `krometrail` command remains an optional, separate installation. The skill explains how to choose current screenshots, structured snapshots, temporal bundles, storyboards, difference maps, region filmstrips, motion history, source frames, and browser events without imposing one debugging workflow.

See the [full installation guide](https://krometrail.dev/guide/installation) for update, removal, alternate marketplace, and activation details.

## Development

Install Rust 1.85 or newer, then run the complete local gate:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the current binary with Cargo:

```bash
cargo run -- --help
cargo run -- doctor
```

Bun is development tooling only. It serves the VitePress documentation and launches selected browser fixtures; it does not build, test, or run the Krometrail product:

```bash
bun install
bun run docs:dev
bun run docs:build
```

## Release

Cargo.toml is the sole product version source. The release helper is retained as Bun tooling because it updates Cargo and all derived plugin/catalog versions atomically, runs the Rust gate, and performs the repository's release commit/tag/push workflow:

```bash
bun scripts/bump-version.ts patch
# or: minor, major, or an explicit x.y.z version
```

GitHub Actions builds the five stable asset names, generates `checksums.txt`, and publishes the GitHub release. Linux is built with pinned musl targets and digest-pinned cross toolchain images through the reproducible cross-build path; every artifact runs `--version` in a matching architecture before upload. The installer keeps the `krometrail` command and platform asset mappings stable. Windows remains a best-effort release artifact and is not a supported development environment.

## Documentation

Read [`docs/agents.md`](docs/agents.md) first. The five authoritative foundation documents are:

| Document | Purpose |
| --- | --- |
| [`VISION.md`](docs/VISION.md) | Product thesis, boundaries, and success criteria |
| [`SPEC.md`](docs/SPEC.md) | External behavior and system contracts |
| [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Rust workspace, boundaries, data flow, and failure isolation |
| [`VISUAL-EVIDENCE.md`](docs/VISUAL-EVIDENCE.md) | Temporal artifact vocabulary and provenance |
| [`EVALUATION.md`](docs/EVALUATION.md) | Capture, artifact, browser-control, and agent-evaluation criteria |

Current contributor instructions are in [`docs/guide/development.md`](docs/guide/development.md). The [MCP configuration guide](docs/guide/mcp-configuration.md) documents the implemented local stdio server and its lifecycle, control, temporal, event, and resource surfaces.

## License

MIT

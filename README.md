# Krometrail

Krometrail is a Rust foundation for local browser control and temporal visual evidence for coding agents. The repository currently contains the Rust workspace, domain contracts, composition root, and classified browser target fixtures. Browser transport, persistence, MCP tools, and temporal artifact generation are designed in the foundation documents but are not exposed by the current executable yet.

## Current executable

The Rust binary currently exposes only a deliberately small surface:

```bash
cargo run -- --version
cargo run -- --help
cargo run -- doctor
```

`--version` and `--help` succeed. `doctor` currently fails explicitly because browser transport has not been implemented; it must not be treated as a successful capture check.

## Workspace

```text
Cargo.toml                 # workspace and root krometrail binary
src/                        # composition root, CLI, and runtime placeholders
crates/
  krometrail-core/          # browser, recording, timeline, capability, and port contracts
  krometrail-cdp/           # reserved CDP adapter boundary
  krometrail-store/         # reserved recording-store boundary
  krometrail-mcp/           # reserved MCP boundary
  temporal-vision/          # browser-agnostic visual-analysis boundary
tests/rust-runtime-smoke.rs # executable contract tests
tests/fixtures/browser/    # standalone browser target applications
```

The Rust workspace is the product runtime. The browser fixtures are test applications, not product libraries; their current uses are documented in [`tests/fixtures/browser/README.md`](tests/fixtures/browser/README.md). The intended CDP boundary treats Chrome-compatible pages and explicitly debug-enabled Electron renderer processes alike; Electron's Node main process is outside that boundary.

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

Cargo.toml is the sole product version source. The release helper is retained as Bun tooling because it updates Cargo, runs the Rust gate, and performs the repository's release commit/tag/push workflow:

```bash
bun scripts/bump-version.ts patch
# or: minor, major, or an explicit x.y.z version
```

GitHub Actions builds the five stable asset names, generates `checksums.txt`, and publishes the GitHub release. The installer keeps the `krometrail` command and platform asset mappings stable. Windows remains a best-effort release artifact and is not a supported development environment.

## Documentation

Read [`docs/agents.md`](docs/agents.md) first. The five authoritative foundation documents are:

| Document | Purpose |
| --- | --- |
| [`VISION.md`](docs/VISION.md) | Product thesis, boundaries, and success criteria |
| [`SPEC.md`](docs/SPEC.md) | External behavior and system contracts |
| [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Rust workspace, boundaries, data flow, and failure isolation |
| [`VISUAL-EVIDENCE.md`](docs/VISUAL-EVIDENCE.md) | Temporal artifact vocabulary and provenance |
| [`EVALUATION.md`](docs/EVALUATION.md) | Capture, artifact, browser-control, and agent-evaluation criteria |

Current contributor instructions are in [`docs/guide/development.md`](docs/guide/development.md). The current MCP page explains why no MCP configuration should be added yet and links the intended boundary contracts; it does not advertise an unavailable command.

## License

MIT

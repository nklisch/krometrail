# Krometrail

Krometrail is a Rust 2024 workspace for local browser control and temporal visual evidence. The binary exposes `--version`, `--help`, browser-discovery `doctor`, and the `mcp` stdio server. Browser transport, controlled capture, durable recording storage, registry-derived control operations, temporal investigation and retention tools, browser-event queries, retained evidence resources, and per-session `every_nth_frame` are implemented; page-state and framework-state capabilities remain future extension points.

## Current Contract Discipline

Krometrail is an agent tool without supported third-party integrations. Optimize the executable, MCP surface, plugin, and retained evidence for the current agent workflow instead of preserving superseded shapes for hypothetical callers.

- Replace obsolete request, response, installer, and persisted-format paths directly. Do not add compatibility shims, deprecated aliases, dual schemas, or migration prose unless a concrete supported consumer is identified first.
- Keep one current persisted format. Open current data directly; reject an incompatible older store with a clear recovery action instead of maintaining historical migrations in the runtime.
- Keep unpublished Rust internals clean and direct, and remove dead abstractions and tests when their behavior is removed.
- Keep foundation documents and skill instructions current with the supported contract. Git carries history; current docs should not preserve superseded runtime instructions.
- Continue validating inputs, invariants, privacy, provenance, and failure behavior rigorously. Removing compatibility work does not weaken correctness requirements.
- Keep Cargo.toml as the sole release version authority. Plugin and catalog versions are derived projections, and each plugin release must select exactly the matching verified binary without polling `latest` or mutating standalone installations.

## Project Structure

```text
Cargo.toml
src/
  main.rs       Process entry point and error reporting
  cli.rs        Current clap command surface
  app.rs        Composition root and injected runtime ports
crates/
  krometrail-core/   Domain identities, time, lifecycle, recording, timeline, capabilities, errors, and ports
  krometrail-cdp/    Production CDP adapter, control, capture, and session supervision
  krometrail-store/  Durable recording index, segments, retention, recovery, and artifacts
  krometrail-mcp/    MCP stdio, generated schemas, registry-derived tools, and evidence resources
  temporal-vision/   Browser-agnostic visual-analysis boundary
tests/
  rust-runtime-smoke.rs  Binary contract checks
  fixtures/browser/      Standalone browser target applications
scripts/
  bump-version.ts       Bun release helper; Cargo remains the version source
  generate-llms-full.ts  Bun documentation generator
  install.sh             Public binary installer
  dev-install.sh         Local Cargo release install

docs/
  agents.md             Documentation navigation
  VISION.md             Authoritative product thesis and boundaries
  SPEC.md               Authoritative external contracts
  ARCHITECTURE.md       Authoritative system architecture
  VISUAL-EVIDENCE.md    Authoritative temporal-evidence semantics
  EVALUATION.md         Authoritative evaluation plan
```

Browser fixtures are target applications, not a second Krometrail runtime. Their current purpose is recorded in `tests/fixtures/browser/README.md`; framework names in fixture paths do not imply framework-state support in the product.

## Documentation Rules

- Read `docs/agents.md` and the five foundation documents before changing behavior or describing the system.
- Foundation documents describe the current direction and intended contracts. Do not replace them with historical runtime documentation.
- Do not add command examples for capabilities that are not present in `src/cli.rs`.
- Do not edit generated `docs/public/llms-full.txt` directly; regenerate it with `bun run docs:build`.
- Bun is allowed for VitePress documentation and preserved browser fixtures only. It is not the product runtime.

## Commands

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -- --version
cargo run -- --help
cargo run -- doctor
```

`doctor` is discovery-only: it reports discovered Chrome/Chromium installations or a structured `browser_not_found` failure without launching. `mcp` serves lifecycle, browser-control, temporal, browser-event, and retained-evidence resource surfaces over stdio; stdout is reserved for JSON-RPC and the server exits cleanly on stdin EOF.

For documentation development only:

```bash
bun install
bun run docs:dev
bun run docs:build
```

## Releasing

Cargo.toml's root `[package].version` is the only product version source. Use the release helper after the working tree and Rust gates are ready:

```bash
bun scripts/bump-version.ts patch
# or minor, major, or an explicit x.y.z version
```

The helper validates and updates Cargo metadata, runs the Rust quality gate, and performs the repository release commit/tag/push workflow. GitHub Actions builds the stable Linux, macOS, and best-effort Windows asset names and publishes checksums. The installer and developer installer retain the `krometrail` executable name.

## Conventions

- Keep domain logic in `krometrail-core`; infrastructure implementations depend inward on its ports.
- Keep one registry for growing variant sets such as capabilities.
- Validate external input and domain invariants at boundaries; fail explicitly on unsupported operations.
- Keep source, observed, and normalized session time distinct.
- Preserve the public viewport contract: ergonomic responsive and mobile presets materialize to explicit target-scoped metrics, custom metrics remain available, and clear restores browser defaults.
- Do not include AI attribution lines in commit messages.

<!-- agile-workflow:start -->
## Agile-Workflow Substrate

Work tracked in `.work/` as markdown items with YAML frontmatter (`kind, stage, tags, parent, depends_on, release_binding, research_refs, research_origin`; `[research]` items also carry the commissioning `research_dials` block). Layout: `.work/active/{epics,features,stories}/`, `.work/backlog/`, `.work/releases/<version>/`, `.work/archive/`. The `.work/` ↔ `.research/` handoff follows the agentic-research plugin contract.

**Primary query tool:** `.work/bin/work-view` filters by stage, tag, kind, parent, and dependency. Common patterns:
- `work-view --ready` — items ready to work (deps satisfied)
- `work-view --stage review` — items awaiting an agent review pass (`/agile-workflow:review`)
- `work-view --parent <id>` / `--blocking <id>` — hierarchy / sequencing
- `work-view --scope all` — include terminal tiers: `releases/` (one summary doc per version) and `archive/` (bodyless ref stubs). Full bodies live in git history. By default work-view shows only active + backlog; `--release` / `--gate` auto-widen to all tiers.
- `work-view --help` for the full flag set

Foundation docs in `docs/` describe the system's current state or intended future state, never the past; git history is the audit trail. Item files are the durable state: update the body with implementation discoveries, review findings, blockers, and decisions instead of relying on chat history.

Project agent rules live in `.agents/rules/*.md` (plugin-managed rules in `.agents/rules/agile-workflow.md`). Do not maintain a second `.claude/rules/` source of truth.

**Before designing, implementing, or reviewing, read `.agents/rules/*.md`** — the project's force-loaded agent rules (tag semantics, test integrity, review policy). The agile-workflow hook auto-loads these at session start and after compaction; read them directly when working without the hook. Do not rely on UserPromptSubmit for rules or queue snapshots; query `work-view` when queue state is needed.

Project-specific refactor style conventions belong in this file under `## Refactor Style Conventions`. Detailed refactor convention references belong in `.agents/skills/refactor-conventions/` and extend `refactor-design`'s defaults; they do not replace the built-in scan and they do not create standalone plan docs.

<!-- agile-workflow:end -->

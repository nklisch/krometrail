---
id: human-centered-documentation-site
kind: feature
stage: implementing
tags: [prose, documentation, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-16
---

# Make the documentation site useful to humans first

## Brief

Rework the public documentation site from an implementation-centered reference into a clear product experience for people deciding whether and how to use Krometrail. The primary surfaces should answer, in order: what Krometrail helps an agent see, how to install it through the Claude Code or Codex plugin or as a standalone binary, how an agent uses temporal browser evidence to diagnose visual motion and transient-state problems, and where to go when setup fails.

Public examples must use the installed `krometrail` command. Development-only `cargo run -- ...` commands belong only in contributor documentation. Deep architecture, research, evaluation machinery, evidence archives, and codebase idiosyncrasies should not compete for attention on the primary site surfaces; retain authoritative material where maintainers and agents need it, but remove it from the human-first journey and navigation.

Rewrite the voice so it is concrete, confident, and memorable without becoming inflated or cute. Lead with recognizable browser problems and useful outcomes rather than Rust implementation details, capability counts, protocol vocabulary, or internal status inventories. Use independent editorial passes from GLM 5.2, GPT-5.6 Luna, and Kimi K2.7, then consolidate their strongest evidence-backed recommendations rather than mechanically blending all suggestions.

## Strategic decisions

- **Primary audience:** people using coding agents to build and debug browser interfaces; contributors are a secondary audience with a separate path.
- **Primary journey:** understand the outcome, install in the preferred environment, let the agent use Krometrail, then troubleshoot only if needed.
- **Technical depth:** progressive disclosure. Primary surfaces stay outcome- and workflow-focused; implementation and research remain available but out of the main journey.
- **Command voice:** installed-binary examples use `krometrail`; Cargo commands appear only in development documentation.

## Simplification opportunity

Consolidate duplicated setup and runtime explanations, remove internal capability inventories from landing pages, reduce top-level navigation, and stop presenting foundation/research documents as end-user starting points. Preserve one authoritative installation path and one contributor path rather than repeating mixed-audience instructions across the homepage, README, and guides.

## Outline

### Public journey

1. `docs/index.md`: a product landing page that names the transient-browser problem, shows what an agent can do with Krometrail, and offers plugin and one-line binary installation immediately.
2. `docs/guide/installation.md`: short choice-first setup for Claude Code, Codex, or standalone use, followed by verification, updates, removal, and advanced/manual setup.
3. `docs/guide/using-krometrail.md`: a human-facing workflow showing when to ask an agent to use Krometrail, representative prompts, what evidence it can inspect, and what remains local.
4. `docs/guide/troubleshooting.md`: practical setup and browser-discovery recovery without protocol internals.
5. `docs/reference/runtime.md` and `docs/reference/configuration.md`: concise operator reference with installed-binary commands and current configuration only.

### Information architecture and secondary surfaces

- Rework `docs/.vitepress/config.ts` around `What it does`, `Install`, `Use with your agent`, `Troubleshooting`, and a subdued reference/contributor path.
- Keep foundation, evaluation, evidence, and research documents available by direct URL and for maintainers/agents, but remove them from primary navigation and user onboarding.
- Rewrite `README.md` as a concise repository landing page that mirrors the public promise and routes users to the site; retain development commands only under a clearly secondary contributor section.
- Update `scripts/generate-llms-full.ts` only as needed so generated agent-facing documentation reflects the new user documentation rather than describing the site as a Rust contributor reference.

### Acceptance criteria

- Every end-user command invokes `krometrail`; `cargo run -- ...` appears only in contributor/development context.
- The homepage shows a plugin path and one-line binary path before technical explanation.
- Primary navigation contains no research, architecture, specification, evaluation, or evidence-archive entry.
- Copy leads with recognizable visual-motion/transient-state problems and agent outcomes, not language/runtime/protocol internals or tool counts.
- Setup claims remain exact for both Claude Code and Codex plugin commands and the standalone installer.
- Privacy/local-first behavior and limitations remain accurate without dominating the main journey.
- `bun run docs:build` succeeds, generated docs are current, and internal links resolve.

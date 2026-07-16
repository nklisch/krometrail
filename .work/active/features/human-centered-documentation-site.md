---
id: human-centered-documentation-site
kind: feature
stage: done
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

## Editorial synthesis

Three independent passes shaped the rewrite: GLM 5.2 audited information architecture, GPT-5.6 Luna focused voice and copy, and Kimi K2.7 walked the Claude Code, Codex, and standalone journeys. Their common finding was structural rather than cosmetic: the product's strongest language already exists in the vision, but the site hides it behind a contributor-first hierarchy.

### Voice

- Lead with the contrast: screenshots show what a page is; Krometrail preserves what it did.
- Name recognizable failures—flicker, reversals, layout shifts, hydration flashes, focus jumps, and canvas motion—before naming artifacts or protocols.
- Be confident about preserved evidence, but never claim perfect frame capture, automatic diagnosis, or causal proof.
- Prefer verbs and visible behavior over implementation nouns. Avoid “registry-derived,” “composition root,” tool counts, release mechanics, and Rust identity on user surfaces.
- Keep the local-first and capture-gap caveats near the claims they qualify rather than using them as the opening pitch.

### Chosen information architecture

- **Top navigation:** Install; Use with your agent; Troubleshooting; a secondary Reference menu.
- **Guide sidebar:** Installation → Use with your agent → Troubleshooting, followed by Manual MCP setup and a clearly separated Contributor section.
- **Reference sidebar:** Runtime commands and configuration.
- Foundation, evaluation, evidence archives, and research remain directly addressable and authoritative, but leave the primary navigation and onboarding path.

### Page responsibilities

- The homepage earns interest, makes both installation paths immediately visible, and explains the normal debugging loop.
- Installation helps the reader choose a path, run exact commands, verify activation, then progressively discloses update/removal/advanced mechanics.
- Use with your agent supplies practical prompts, sets permission expectations, explains the evidence ladder in plain language, and closes with fix-and-verify.
- Troubleshooting starts from observed symptoms and safe checks; it never suggests running `krometrail mcp` interactively.
- Runtime and configuration remain compact, current-state references without source-code pointers or future-contract prose.

## Implementation notes

- **Execution capability:** inline GPT-5.6 authoring with read-only editorial input from GLM 5.2, GPT-5.6 Luna, and Kimi K2.7. One owner retained voice and information-architecture coherence across the site; the independent passes supplied distinct product, copy, and first-use lenses.
- **Review weight:** standard (project default).
- **Files changed:** rewrote `docs/index.md`, `docs/guide/installation.md`, `docs/guide/mcp-configuration.md`, `docs/reference/runtime.md`, `docs/reference/configuration.md`, and `README.md`; added `docs/guide/using-krometrail.md` and `docs/guide/troubleshooting.md`; reorganized `docs/.vitepress/config.ts`; aligned `docs/agents.md` and `docs/guide/development.md`; curated the generated agent surface through `scripts/generate-llms-full.ts` and regenerated `docs/public/llms-full.txt`.
- **Tests added/removed:** none. `bun run docs:build` validates VitePress rendering, links, sitemap generation, and the generated agent documentation; `bash tests/distribution-static.sh` validates release, installer, and plugin documentation contracts.
- **Verification:** the documentation build and distribution contracts pass. End-user command searches show Cargo invocation only in the explicit development guide and retained transport-evidence instructions. Primary pages no longer use implementation-first phrases or hard-coded tool counts.
- **Simplification:** removed research and foundation documents from primary navigation; replaced the contributor-first guide with a task-ordered user journey; consolidated duplicated setup prose; removed source-code pointers, future configuration prose, registry terminology, and exact tool counts from public reference pages; reduced `llms-full.txt` from an indiscriminate repository-doc glob to eight practical usage documents.
- **Discrepancies from design:** none. The homepage uses VitePress's home layout rather than a plain document layout to make the new hierarchy visible without introducing custom styling.
- **Adjacent issues parked:** none.

## Review (2026-07-16)

**Verdict:** Approve

**Blockers:** none

**Important:** none

**Nits:** “four command-line entry points” could be read as conflating flags and subcommands, though the table is explicit; “Before / during / after” could be labeled as an orientation view. Both are clear in context and do not warrant churn.

**Rejected:** none

**Notes:** Standard-weight single independent pass by a fresh GLM 5.2 reviewer. The reviewer verified the first-use journey, voice, commands, plugin contracts, navigation and links, generated-document curation, local-data and capture-limit claims, and every acceptance criterion. Review relied on the already-green documentation build and distribution contracts while independently inspecting their outputs and contract sources. No material current-cycle findings remain.

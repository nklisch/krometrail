# Migration Report — agile-workflow bootstrap

**Date:** 2026-07-11

**Source shape:** workflow-plugin

**Destination:** `.work/` substrate

**Plugin version:** 0.15.3

## Foundation docs detected

- `docs/VISION.md` — preserved and included with the bootstrap commit.
- `docs/SPEC.md` — preserved and included with the bootstrap commit.
- `docs/ARCHITECTURE.md` — preserved and included with the bootstrap commit.
- `docs/VISUAL-EVIDENCE.md` — preserved and included with the bootstrap commit.
- `docs/EVALUATION.md` — preserved and included with the bootstrap commit.

The six pre-bootstrap working-tree changes were one completed, user-approved foundation-authoring cluster rather than in-flight implementation. They are included in the bootstrap commit and did not produce an `implementing` feature item.

## Run mode and entrypoints

- Mode: bootstrap (`.work/` was absent).
- Entrypoint model: `agents-canonical`.
- Canonical instruction file: `.agents/AGENTS.md`.
- Root `AGENTS.md`: installed as a symlink to `.agents/AGENTS.md`.
- Root `CLAUDE.md`: existing healthy symlink to `.agents/AGENTS.md`, preserved.
- Agile-workflow managed section: installed in `.agents/AGENTS.md` after the dense rules were written and verified.
- Plugin-managed rules: installed at `.agents/rules/agile-workflow.md`.
- Agentic-research integration: detected and advertised through research fields, routing semantics, and commissioning dials.

## Items seeded

### Active items

None. The legacy project work had shipped before the direction reset.

### Retroactive release

- `.work/releases/v0.2.20/release-v0.2.20.md`
- Stage: `released`
- Shipped rows: 49 legacy design items, all classified `done-shipped` from implementation and Git evidence.
- Terminal retention: `delete-refs`; no individual terminal item bodies were created.

The release summary contains each legacy design’s item id, title, kind, and Git reference.

### Backlog

None. The new foundation is decomposed after bootstrap through `/agile-workflow:epicize`.

## Legacy-design content integrity

The confirmed cleanup removed `docs/designs/` only after the following gate passed:

- 49 tracked Markdown files identified.
- 1,429 heading/frontmatter blocks accounted for.
- Every working file was byte-equal to the file recoverable at its recorded Git reference.
- Every whole-file SHA-256 digest was verified before deletion.
- Every design received a terminal row in the `v0.2.20` release summary.
- Terminal state: `landed_existing` in Git history plus `landed_this_run` as the release-summary reference.

This whole-file equality check is stronger than checking heading anchors alone and proves all source blocks remain recoverable.

## Classified artifact inventory

### Canonical or healthy compatibility

- `.agents/AGENTS.md` — canonical project instruction file.
- `CLAUDE.md` — healthy symlink to the canonical entrypoint.
- `.agents/skills/krometrail-chrome` — unrelated project skill symlink; preserved.
- `.agents/skills/krometrail-debug` — unrelated project skill symlink; preserved.
- `.agents/skills/krometrail-mcp` — unrelated project skill symlink; preserved.
- `.claude/skills/citty` and `.claude/skills/krometrail-*` — healthy project-skill mirrors; preserved.

### Unrelated project skills

- `.agents/skills/citty/`
- `.agents/skills/react-devtools/`
- `.agents/skills/solid-devtools/`
- `.agents/skills/svelte-devtools/`
- `.agents/skills/vue-devtools/`

These do not mirror an agile-workflow-owned concept and were left untouched.

### Bespoke overlap candidates

- `.agents/skills/structural-refactor/` and its Claude symlink mirror — overlaps refactor conventions and generates a standalone plan document.
- `.agents/skills/stylistic-refactor/` and its Claude symlink mirror — overlaps refactor conventions and generates a standalone plan document.
- `.claude/skills/patterns/` — Claude-only copy of the plugin-owned patterns concept with no `.agents` canonical.
- `.claude/rules/patterns.md` — legacy structural-pattern digest pointing to the Claude-only pattern skill.

The user selected no convergence. All four candidates remain in place. No pattern or refactor content was copied, replaced, or deleted.

### Legacy tracking artifacts

- `docs/designs/` — 49 files migrated to Git-backed terminal references and deleted after exact-path confirmation.
- `docs/legacy/` — preserved; not selected as a tracking cleanup target.
- `docs/structural-refactor-plan.md` and `docs/stylistic-refactor-plan.md` — preserved because their owning bespoke skills were not converged.

## Reference integrity

Before deleting `docs/designs/`, inbound references were discovered and rewritten in:

- `.agents/AGENTS.md`
- `.agents/skills/structural-refactor/references/docs-hierarchy.md`
- `docs/agents.md`
- `docs/public/llms-full.txt`
- `docs/reference/changelog.md`
- `docs/structural-refactor-plan.md`

A final repository search found no live reference to `docs/designs/` outside the deletion target.

## Cleanup decisions

- Cleanup scope: `legacy-cleanup`.
- Confirmed deletion: `docs/designs/` and `docs/designs/completed/` as one exact legacy tracking tree.
- Preserved because convergence was declined: legacy pattern and refactor skills/rules.
- Source code, tests, build configuration, and non-tracking documentation were outside convert cleanup and remain unchanged.

## Conventions chosen

- Release mapping: `tag-based`.
- Tags: `visual`, `browser`, `storage`, `agent-ux`, `infra`, `security`, `testing`, `perf`, `refactor`, `prose`, `research`.
- Slugs: kebab-case with parent prefixes for children.
- Stage overrides: none.
- Terminal-tier retention: `delete-refs`.
- Release gates: security → tests → cruft → docs → patterns.
- Finding routing: critical/high → implementing; medium → drafting; low → backlog; info → skip.
- Binding guard: `warn`.
- Epic cohesion: `phased`.
- Backlog staleness threshold: 90 days.

## Installed artifacts

- `.work/CONVENTIONS.md`
- `.work/bin/work-view` 0.15.3
- `.agents/rules/agile-workflow.md`
- Agile-workflow managed section in `.agents/AGENTS.md`
- Root `AGENTS.md` compatibility symlink
- `.work/` active, backlog, release, and archive tiers

## Next steps

1. Run `/agile-workflow:epicize` to decompose the new foundation into implementation epics.
2. Make the Rust CDP compatibility spike the first dependency gate before deleting the TypeScript implementation.
3. Remove or replace obsolete TypeScript-specific skills and documentation as part of the tracked rewrite, with Git tag `v0.2.20` as the reference.
4. Delete this migration report after the bootstrap has been reviewed.

---
head:
  - - meta
    - name: robots
      content: noindex, nofollow
---

# Agent Instructions — Documentation Navigation

## Foundation docs (current, authoritative)

These docs in `docs/` describe the current system and should be trusted:

- **VISION.md** — Project motivation, problem statement, and what's implemented
- **ARCH.md** — System architecture (layers, data flow, compression, viewport rendering)
- **SPEC.md** — Adapter interface contract, breakpoint types, resource limits
- **UX.md** — Viewport abstraction, interaction patterns, agent skill file
- **ADAPTER-SDK.md** — How to create a new language adapter (step-by-step guide)
- **PRIOR_ART.md** — Historical analysis of other MCP-DAP projects (reference only)

## Generated docs (auto-generated, do not edit)

Files in `docs/.generated/` are produced by `scripts/generate-docs.ts` from Zod schemas and the adapter/framework registries. Regenerate with `bun run generate-docs` after changing tool schemas, adapters, or frameworks.

## Legacy docs (outdated, do not use for current state)

Files in `docs/legacy/` contain early-phase planning that no longer reflects the codebase:

- **ROADMAP.md** — Original phase-by-phase roadmap. All phases are complete. The "What Exists Today" section describes the project as barely scaffolded.
- **INTERFACE.md** — Original CLI/MCP interface design. The CLI was overhauled to use namespaced subcommands (`krometrail debug launch` instead of `krometrail launch`). Missing browser commands, attach, threads, and other features added later.
- **TESTING.md** — Original test strategy. Fixture paths and test file structure are speculative and don't match the actual layout.

These files are kept for historical reference. Do not rely on them for understanding current behavior.

## Completed design docs (historical, do not use for current state)

Files in `docs/designs/completed/` are detailed phase design documents that guided implementation. All 35 designs are fully implemented. These are valuable for understanding *why* something was built a certain way, but should not be used to understand *what* currently exists — the code and foundation docs are the source of truth.

## Framework state docs

Files in `docs/framework-state/` describe the framework observation subsystem. The per-framework `SPEC.md`, `INTERFACE.md`, and `ARCH.md` files for React and Vue are current. The Solid and Svelte specs describe planned-but-unimplemented observers (only detection is implemented for those frameworks).

---
id: story-skill-evaluate-read-guidance
kind: story
stage: done
tags: [prose, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Teach read-only evaluation's DOM allowlist in the skill

## Brief

V8's side-effect-free evaluation (used by read-only `evaluate_page`) refuses
`document.getElementById(...)` — and any expression containing it — while
allowing `document.querySelector(...)`. An agent's most natural DOM read fails
with a side-effect refusal and the fix is a non-obvious rewrite. Add one concise
line to the krometrail plugin skill (and its evaluate/evidence reference if it
has an evaluate section): prefer `querySelector`/`querySelectorAll` in read-only
expressions; some DOM getters sit outside V8's side-effect-free allowlist.
Regenerate `docs/public/llms-full.txt` via `bun run docs:build` if any generated
doc source changes.

Absorbed backlog: `idea-evaluate-dom-read-guidance`.

## Acceptance

- Skill text carries the querySelector-over-getElementById guidance in the
  evaluate_page context, one or two sentences, no new section.
- No generated docs edited by hand.

## Completion notes

Added a two-sentence guidance block after the current-state tool list in
`plugin/skills/krometrail/SKILL.md` (the only evaluate-usage teaching surface).
`docs/SPEC.md` only names the evaluation capability and carries no expression
guidance, and `docs/public/llms-full.txt` derives from `docs/` — no regeneration
needed. Implemented inline as prose per the prose lane; verified by reading the
edited section.

---
id: idea-evaluate-dom-read-guidance
created: 2026-07-19
updated: 2026-07-19
tags: [prose, agent-ux]
---

Small skill/doc guidance gap found in the 2026-07-19 motion workload: V8's
side-effect-free evaluation (used by read-only `evaluate_page`) refuses
`document.getElementById(...)` (and object-literal expressions containing it)
while allowing `document.querySelector(...)` — so an agent's most natural DOM
read fails with the side-effect refusal (currently presented as "threw EvalError"
until idea-evaluate-refusal-needle-drift is fixed) and the fix is a non-obvious
rewrite. Add one line to the krometrail skill and evaluate_page guidance:
prefer `querySelector`/`querySelectorAll` in read-only expressions; note that
some DOM getters are outside V8's side-effect-free allowlist.

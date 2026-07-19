---
id: idea-evaluate-refusal-needle-drift
created: 2026-07-19
updated: 2026-07-19
tags: [bug, browser]
---

Found in the 2026-07-19 post-fix live shakedown (Chrome 149.0.7827.155): the
`evaluate_page` refusal-vs-throw split from `feature-failure-surface-clarity` never
classifies a side-effect refusal on current Chrome. A mutating expression
(`document.title = "x"`) reports `page evaluation threw: EvalError: Possible
side-effect in debug-evaluate` instead of the designed "page evaluation was refused
as side-effecting".

Root cause: `evaluation_exception_error` in
`crates/krometrail-cdp/src/control/evaluation.rs:112-120` matches the lowercase
needle "side effect" (with a space), but V8/Chrome 149 emits "Possible side-effect
in debug-evaluate" (hyphenated), so the refusal branch is unreachable and every
refusal falls through to the throw presentation. Fix direction: match both "side
effect" and "side-effect" (or normalize hyphens before matching) and add a
deterministic decode test using the real Chrome 149 description string.

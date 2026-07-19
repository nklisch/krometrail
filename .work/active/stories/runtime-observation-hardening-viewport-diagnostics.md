---
id: runtime-observation-hardening-viewport-diagnostics
kind: story
stage: done
tags: [browser, agent-ux]
parent: runtime-observation-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Use truthful desktop viewport authority and expose bounded mismatch facts

Correct responsive viewport acknowledgement on overflowing Chrome pages and add privacy-safe numeric diagnostics for genuine geometry, DPR, or touch mismatches. Verification must include the Krometrail public documentation site on current real Chrome.

## Implementation notes

- Execution capability: inline implementation; the change is one lifecycle-complete viewport authority correction with tightly coupled deterministic and real-browser evidence.
- Review weight: standard (project default).
- Root cause: Chrome 150 reports both `cssLayoutViewport` and `cssVisualViewport` at the scrollbar-reduced 384px content width after a 390px desktop override, while `window.innerWidth` remains the requested 390px layout authority.
- Files changed: `crates/krometrail-cdp/src/control/viewport.rs`, viewport-related test doubles in `control/tests.rs`, `control/navigation.rs`, `session/mod.rs`, and `tests/support/scripted_cdp.rs`, plus `tests/verified_interactions.rs`.
- Implementation: the existing bounded runtime projection now returns `window.innerWidth`/`innerHeight`; desktop acknowledgement and capture geometry use that layout size, mobile remains visual-authoritative, and CDP visual metrics remain the reported content area. `viewport.ack.failed` emits only expected/observed numeric geometry, DPR/touch state, mismatch flags, and opaque target identity.
- Tests added/changed: a deterministic Chrome-150 regression proves layout 390 / visual 384 and capture width 390; mismatch facts cover geometry, DPR, and touch independently; the opt-in real-Chrome viewport qualification applies the 390x844 responsive preset to `https://krometrail.dev/` and proves the public site's scrollbar-reduced visual content remains accepted.
- Verification: `cargo test -p krometrail-cdp control::viewport --locked`; `cargo test -p krometrail-cdp --lib --locked` (176 passed); `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test verified_interactions opt_in_real_chrome_qualifies_viewport_presets_guidance_and_target_isolation --locked -- --nocapture` (passed against current local Chrome and the public site).
- Simplification: removed CDP `cssLayoutViewport` as a competing desktop acknowledgement authority; no polling, retry, or second viewport observation path was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.

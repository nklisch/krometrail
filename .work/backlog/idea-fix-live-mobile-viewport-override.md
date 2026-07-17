---
id: idea-fix-live-mobile-viewport-override
created: 2026-07-17
updated: 2026-07-17
tags: [browser, agent-ux]
---

An exploratory plugin run navigated a managed Chrome 150 temporary-profile session to Wikipedia, waited for complete readiness, and requested a 390×844 mobile/touch viewport at device scale factor 3. `set_viewport` failed with `target_failed: browser did not apply the requested viewport metrics`; subsequent evidence remained at the original 1200×702 viewport and device scale factor 2. Following the returned recovery guidance, the agent reloaded the page, waited for complete readiness, and retried the same override; it failed identically. Correlations: `5be00476-4c06-4746-b207-6a857e7f6b3f` and `fc86637e-d241-43d0-80cd-b24ee1f46f03`.

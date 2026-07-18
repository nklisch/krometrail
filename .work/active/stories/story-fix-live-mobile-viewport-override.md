---
id: story-fix-live-mobile-viewport-override
kind: story
stage: review
tags: [bug, browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Fix live mobile viewport override

## Symptom

An exploratory plugin run navigated a managed Chrome 150 temporary-profile session to Wikipedia, waited for complete readiness, and requested a 390×844 mobile/touch viewport at device scale factor 3. `set_viewport` failed with `target_failed: browser did not apply the requested viewport metrics`; subsequent evidence remained at the original 1200×702 viewport and device scale factor 2. Following the returned recovery guidance, the agent reloaded the page, waited for complete readiness, and retried the same override; it failed identically. Correlations: `5be00476-4c06-4746-b207-6a857e7f6b3f` and `fc86637e-d241-43d0-80cd-b24ee1f46f03`.

## Root cause

`Emulation.setDeviceMetricsOverride` enables Chrome's mobile layout behavior but does not guarantee that the CSS visual viewport equals the requested dimensions. On pages without an effective viewport meta tag, Chrome applies a fallback page scale (0.348214 in the reproduced 390×844 case), so `Page.getLayoutMetrics` reports approximately 1120×2424 even though device scale factor and touch emulation were applied. Krometrail then correctly rejects the partially applied contract, but it never supplied the page-scale command needed to make its requested CSS viewport authoritative.

## Fix

- Set page scale factor to 1 after applying a mobile device-metrics override.
- Reset page scale when clearing the override so native browser behavior is restored.
- Keep the existing post-apply observation and rollback behavior.

## Regression

The opt-in real-Chrome viewport qualification now uses a fixture with no viewport meta tag and requests 390×844 at DPR 3. It fails before the repair at the effective-viewport assertion and must pass after the repair, including navigation persistence, target isolation, and clear behavior.

## Implementation notes

- Added mobile page-scale application to normal control and managed reconnect replay, with explicit reset on clear.
- Preserved independent post-apply verification and target-scoped behavior.
- Verified the command contract with the full `krometrail-cdp` unit suite and the no-meta-tag regression against real Chrome.

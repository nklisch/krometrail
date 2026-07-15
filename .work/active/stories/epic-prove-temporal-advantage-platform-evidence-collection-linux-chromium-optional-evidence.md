---
id: epic-prove-temporal-advantage-platform-evidence-collection-linux-chromium-optional-evidence
kind: story
stage: implementing
tags: [testing, browser, infra]
parent: epic-prove-temporal-advantage-platform-evidence-collection
depends_on: [epic-prove-temporal-advantage-platform-evidence-collection-lane-contract-and-shared-runner]
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Collect optional Linux Chromium evidence

## Checkpoint

Exercise the separately labeled Linux Chromium lane without allowing it to satisfy stable Chrome,
macOS, or cross-platform claims.

## Exact implementation

Implement `run_linux_chromium_optional` in `src/app/platform_evidence.rs` using an explicit
`BrowserProduct::Chromium` selection and the shared live qualification authorities. On Linux with
no Chromium installation, write the canonical optional `Skipped` outcome with
`OptionalUnavailable`, a safe reason, and recovery action; every optional row must remain skipped.
If Chromium is observed, retain its own product/protocol/revision and full live qualification
status. A complete failure is `Fail`; incomplete or unavailable evidence is `Inconclusive` rather
than a skip after execution began.

The lane is opt-in, local-only, and does not download a browser or use network fallback. It is
independent of the required Linux Chrome checkpoint and cannot unblock manual interpretation.

## Acceptance evidence

- [ ] Absent Linux Chromium closes as an explicit optional skip with no fabricated measurements.
- [ ] Observed Chromium is separately identified and validated; it cannot be relabeled as Chrome.
- [ ] Complete failure, gap, retention loss, unsupported protocol, and cleanup failure remain
      non-passing and carry recovery records.
- [ ] Optional skip semantics reject mixed skipped/pass rows and do not affect required matrix
      coverage.

## Ordering and blocker

Depends only on the shared lane contract. Its absence is an allowed terminal skip and never blocks
the Linux reference-host, macOS, manual interpretation, or agent-debugging paths.

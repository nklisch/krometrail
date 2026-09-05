---
id: epic-a-grade-reliability
kind: epic
stage: backlog
tags: [agent-ux, browser, storage, visual, testing, infra]
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Bring Krometrail to A-grade operational reliability

## Outcome

Make the normal agent browsing journey easy to complete and recover, preserve trustworthy temporal evidence through serialization and resource turnover, and make distribution/qualification prove the claims they advertise. This is remediation and qualification of the current concept, not a feature-expansion roadmap.

The commissioning assessment was **78/100 (C+) for the reviewed implementation** and **92/100 (A−) for the concept**. These are subjective review judgments, not measured product benchmarks. A future A must be supported by the concrete exit criteria below, not awarded automatically because tickets are closed.

The concept is strong because agents need what happened over time, not only the current page. Preserve source-versus-derived distinctions, provenance, explicit gaps, current-state action outcomes separate from observation quality, inward domain boundaries, and browser-independent temporal analysis. Reduce incidental agent work around selection, arguments, handles, and response detail rather than weakening those authorities.

## Scope and evidence integrity

- Origin: personal read-only review at `eb5b4656`, product 1.6.2 and temporal-vision 0.1.1, plus a recent user-reported local browser/agent incident. Record evidence as point-in-time, not guaranteed current behavior.
- All children remain **backlog**. Priorities and waves organize future work; they do not authorize execution, releases, paid model calls, or a settled technical design.
- Reproduced: actual Rust 1.85 compile failure; release verifier failure; doctor reclaiming abandoned cache; profile reacquisition failure after owner exit without destructors; temporal provenance round-trip loss; nonadjacent source duplicate acceptance.
- Code-traced findings and unmeasured performance/object-lifetime risks retain their stated confidence. An investigation may close with reproducible disconfirming evidence rather than an unnecessary code change.
- Current-compiler format/schema/workspace-test/Clippy and distribution fixture gates passed in review. Those passes did not establish live browser coverage, supported-client result delivery, or the advertised minimum compiler.
- No production code changed during the review or this backlog capture. Existing untracked work and the local stress driver are not review-generated implementation.

## User-reported agent friction to preserve

Chrome launched and loaded the requested site. The first screenshot request omitted required arguments (agent error). A corrected request returned **selected browser page was not found**. **list_pages** exposed only **succeeded**, with no page IDs, preventing recovery. A desktop screenshot showed Chrome running, and the agent fell back to desktop control. No private site content is required for a regression fixture.

Do not collapse this into an established selection bug or an established client bug. Correlate server wire, client-decoded, and model-visible results and target-attachment timing. The response-delivery and page-selection items own the two possible failure surfaces; the agent-journey item verifies successful recovery across them.

## Proposed execution waves

1. **Restore the agent feedback loop:** useful results, fresh same-document observations, page-selection diagnosis/recovery, and accurate clipboard errors. Include existing profile recovery as an early operational priority.
2. **Repair correctness and distribution:** effective compiler gates, coherent release ownership, discovery-only doctor, complete provenance, target/handle retirement, input cleanup, and license distribution. Independent work can proceed alongside wave 1.
3. **Prove sustained operation:** actual-integration journeys, honest live-browser coverage, contention/memory/object-lifetime measurement, and remaining registry lifecycle audit. Qualification harness construction can start early; final qualification follows owning fixes.
4. **Reduce maintenance drag and reconcile docs:** behavior-preserving snapshot/response boundaries and final executable-to-documentation reconciliation. Essential docs/tests still ship with each owning fix.

Waves are sequencing guidance, not extra artificial dependency edges. Frontmatter dependencies name actual prerequisites only.

## Work inventory

| Wave | Priority | Outcome | Evidence status |
| --- | --- | --- | --- |
| 1 | P1 | [Preserve essential results in the agent-visible MCP response](epic-a-grade-reliability-agent-result-delivery.md) | Code-traced response behavior |
| 1 | P1 | [Separate snapshot reference validity from content novelty](epic-a-grade-reliability-snapshot-freshness.md) | Code-traced defect |
| 2 | P1 | [Make the declared minimum Rust version genuinely compile and gate CI](epic-a-grade-reliability-minimum-rust-gate.md) | Reproduced with explicit Rust 1.85 compiler selection |
| 2 | P1 | [Keep independent crate and all plugin versions correct through release](epic-a-grade-reliability-release-version-ownership.md) | Release verifier failure reproduced |
| 2 | P1 | [Keep doctor independent of recording storage and reclamation](epic-a-grade-reliability-doctor-discovery-only.md) | Reproduced in isolated storage: doctor removed abandoned recording evidence and preserved managed profiles. |
| 2 | P1 | [Preserve and validate complete temporal sequence provenance](epic-a-grade-reliability-sequence-provenance.md) | Both silent round-trip loss and nonadjacent duplicate acceptance reproduced through public APIs. |
| 2 | P2 | [Release browser-event capacity when page targets retire](epic-a-grade-reliability-event-target-retirement.md) | Code-traced lifecycle leak |
| 2 | P2 | [Reclaim resolved-range handle entries and byte budget](epic-a-grade-reliability-range-handle-reclamation.md) | Code-traced: entries and budget are never removed/released. |
| 2 | P2 | [Release dispatched keyboard state after errors and cancellation](epic-a-grade-reliability-input-release-cleanup.md) | Code-traced missing cleanup path |
| 1 | P2 | [Classify clipboard failures from the actual CDP result shape](epic-a-grade-reliability-clipboard-error-shape.md) | Code-traced extractor/test mismatch |
| 3 | P2 | [Measure and bound storage interference with browser responsiveness](epic-a-grade-reliability-storage-responsiveness.md) | Risk: synchronous work on the single-threaded runtime is established |
| 3 | P2 | [Account for source materialization in artifact memory limits](epic-a-grade-reliability-artifact-memory-envelope.md) | Risk: admission ordering is code-traced |
| 3 | P2 | [Verify and close the lifecycle of browser remote objects](epic-a-grade-reliability-remote-object-lifetime.md) | Investigation: inspected paths create remote objects without an obvious matching release |
| 3 | P2 | [Make live-browser coverage explicit and continuously exercise critical journeys](epic-a-grade-reliability-live-browser-qualification.md) | Code-traced coverage gap: opt-out early returns can appear as ordinary passing tests. |
| 3 | P2 | [Qualify task completion through the actual agent integrations](epic-a-grade-reliability-agent-journey-qualification.md) | User-reported workflow failure plus a missing end-to-end assurance boundary |
| 1 | P1 | [Diagnose and make page-selection failures recoverable](epic-a-grade-reliability-page-selection-recovery.md) | User-reported incident |
| 4 | P3 | [Split snapshot production responsibilities without changing behavior](epic-a-grade-reliability-snapshot-module-boundaries.md) | Maintainability judgment, not a runtime defect. |
| 4 | P3 | [Split MCP projection responsibilities around the canonical result](epic-a-grade-reliability-response-module-boundaries.md) | Maintainability judgment, not a runtime defect. |
| 2 | P2 | [Ship the declared MIT license text with source and packages](epic-a-grade-reliability-license-distribution.md) | Confirmed repository inventory gap |
| 4 | P3 | [Align operational documentation with verified executable behavior](epic-a-grade-reliability-operational-doc-accuracy.md) | Documentation drift and duplicated instructions observed |
| 3 | P2 | [Audit remaining bounded registries for full retirement semantics](epic-a-grade-reliability-bounded-registry-lifecycles.md) | Category-level investigation motivated by two code-traced capacity leaks |
| 1 | P1 | [Managed profile recovery](idea-profile-lock-fallback-or-recovery.md) — existing item reused as a child | Owner-exit reacquisition failure reproduced; automatic fallback remains a design choice |

## Full review coverage map

| Review finding / improvement | Owning item(s) |
| --- | --- |
| 1. Essential results disappear at integration delivery | `epic-a-grade-reliability-agent-result-delivery` |
| 2. Same document treated as unchanged content | `epic-a-grade-reliability-snapshot-freshness` |
| 3. Broken Rust 1.85 support and ineffective selection | `epic-a-grade-reliability-minimum-rust-gate` |
| 4. Mixed-version release verification blocked | `epic-a-grade-reliability-release-version-ownership` |
| 5. Doctor performs cache cleanup | `epic-a-grade-reliability-doctor-discovery-only` |
| 6. Stale reusable-profile lock | `idea-profile-lock-fallback-or-recovery` (existing, enriched) |
| 7. Sequence serialization loses provenance | `epic-a-grade-reliability-sequence-provenance` |
| 8. Retired targets consume event capacity | `epic-a-grade-reliability-event-target-retirement` |
| 9. Range handle budget never reclaimed | `epic-a-grade-reliability-range-handle-reclamation` |
| 10. Interrupted keyboard sequence skips release | `epic-a-grade-reliability-input-release-cleanup` |
| 11. Clipboard exception nesting misclassified | `epic-a-grade-reliability-clipboard-error-shape` |
| 12. Nonadjacent duplicate source IDs accepted | `epic-a-grade-reliability-sequence-provenance` |
| 13. Antigravity manifest absent from version projections | `epic-a-grade-reliability-release-version-ownership` |
| Risk: synchronous persistence stalls browser runtime | `epic-a-grade-reliability-storage-responsiveness` |
| Risk: encoded-source memory precedes admission | `epic-a-grade-reliability-artifact-memory-envelope` |
| Risk: remote objects lack clear release lifetime | `epic-a-grade-reliability-remote-object-lifetime` |
| Risk: passing tests overstate live execution | `epic-a-grade-reliability-live-browser-qualification` |
| Improvement: agent-visible result is a tested contract | `epic-a-grade-reliability-agent-result-delivery`, `epic-a-grade-reliability-agent-journey-qualification` |
| Improvement: recovery without hidden information | `epic-a-grade-reliability-page-selection-recovery`, existing profile item |
| Improvement: full lifecycle for bounded registries | `epic-a-grade-reliability-bounded-registry-lifecycles` and owning fixes |
| Improvement: serialization property tests | `epic-a-grade-reliability-sequence-provenance` |
| Improvement: cohesive production modules, not arbitrary line limits | `epic-a-grade-reliability-snapshot-module-boundaries`, `epic-a-grade-reliability-response-module-boundaries` |
| Improvement: real mixed-version packaging fixtures | `epic-a-grade-reliability-release-version-ownership` |
| Improvement: actual license text | `epic-a-grade-reliability-license-distribution` |
| Improvement: executable/documentation accuracy and duplicated instructions | `epic-a-grade-reliability-operational-doc-accuracy` |
| Recent local page-discovery / screenshot / desktop-fallback incident | `epic-a-grade-reliability-page-selection-recovery`, `epic-a-grade-reliability-agent-result-delivery`, `epic-a-grade-reliability-agent-journey-qualification` |

## Existing work to reuse, not duplicate

- `idea-profile-lock-fallback-or-recovery` is an existing child, enriched with the reproduced failure and safety/recovery acceptance criteria. Original observations are retained.
- `idea-mcp-locator-ergonomics`, `idea-mcp-scroll-delta-simplification`, and `idea-temporal-range-active-target-defaults` own previously recorded input/default friction. Do not replace these with a second ergonomics epic. The journey qualification measures their impact; select and validate one current ergonomic contract, not a pile of aliases for hypothetical consumers.
- `idea-browser-automated-clipboard-permissions` owns unattended clipboard policy, distinct from wrong error classification. Permission work must not disable browser security or silently grant blanket access. Verify focus, actual permission state, explicit caller authority, and scoped cleanup before selecting a mechanism.
- `perf-scout-profile-artifact-stages` and existing artifact performance candidates provide profiling context. Reuse their benchmark infrastructure; this epic adds correctness of the memory/responsiveness envelope, not duplicate optimization tickets.
- `epic-prove-temporal-advantage` already owns capture, platform, and model-effectiveness qualification, including `epic-prove-temporal-advantage-agent-debugging-qualification`. It remains independently owned and is not reparented or blocked by this epic.

## A-grade exit criteria

- [ ] All confirmed high/medium findings have focused regressions and fixes, or an independently checkable documented disconfirmation against the current contract. No known high-severity workflow or evidence-integrity failure remains unresolved.
- [ ] A clean supported-client journey can start/attach, discover/select a page, take a screenshot, interact, see same-document changes, request temporal evidence, recover from closure/stale selection, and stop without hidden identifiers or an undocumented desktop rescue.
- [ ] Every supported integration has actual model-visible delivery evidence; unsupported/unavailable environments remain explicitly unqualified. Scripted smoke and manually authorized model success are reported separately.
- [ ] Runtime churn, cancellation, abrupt owner termination, slow storage, concurrent large evidence requests, and shutdown preserve bounded resource use and explicit evidence quality. Each investigated risk has measured disposition and durable regression coverage where justified.
- [ ] Source provenance survives supported transfers and transformations; invalid identities, gaps, and omissions never become confident fabricated continuity or stale-content claims.
- [ ] The effective declared-minimum compiler and stable gates pass, and a hermetic release test covers independent crate versions, every plugin projection, rollback, license contents, and exact-version activation without publishing.
- [ ] Documentation and skills describe the actual executable/integration contract; generated artifacts are regenerated through the supported build. Targeted structural cleanup is behavior-preserving and reviewable.
- [ ] Existing input/default ergonomics findings have measured disposition in the real agent journey. Completion is not claimed while agents still repeatedly fail ordinary requests or cannot recover; improvements preserve explicit target authority and privacy.
- [ ] The final assessment distinguishes **operational reliability** from **concept effectiveness**. Reuse `docs/EVALUATION.md` and the existing temporal-advantage epic for measured benefit claims: required capture/control thresholds, transient-identification improvement, stable-control false positives, provenance, sample minimums, and named platform/model boundaries. Missing evidence remains blocked/inconclusive, not an A-grade proof.

## Verification and claim boundaries

Each child records its implementation discoveries, tests, remaining risks, and current reference revision. Final qualification consumes those results plus actual integration evidence; no aggregate numerical score can hide a failed evidence-integrity or recovery gate. A broad final review follows implementation, but this backlog capture does not schedule autonomous execution or paid experiments.

Keep Cargo as the sole product version source, independent temporal-vision release ownership, current-format cache policy, explicit security threat models, and the known-cache-only cleanup boundary. Do not trade these strengths for a superficially smoother happy path.

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
| 1 | P2 | [Classify clipboard failures from the actual CDP result shape](../active/features/epic-a-grade-reliability-clipboard-error-shape.md) | Code-traced extractor/test mismatch |
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

## Backlog assessment and execution topology — 2026-09-05

### Commission and current state

The user requested backlog assessment, commit/push preservation of unretained work, and an execution topology with model assignments. This section is that planning handoff, not authorization to start implementation, launch qualification, publish releases, or spend a model-evaluation budget. It stays with the owning epic rather than creating a competing standalone plan.

Inventory at assessment: **42 backlog files** (this epic, 22 child features, and 19 lightweight ideas), plus **10 nonterminal active items** and **32 active-directory items already marked done**. Parent containers are included in these counts; they are not 52 independent implementation jobs. Lightweight ideas lack full kind/stage metadata and need scope/design before dispatch. The reliability children have useful outcomes and acceptance criteria but no settled implementation designs yet.

The 27 untracked backlog files and `tests/stress_test_driver.py` were retained in commit `90d64eb7` and pushed to `origin/main`. The driver is preserved as exploratory source, not a passing test suite: it has a machine-specific executable, public-site dependencies, unbounded reads, response-shape assumptions, conditional coverage omissions, and no reliable failing exit status. Its module warning makes that boundary explicit. Only syntax was checked; it was not executed. Live-browser qualification should reuse useful scenarios through the existing harness, not promote this driver into CI unchanged.

### Assessment changes to the original waves

1. **Add the existing giant-page session-kill finding to the first critical queue.** `idea-giant-page-transport-session-kill` describes loss of the entire session, not merely an inconvenient observation limit. Give it proposed P1 investigation/remediation priority alongside result delivery and profile recovery. Reproduce on the current revision with a bounded local giant-document fixture before selecting a transport fix. The oversized-message explanation remains a hypothesis, not an established cause. Reuse its existing item rather than duplicating/reparenting it.
2. **Do not reimplement trim signaling.** `idea-trim-signaling-visibility-gaps` maps to completed `story-trim-signaling-visibility` (1.6.2). Current source contains sticky `grace_overridden_through` state and `fully_evicted_range_not_found` with boundary/recovery coverage. Re-run those regressions at execution preflight, then reconcile the duplicate idea. Its small-budget segment-size tuning note remains a separate unmeasured observation.
3. **Run one performance adjudication, not eleven optimization projects.** `feature-perf-scout-adjudication` already owns all 11 `perf-scout-*` candidates. Start with `perf-scout-profile-artifact-stages`; disposition each other candidate from release-build measurements. The feature's mistaken ten-item/five-versus-six counts were corrected during assessment. Memory admission and responsiveness correctness are not optional merely because a throughput optimization is unhelpful.
4. **Retain the independent temporal-advantage program.** Its remaining platform, manual interpretation, and debugging work is substantial, but it is not a second reliability implementation. Validate delivered harness code versus missing run evidence before assigning implementation. A valid Linux reference run does not wait on macOS, and an unavailable macOS row never becomes a pass.
5. **Keep low-value expansion out of the critical path.** The byte-fingerprint transport hook remains dependent on a measured parse bottleneck and a deliberate upstream/fork decision. Descriptor-anchored root reclamation retains its existing low practical severity and same-user threat-model adjudication. Neither justifies delaying ordinary recovery or introducing broad startup guards.
6. **Retire obsolete ergonomic proposals at design, not by adding aliases.** Locator and scroll ideas document real friction but their lists of alternative shorthands are not binding implementation instructions. Choose one current ergonomic wire contract, materialize it into existing domain authorities, and replace obsolete shapes directly. Temporal defaults must select only the actual current session/page and preserve explicit historical selection.

### Execution graph

```text
Preflight: current reproductions, design, baseline gates, source ownership
  |
  +-- A Agent results --> snapshot freshness --> input/default ergonomics ---+
  +-- B Giant-page survival --> selection/profile recovery --> event retire -+
  +-- C Clipboard classification --> input cleanup --> object-lifetime study +
  +-- D Compiler/release/license/doctor correctness -------------------------+
  +-- E Provenance + handle lifetime --> memory/storage measurement & fixes -+
  +-- F Local-fixture qualification harness (can start before fixes) --------+
                                                                            |
                       integrated regressions + registry lifecycle audit <--+
                                      |
                 snapshot/response structural cleanup + final docs
                                      |
                   repeat live and actual-integration qualification
                                      |
                         final independent reliability review

Parallel evidence program, on a pinned integrated revision:
  Linux reference capture --> authorized interpretation --> debugging study
  macOS default-DPI ----+
  macOS high-DPI -------+--> platform matrix (separate from Linux model gate)
  optional Chromium ---+
```

Arrows within a lane generally express recommended scheduling/file ownership, not new frontmatter prerequisites. Actual dependencies remain in item frontmatter; integration/final-qualification joins consume the owning fixes. In particular, provenance and range-handle work are independent units even though they share one lane owner. No new artificial requirement makes all P1 work wait for every other P1.

### Work lanes and planned model ownership

Identifiers below omit the `epic-a-grade-reliability-` prefix only for this epic's children. Existing `idea-*`, `perf-scout-*`, and other epic IDs retain their full names.

| Lane | Work and order | Primary model | Ownership and review boundary |
| --- | --- | --- | --- |
| A — Agent feedback | `agent-result-delivery` → `snapshot-freshness`; then `idea-mcp-locator-ergonomics`, `idea-mcp-scroll-delta-simplification`, `idea-temporal-range-active-target-defaults` | GPT-6 Astra, medium thinking | One writer for MCP response/session presentation and shared wire changes. Correlate server/client/model-visible data first; do not fix a hypothetical client. GLM-5.3-Flash reviews canonical authority, bounded output, and freshness regressions. |
| B — Session survival | `idea-giant-page-transport-session-kill` → `page-selection-recovery` → `idea-profile-lock-fallback-or-recovery` → `event-target-retirement` | GPT-6 Astra, medium thinking | Own target/session supervisor, transport investigation, profile lease, and event retirement integration. Selection investigation may end with disconfirmation if delivery owns the incident. GLM-5.3-Flash independently reviews races, live-browser ownership, and loss accounting. |
| C — Input and browser object lifetime | `clipboard-error-shape` → `input-release-cleanup`; then `remote-object-lifetime` investigation and, if justified, `idea-browser-automated-clipboard-permissions` | GLM-5.3-Flash, xhigh thinking; Astra owns risky lifecycle/permission design decisions | Keep classification separate from permission policy. Coordinate snapshot/object changes with A and B; never edit their shared files concurrently. Astra reviews cancellation cleanup, object ownership, privacy, and scope of permission grants. |
| D — Distribution and composition | `minimum-rust-gate`, `release-version-ownership`, `license-distribution`, `doctor-discovery-only` as separate changes under one owner | GLM-5.3-Flash, xhigh thinking | Own workflows, version projection inventory, packaging, and discovery composition. Serialize shared manifest/workflow edits. Astra reviews effective compiler selection, independent crate ownership, and doctor side effects. No release helper invocation that tags/pushes/publishes during verification. |
| E — Evidence and resource bounds | `sequence-provenance`; `range-handle-reclamation`; `artifact-memory-envelope`; `storage-responsiveness`; existing `feature-perf-scout-adjudication` | GPT-6 Astra, medium thinking | Own public sequence invariants, range capacity, artifact admission, and storage/performance design. Source metadata/reading changes in `recording.rs` have one writer. Profile first; implement only demonstrated optimizations, with explicit disposition for all 11 scouts. GLM-5.3-Flash reviews ordering, serialization, budget/cancellation release, and measurement validity. |
| F — Verification infrastructure and integration | `live-browser-qualification`, `agent-journey-qualification`, then `bounded-registry-lifecycles`; coordinate existing temporal-advantage harness/status work | GLM-5.3-Flash, xhigh thinking; Astra runs integrated review and interprets visual evidence | Harness construction starts early on owned fixture/test files. Final journeys wait for their owning fixes; registry-audit completion follows event/handle fixes. Inventory other registries without presuming defects. Actual client and model-visible evidence cannot be replaced by a Rust structured-content assertion. |
| G — Maintenance after behavior settles | `snapshot-module-boundaries`, `response-module-boundaries`, `operational-doc-accuracy` | GLM-5.3-Flash at xhigh for extraction; the same model at low for bounded doc/link inventory; Astra for final contract reconciliation | Snapshot extraction also waits for any remote-object/transport work touching that module; response extraction waits for A's wire/presentation changes. Preserve deterministic external output, not arbitrary line limits. Essential documentation ships in each owning fix; this is final reconciliation. |

This covers every one of the 22 reliability children. G reuses freed worker slots; it is not an additional standing team. E's performance work reuses the existing active owner rather than creating a parallel benchmark framework. The broader active temporal-advantage epic remains its own outcome authority.

### Concrete model roster

Availability was checked with `list_subagent_models` in this session. The user subsequently selected GLM-5.3-Flash in place of GLM-5.3 and requested xhigh for most Flash work, with lower routine thinking for Astra. These are callable identifiers, not inferred product names or benchmark rankings:

- **Orchestrator and complex owners:** `openai-codex/gpt-6-astra`, low thinking for medium-complexity implementation work, not merely isolated fixes or mechanical changes. Use medium for complex, cross-cutting implementation and design; the medium lane assignments describe those broader responsibilities, not a floor for every subtask. Escalate to high only for unresolved concurrency races, cancellation/ownership questions, or conflicting evidence; no routine xhigh/max. Own scope, cross-lane decisions, high-risk state/provenance work, integration, and final adjudication. This keeps the most context-coupled decisions with the host model rather than redistributing them each wave.
- **Bounded implementation and independent review:** `zai/glm-5.3-flash`, xhigh thinking for implementation and independent review. Own C/D/F and later extractions; review Astra-authored units in a fresh context. Astra reviews GLM-authored units. Model diversity is a review lens, not proof of correctness; tests and reproductions remain decisive.
- **Small inventory exception:** `zai/glm-5.3-flash`, low thinking only for straightforward inventories; xhigh remains the default for substantive work. Enumerate schema consumers, file ownership, documentation links, and checklist coverage. No independent sign-off on concurrency, provenance, permission policy, or destructive filesystem behavior.

Recheck availability before dispatch; do not silently substitute a model or provider. Other available models are unnecessary for this topology. Kimi K3 is not selected without explicit user approval. No implementation subagents were launched to prepare this assessment.

The paid product-effectiveness study is a different roster from coding/review workers: the existing initial lane requires the locally available **Codex CLI**, with a proposed GPT-6 Astra model only if that CLI actually advertises it. Pi availability does not establish CLI availability. Resolve and pin the actual CLI model/version, prompts, sample sizes, and budget before authorization; add an independent visual model family only as a separately approved experiment. Coding-agent review calls are not product-evaluation evidence.

### Dispatch and merge policy

- **At most four implementation workers, one independent reviewer, and the orchestrator.** Begin A, B, D, and E's provenance unit. Fill the first free slots with C and F; read-only fixture/integration inventory may happen earlier, but there is no six-way build swarm. Tune downward if measured RAM/disk headroom requires it.
- **One worktree per active owner, one item per reviewable commit series.** Start each from the latest integrated commit, declare the intended file set before editing, and report concrete overlaps to the orchestrator. A lane is an ownership queue, not a weeks-long divergent branch. Merge completed, verified units continuously.
- **Serialize hotspots explicitly:** `response.rs`/MCP session/schema with A; target/session/profile lifecycle with B; keyboard/clipboard with C; Cargo/lock/workflows and `app.rs` composition with D; `recording.rs`, artifact service, and range handles with E. Transfer ownership for cross-cutting edits rather than having two workers “resolve it at merge.” Foundation docs, generated schemas, and final reconciliation have one integration owner.
- **Resource isolation:** per-worker temporary browser profiles, stores, and output directories. Honor the repository's real-browser lock. Run one heavy Rust build or benchmark per host initially; compilation concurrency is separate from agent concurrency. Do not benchmark while other builds/soaks are distorting the host.
- **Build cleanup:** every isolated/custom Cargo target directory created for the work has an owner and is deleted on completion, integration, cancellation, or abandonment. Preserve authorized local evaluation evidence outside a build directory before retiring that directory; do not commit raw runs.
- **Review loop:** current reproduction → proportionate design → regression/fault tests → implementation → focused gates → fresh-context cross-model review → accepted-finding fixes → orchestrator integration gates. A reviewer cannot approve their own implementation, and an unresolved failure never becomes a done item.
- **No blind backlog drain:** speculative items can close with reproducible negative measurements. Proven defects require a fix/regression or a checkable disconfirmation. Missing hosts/clients/authorization stay explicit blockers, not inferred successes.

### Verification joins and completion boundary

1. **Per unit:** exercise the actual production boundary and the failure path that motivated the item. Provenance needs sparse round-trip/generative coverage; profile/input/event/handle work needs abrupt exit, cancellation, replacement, and churn; result delivery needs the actual supported integration surface; distribution uses hermetic packaging/rollback fixtures.
2. **After each integration wave:** format, wire-schema check, locked workspace/all-targets check and tests, Clippy with warnings denied, distribution fixtures where relevant, and explicitly selected declared-minimum Rust gates after D fixes their authority. Record the effective compiler and revision. Existing review gate passes are historical evidence, not a new pass for these changes.
3. **Before structural work:** stabilize corrected behavior with differential/golden fixtures. Re-run affected browser/integration tests after extraction; moving code cannot erase the fix's proof.
4. **Final operational qualification:** full supported journey, same-document mutation, giant-page survival, closed/replaced target recovery, stale-profile ownership, interrupted input, >64-target churn, range capacity turnover, slow storage, concurrent large artifact requests, remote-object soak, and shutdown. Track executed/skipped/blocked/failed/inconclusive separately; preserve data needed to recompute results locally.
5. **Separate thesis report:** Linux reference capture → manually authorized interpretation → agent debugging. macOS default/high-DPI and optional Chromium feed their own matrix. Apply existing sample minimums and thresholds; no software-release claim of measured temporal benefit without the actual evidence.
6. **Closure:** reconcile covered/stale ideas and done-tier clutter through the current retention rules, record each remaining limitation, regenerate docs through `bun run docs:build`, and perform a final independent review against this epic's A-grade exit criteria. Do not award an A merely because item statuses are green. No tag, publish, or deployment is part of this topology request.

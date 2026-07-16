---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-corpus
kind: story
stage: done
tags: [testing, visual, browser]
parent: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Commit the deterministic benchmark target and hidden ground truth

## Checkpoint

Create the dependency-free static browser target and its one current benchmark-definition
input. The target is a standalone application under `tests/fixtures/browser/`, not a product
runtime or framework-state fixture. The definition is the evaluator-owned ground truth and is
loaded by the contract crate, but is never included in agent-facing visual-condition inputs.

## Exact target contract

The target is `tests/fixtures/browser/temporal-benchmark/` with only committed local assets:
`index.html`, `benchmark.js`, `benchmark.css`, and `README.md`. It has no package manager,
network request, external font, current-time wall-clock input, random source, or framework
runtime. The harness selects one case with `index.html?case=<case_id>&duration_ms=<integer>` and
clicks the visible case anchor. A run starts at the first accepted click and uses
`performance.now()` plus `requestAnimationFrame` for the visual loop. The requested duration is
the intended visible interval; capture evidence must report observed timing separately and must
not claim that a browser presented every intended interval.

The fixed qualification viewport is 800x450 CSS pixels at device scale 1 unless a later live
configuration explicitly declares another value. Each case resets before the anchor is enabled.
The target exposes no ground-truth endpoint and does not render state labels that would reveal the
defect mechanism. It may expose stable interaction anchors and normal visible content needed to
operate the case. Ground truth lives in the committed definition, not in Krometrail measurements.

The canonical cases and phase identities are:

| Case ID | Family / intent | Exact behavior and timing identity |
| --- | --- | --- |
| `movement-reversal/basic` | movement reversal / defect | At `t=0` the panel is at x=48; after a 100 ms lead-in it moves toward x=288, reverses through x=160→120 for exactly `duration_ms`, corrects toward x=288 for 100 ms, then remains at x=288. Phases are `baseline`, `forward`, `reversal`, `correction`, `stable`; the affected region is the panel's declared viewport rectangle. |
| `flicker/visibility` | flicker / defect | At 100 ms the status card becomes absent for `duration_ms`, then returns with the same final geometry and text. Phases are `baseline`, `incorrect-visibility`, `stable`. |
| `flicker/color` | flicker / defect | At 100 ms the status card changes from neutral to the committed incorrect red style for `duration_ms`, then returns to neutral. Text and geometry remain stable. |
| `flicker/text` | flicker / defect | At 100 ms the status text changes from `Ready` to the incorrect `Stale data` text for `duration_ms`, then returns to `Ready`. Geometry remains stable. |
| `layout/width` | transient layout / defect | At 100 ms the content column changes from width 640 to width 480 for `duration_ms`, then returns to width 640. The panel's final text and position are unchanged. |
| `layout/content-shift` | transient layout / defect | At 100 ms a fixed-height 48 px notice is inserted above the content for `duration_ms`, moving the content down by 48 px, then removed. |
| `layout/scroll-position` | transient layout / defect | At 100 ms the scroll container jumps from scroll position 0 to 160 px for `duration_ms`, then returns to 0. The final viewport is unchanged. |
| `dom-opaque/path-reversal` | DOM-opaque motion / defect | A canvas marker moves from x=80 toward x=320, reverses to x=240 for `duration_ms`, corrects to x=320, and remains there. The canvas has no actionable DOM representation. |
| `dom-opaque/teleport` | DOM-opaque motion / defect | At 100 ms the canvas marker is drawn at x=520 instead of its expected x=320 for `duration_ms`, then returns to x=320. |
| `dom-opaque/sprite` | DOM-opaque motion / defect | At 100 ms one canvas object is drawn with the incorrect committed color and shape for `duration_ms`, then returns to the final sprite. |
| `stable/smooth-panel` | stable control / intentional | The same panel moves monotonically from x=48 to x=288 over `duration_ms`; it has no reversal or incorrect state. |
| `stable/loading-indicator` | stable control / intentional | A labeled loading indicator rotates and changes its expected progress presentation for `duration_ms`, then settles at the final ready state. Every change is declared intentional. |
| `stable/caret` | stable control / intentional | A text field's caret blinks at the committed 500 ms cadence while the surrounding state remains stable; `duration_ms` controls only the observation window. |

The definition records every phase's half-open interval, state ID, expected final state, defect
interval (when any), affected viewport region, anchor ID, duration rule, and final-state rule.
The duration sweep is exactly `[16, 33, 50, 100, 200]` milliseconds; 16 ms is named
`frame_approx`, not an exact display-frame guarantee. Defect cases use 100 ms lead-in and 100 ms
settle intervals. Stable controls are run through the same matrix but remain intentional.

## Acceptance evidence

- [x] The target renders all canonical case IDs with no network or framework dependency and the
      route cannot select an undeclared case or duration.
- [x] The committed definition is canonical UTF-8 JSON, has ordered fixture file identities and
      SHA-256 digests, and validates against the generated definition schema.
- [x] A test proves the phase boundaries and final state declared in the definition agree with
      the fixture's case registry; no ground-truth value is read from a Krometrail frame or
      artifact.
- [x] Tests prove the target uses no wall-clock or random source and that each reset/run starts
      from the same baseline.
- [x] The definition's defect/control and family registries are exhaustive and have no duplicate
      case IDs, phase IDs, anchors, or duration values.

## Implementation notes

- **Execution capability**: inline feature-owning worker with direct-read integration mapping; the
  story is a cohesive fixture/contract slice and does not need delegated ownership.
- **Review weight**: standard, inherited from project default; child stories advance directly to
  `done` after verification and do not enter review.
- **Files changed**: added the `temporal-evaluation` workspace crate and corpus contract,
  generated-definition tool, canonical definition/schema, fixture target assets, and focused
  contract tests; updated `Cargo.toml`/`Cargo.lock` for the workspace member.
- **Tests added**: nine Rust contract tests cover canonical loading, generated schema equality,
  SHA-256 fixture identity, exact case/phase/duration/final-state invariants, invalid edits,
  static no-network/no-wall-clock/no-random checks, reset ordering, neutral agent-facing markup,
  and canonical JSON rejection; `node --check` validates the fixture script.
- **Simplification**: kept the target dependency-free and local, with no launcher, server,
  framework instrumentation, product command, model client, manifest, or compatibility alias.
- **Discrepancies from design**: none; the initial contract crate is limited to corpus/definition
  types and does not implement later prompts, conditions, run manifests, or CI output handling.
- **Adjacent issues parked**: none.
- **Verification**: Rust 1.85 `cargo fmt --all -- --check`, locked workspace check/test/clippy,
  plus fixture-specific static checks all pass.

## Ordering

This checkpoint must finish before the matrix/prompt story can refer to case IDs or before any
manifest can hash fixture inputs. Use the existing browser fixture boundary and existing
`ChromeWrapper`, lock, and local static-server conventions later; do not add a browser launcher,
benchmark product command, framework instrumentation, or a second fixture runtime.

# Krometrail Evaluation

## Purpose

Krometrail is valuable only if it captures temporal defects and presents them in forms that coding agents can understand.

Evaluation tests four separate claims:

1. Chrome capture preserves the relevant visible evidence.
2. Temporal artifacts communicate that evidence accurately.
3. Agents debug transient defects more effectively with temporal evidence than with current-state screenshots alone.
4. Recording and analysis remain reliable enough not to invalidate the observation.

A passing artifact cannot compensate for missing source frames. A technically complete recording cannot compensate for artifacts that models misinterpret. Each layer is evaluated independently.

## Evaluation Principles

Evaluation is:

- comparative rather than anecdotal;
- repeatable across model and browser versions;
- explicit about capture gaps;
- blind to fixture implementation during visual interpretation;
- separated into observation and root-cause diagnosis;
- measured against current-state and uniform-sampling baselines;
- resistant to selecting only successful examples.

No visual artifact is described as effective until it demonstrates improvement in the benchmark.

## Benchmark Corpus

The benchmark contains deterministic local applications with known temporal behavior.

### Movement reversal

A panel begins moving toward its final position, reverses briefly, and then settles at the expected destination.

The final screenshot is visually correct.

The fixture varies:

- reversal distance;
- reversal duration;
- transition duration;
- visual complexity behind the panel;
- text and non-text panel content.

### Flicker

A component briefly displays an incorrect visual state before returning to the intended state.

Variants include:

- visibility flicker;
- color or style flicker;
- incorrect text;
- loading-state replacement;
- repeated mount-like flashes.

The incorrect state lasts for controlled durations.

### Transient layout change

A page briefly uses incorrect geometry before stabilizing.

Variants include:

- width changes;
- content shifts;
- delayed font or style application;
- hydration-like replacement;
- scroll-position jumps.

### DOM-opaque motion

A canvas or WebGL-style surface renders a motion defect that has no useful DOM representation.

Variants include:

- path reversal;
- object teleportation;
- one-frame incorrect sprite or color;
- unstable camera or viewport movement.

### Stable controls

Control fixtures contain legitimate motion and visually stable interactions.

They measure whether temporal artifacts or agents invent defects when none exists.

Examples include:

- smooth panel transitions;
- expected loading indicators;
- caret blinking;
- intentional looping animation;
- ordinary scrolling;
- video-like changing content.

## Ground Truth

Each fixture owns a machine-readable ground-truth timeline that is unavailable to the evaluated agent.

The timeline records:

- intended interaction anchor;
- visible-state identifiers;
- expected state intervals;
- geometry or pixel-region expectations;
- known defect interval;
- final stable state;
- whether the behavior is defective or intentional.

Fixtures visibly encode a frame or state identifier where doing so does not trivialize the agent task. This allows capture tests to determine which states reached the recorded frame stream.

Ground truth describes fixture behavior. Krometrail measurements are not reused as benchmark truth.

## Capture-Fidelity Evaluation

Capture fidelity is evaluated before artifact quality.

For each fixture and duration setting, the harness records:

- Chrome-rendered state intervals;
- Krometrail source frames;
- source and receive timestamps;
- observed frame cadence;
- captured state identifiers;
- declared gaps;
- CPU, memory, and disk activity.

### Duration sweep

Brief states are tested at controlled durations including:

- approximately one display frame;
- 33 milliseconds;
- 50 milliseconds;
- 100 milliseconds;
- 200 milliseconds.

Results report capture probability at each duration rather than collapsing all durations into one score.

The baseline supported envelope requires:

- states visible for at least 100 milliseconds appear in a source frame in at least 95% of runs;
- states visible for at least 50 milliseconds appear in a source frame in at least 80% of runs;
- every known ingestion loss is represented by a capture-gap record;
- the system makes no guarantee for durations beneath the demonstrated envelope.

If CDP screencasting cannot meet this envelope on supported systems, the capture mechanism does not support the product claim and requires redesign.

### Motion sequences

Movement fixtures require enough distinct source frames to show the path rather than only its endpoints.

A reversal fixture passes capture validation when the recorded source sequence contains:

- a pre-motion state;
- multiple forward-motion states;
- at least one state evidencing the reversal;
- subsequent correction;
- the final stable state.

A final-state screenshot alone cannot satisfy this criterion.

### Timing integrity

Capture timing passes when:

- source and observed times are preserved separately;
- frame ordering is deterministic through session time and Krometrail capture ordinals;
- every known ingestion or lifecycle loss is visible as an explicit gap, without inferring unknown loss from Chrome acknowledgement tokens or ordinal arithmetic;
- wall-clock changes do not reorder session observations;
- action anchors resolve to the same normalized range across repeated queries;
- no artifact crosses a declared gap without displaying it.

## Artifact Evaluation Conditions

Every agent-facing visual task is run under controlled evidence conditions.

### Condition A: final screenshot

The agent receives only the final post-action screenshot and ordinary current page snapshot.

This represents current-state browser inspection.

### Condition B: uniform storyboard

The agent receives a fixed number of uniformly sampled source frames.

This tests whether any multi-frame contact sheet is sufficient.

### Condition C: change-aware storyboard

The agent receives the same maximum number of frames selected by Krometrail’s storyboard algorithm.

This tests the value of representative-frame selection.

### Condition D: temporal bundle

The agent begins with the default anchor-epoch bundle and receives:

- before/during/after composite;
- change-aware storyboard;
- temporal difference map;
- capture summary;
- source-frame references.

This tests the complete compact temporal experience.

Multi-epoch fixtures also exercise explicit `epochs: "all"`. Evaluation verifies that the default bounds artifact work to one deterministic anchor/nearest epoch without losing full-range or gap provenance, while explicit all-epoch requests preserve original epoch identities across geometry transitions.

### Condition E: progressive source access

The agent begins with the temporal bundle and can request source frames or a region filmstrip.

This tests the complete debugging workflow rather than one-shot interpretation.

Conditions use the same captured source interval. They differ only in presentation and permitted retrieval.

### Optional video conditions

When both a qualified local encoder and an explicitly identified model host that accepts video are available, the same interval is additionally evaluated under:

- **Condition F: real-time MP4/H.264 clip** — relative source timing is preserved within declared bounds;
- **Condition G: model-optimized MP4/H.264 clip** — meaningful selected states may be held longer, with the exact presentation mapping retained in provenance.

Video conditions are compared with the change-aware storyboard, temporal bundle, and progressive-source conditions. Encoder absence, unsupported host attachment, or a model without declared video input makes these rows not applicable; it does not fail the still-first product or permit a substituted pass. Every result identifies the host, provider, model version, FFmpeg build, H.264 encoder, presentation policy, source interval, gaps, and output hash.

## Visual Interpretation Tasks

Agents answer structured questions before seeing application source code.

Tasks include:

- Did any visible state differ temporarily from the final state?
- Describe the order of visible states.
- Identify the affected screen region.
- Did an object move monotonically toward its destination?
- Did content appear, disappear, and reappear?
- Was the observed change local or page-wide?
- Which claims cannot be established because of capture gaps?
- Is the behavior defective, intentional, or uncertain?

Answers are scored against fixture ground truth.

## Debugging Tasks

After the visual interpretation stage, the coding agent receives the fixture repository and normal development tools.

The agent must:

1. reproduce the behavior;
2. identify the visible temporal defect;
3. locate the responsible application logic or styling;
4. apply a focused correction;
5. repeat the interaction;
6. verify both final-state and temporal behavior.

The benchmark records whether the patch removes the defect without removing intentional behavior.

Krometrail does not need to identify the root cause automatically. The evaluation asks whether its evidence helps the agent do so.

## Scoring

### Capture metrics

- defect-state capture rate;
- frame cadence distribution;
- inter-frame gap distribution;
- declared versus detected loss;
- timestamp-ordering errors;
- stored bytes per minute;
- CPU and memory overhead.

### Artifact metrics

- temporal-state recall;
- temporal-order accuracy;
- affected-region accuracy;
- direction or reversal accuracy;
- false-defect rate on stable controls;
- uncertainty calibration when gaps exist;
- source-frame retrieval accuracy.

### Agent metrics

- correct visual diagnosis rate;
- correct root-cause identification rate;
- successful patch rate;
- regression-free verification rate;
- tool calls to diagnosis;
- elapsed time to diagnosis;
- model input devoted to visual evidence;
- requests for raw source frames.

## Product-Thesis Assessment

The temporal product thesis is supported for an evaluated agent when, across the benchmark:

- temporal evidence improves correct identification of transient defects by at least 25 percentage points over final-screenshot inspection;
- the improvement occurs for movement, flicker, and layout defects rather than one fixture family alone;
- the temporal bundle performs at least as well as uniform storyboards while using no more source-frame tiles;
- stable-control false positives do not increase by more than 10 percentage points;
- agents can trace every accepted visual claim to retained source evidence.

These thresholds produce a reportable pass, fail, or inconclusive assessment; they do not block a software release. A release identifies the measured outcome and does not claim validated improvement when thresholds are unmet. Results apply only to the evaluated agent family until another independently developed family reproduces them.

## Artifact-Specific Evaluation

### Storyboard

The storyboard is evaluated for:

- inclusion of the defective state;
- correct chronological ordering;
- readable labels;
- sufficient page context;
- advantage over uniform sampling;
- avoidance of redundant frames.

### Difference map

The difference map is evaluated for:

- localization of changed regions;
- distinction between repeated and one-time change;
- correct interpretation of frequency and timing legends;
- resistance to encoding noise;
- false emphasis from caret, scrolling, and legitimate animation.

### Region filmstrip

The filmstrip is evaluated for:

- crop consistency;
- readability at the output scale;
- preservation of surrounding context;
- explicit handling of region drift or loss;
- improvement for small localized defects.

### Motion history

Motion history is evaluated for:

- path visibility;
- repeated-traversal visibility;
- temporal legend comprehension;
- text-smearing and overlap failure;
- false direction inference.

An artifact that consistently harms interpretation is removed from the default bundle even if its implementation is technically correct.

### Temporal video

Temporal video is evaluated for:

- temporal-state recall after provider-side video sampling;
- temporal-order accuracy relative to the source manifest;
- whether the real-time and model-optimized policies preserve or distort agent interpretation;
- explicit comprehension of gap slates and held-frame timing;
- advantage or regression against still-image conditions at comparable task scope;
- bounded generation time, output size, cancellation, and cleanup; and
- correct omission of the MCP tool when startup qualification fails.

Video results are model-, host-, encoder-, and policy-specific. They do not establish that video is generally superior to compact still artifacts.

## Browser-Control Evaluation

Temporal evaluation assumes the agent can operate the browser reliably.

The control benchmark covers:

- browser launch and attachment;
- page creation, selection, and closure;
- navigation, reload, back, and forward;
- accessibility snapshots;
- click, fill, typing, key input, selection, hover, drag, and scroll;
- dialogs and file upload;
- waiting;
- page evaluation;
- batched actions;
- post-action screenshots;
- transient-reference invalidation.

Control tests include:

- dynamic DOM replacement;
- overlays;
- scrolling targets;
- high-DPI displays;
- shadow DOM;
- iframes;
- canvas fallback;
- stale references;
- navigation during interaction.

The common static and moderately dynamic benchmark requires at least 95% successful action completion. Failures must be explicit; a silent no-op is scored as a failure.

Every state-changing standalone action must return a valid live observation or a structured explanation of why observation failed.

## MCP interoperability qualification

Default tests exercise the actual executable with modern discovery and both supported legacy
initialization versions. Raw-wire assertions cover required result/cache fields, complete catalogue
modern pagination and complete single-response legacy inventory, malformed metadata, unsupported versions, unknown methods, concurrent request IDs,
EOF and output backpressure. Official-SDK clients separately qualify typed decoding and per-Peer
catalogue caching. Schema tests compile the public contracts with a JSON Schema validator.

Opt-in Linux/macOS qualification uses real Chrome and a disposable local fixture to verify image
and artifact decoding, retained reads after stop, cancellation isolation and managed-process cleanup.
Receipts identify the exact candidate revision, platform, protocol and executable version. Native
Claude Code/Codex sessions additionally qualify model-visible structured and image delivery; a raw
wire pass is not a claim about a host's renderer or resource UI. Installation and fresh-session
activation are reported separately. Staged release assets must pass discovery, legacy initialization,
all-page listing and bounded EOF probes before publication.

## Storage and Retention Evaluation

Retention tests run with deliberately small budgets to force segment rotation and eviction.

They verify:

- usage remains within the configured budget after bounded cleanup overhead;
- oldest unpinned segments are removed first;
- pinned ranges remain readable;
- artifacts retain valid provenance or are removed with their source data;
- an all-pinned budget pauses capture without deleting protected data;
- status reports the correct retained range;
- crash recovery restores complete records and removes incomplete trailing writes;
- deletion removes all data belonging to the selected session.

Disk accounting tolerates at most one open segment beyond the configured budget while recording. The open-segment bound is reported.

## Performance Evaluation

Performance is measured on Linux and macOS at declared viewport sizes and capture settings.

The harness reports:

- browser baseline frame timing without Krometrail;
- frame timing while recording;
- daemon CPU and memory;
- encoded frame throughput;
- ingestion queue depth;
- disk write throughput;
- dropped-frame count;
- temporal-query latency;
- peak decoded-frame memory;
- action latency.

The system does not hide overhead behind an aggregate score.

For a two-second 1080p range under ordinary local load:

- a cached temporal bundle returns within one second;
- an uncached storyboard and difference map return within five seconds;
- artifact generation does not interrupt frame ingestion;
- steady-state memory remains bounded independently of session duration.

Performance measurements identify the exact browser, operating system, hardware, viewport, image format, quality, and frame count.

## Cross-Platform Evaluation

Required browser, capture, control, storage, and artifact tests run on:

- Linux with a current stable Chrome or Chromium build;
- macOS with a current stable Chrome build, including a high-DPI display configuration.

Results identify the tested browser versions. Unsupported protocol behavior fails the compatibility probe rather than producing partial silent operation.

## Model Evaluation Discipline

Paid multimodal-agent evaluations are invoked manually through the locally available Codex CLI. Deterministic capture, artifact, control, storage, and scoring checks run without paid agent calls and can execute in CI. Additional independent model families can contribute separate evidence but are not required for the initial assessment.

Every model run records:

- provider and model identifier;
- model version or dated alias;
- invocation date;
- system and task prompts;
- available tools;
- evidence condition;
- artifact parameters;
- token or image usage where available;
- raw answer;
- score and scoring rationale.

Fixture order and evidence condition are randomized. The evaluator does not reveal the fixture label or defect mechanism before the interpretation response.

The same model receives equivalent prompts across conditions. Failed or ambiguous runs remain in the reported result set.

Model-specific success does not establish general visual comprehensibility. Claims identify the models and artifact versions actually tested.

## Reproducibility

Benchmark definitions, fixtures, prompts, schemas, scoring rules, and harness code are versioned in Git. Per-run manifests, source frames, generated artifacts, raw model answers, transcripts, and aggregate results remain local evaluation outputs and are not committed.

Generated artifacts are reproducible from local run manifests and retained source frames. Every local result identifies the exact Git revision containing its benchmark definitions.

An evaluation result identifies:

- Krometrail revision;
- temporal visual algorithm versions;
- fixture revision;
- browser version;
- operating system;
- model configuration;
- benchmark configuration.

Foundation claims follow demonstrated results. They do not exceed the tested capture envelope, platforms, artifact types, or model families.

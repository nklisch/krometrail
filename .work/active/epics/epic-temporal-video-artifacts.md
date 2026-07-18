---
id: epic-temporal-video-artifacts
kind: epic
stage: done
tags: [visual, agent-ux, infra, security, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Optional temporal video artifacts

## Brief

Add an optional MP4/H.264 presentation of retained Krometrail source frames for agent hosts and models that can consume video. Video remains a derived local artifact over the existing resolved-range, source-frame, retention, provenance, and resource contracts. It does not create a second recorder, replace the still-first debugging workflow, capture audio, or upload evidence to a model provider.

Krometrail never downloads, bundles, or redistributes FFmpeg. At MCP startup the composition root qualifies a user-installed `ffmpeg` executable and a supported H.264-to-MP4 path. The capability and its tool are registered only after a bounded encode probe succeeds. Missing or unsuitable FFmpeg leaves browser control, recording, and still-image artifacts fully available and produces a safe local diagnostic rather than a startup failure or dead placeholder tool.

The shipped Krometrail skill must explain why the video tool may be absent, how the user can enable it with their own FFmpeg installation, and how to use returned local resources. When the agent already knows that its host and selected model accept video, the skill may recommend the video presentation; Krometrail must not claim it can infer model capabilities that the host does not expose.

## Foundation references

- `docs/VISION.md` — still-first product thesis and optional video boundary
- `docs/SPEC.md` — conditional capability, public artifact behavior, local-data boundary, and degraded operation
- `docs/ARCHITECTURE.md` — injected encoder port, startup qualification, subprocess isolation, cache identity, and MCP registration
- `docs/VISUAL-EVIDENCE.md` — clip policies, timing provenance, gap slates, and determinism boundary
- `docs/EVALUATION.md` — optional video conditions and model/host/encoder-bounded claims
- `plugin/skills/krometrail/SKILL.md` — installed agent guidance and capability discoverability

## Design decisions

- **User-installed encoder only.** Krometrail owns neither FFmpeg acquisition nor redistribution. The implementation must not enable any dependency feature that downloads FFmpeg and must not place FFmpeg binaries in release assets or plugin packages.
- **Narrow licensing posture.** Keeping FFmpeg out of Krometrail's artifacts avoids taking on FFmpeg binary redistribution as part of this product, but it is not described as eliminating every licensing consideration. Dependency licenses, process-boundary assumptions, notices, and the supported encoder contract still receive release review; the user's FFmpeg installation remains outside Krometrail's managed installation.
- **Conditional MCP surface.** Introduce `temporal-video` as a runtime-qualified capability depending on `temporal-vision`. Register its tool only when startup qualification proves that the resolved executable can produce the supported MP4/H.264 contract. If that executable later disappears or changes, the advertised tool returns a stable structured encoder-unavailable failure with recovery guidance.
- **Derived artifact, not capture.** Video generation consumes the existing resolved range and authoritative retained source frames. Known gaps become labeled gap slates, incompatible visual epochs remain explicit, and no motion interpolation invents unseen state.
- **Two bounded presentation policies.** Real-time presentation preserves relative recorded timing within limits. Model-optimized presentation may hold selected meaningful states so sparse provider sampling is less likely to miss them; the manifest exposes every hold and mapping so it cannot be mistaken for observed duration.
- **Provider-neutral local output.** Return a retained `video/mp4` resource plus a machine-readable manifest. Krometrail does not upload, attach, or adapt the clip for Gemini, Kimi, or another provider; that remains the MCP host's responsibility.
- **External encoder provenance.** Cache and manifest identity include the exact FFmpeg build, selected H.264 encoder, adapter version, arguments, presentation plan, and output limits. The deterministic guarantee applies to the presentation plan; encoded byte equality is claimed only for the exact qualified encoder identity, never across arbitrary installations.
- **Narrow subprocess boundary.** The production adapter invokes the executable directly with allowlisted arguments, not through a shell. It bounds input/output, concurrency, stderr, CPU time, deadline, cancellation, process-tree cleanup, and atomic publication. Logs contain no frame pixels, browser content, raw command input, or unredacted local paths.
- **No UI work.** This epic adds an MCP capability, retained resources, diagnostics, evaluation fixtures, and skill guidance. It has no human screen or browser UI and needs no mockups.
- **Capability-shaped decomposition.** The work is split into deterministic clip semantics, the external encoder boundary, retained generation/storage, and the conditional agent surface. A crate-by-crate or test-only split was rejected because each child must deliver a coherent behavior boundary and carry its own verification.
- **Direct process adapter.** The initial production adapter uses `tokio::process::Command` directly. A wrapper crate does not earn its additional dependency and download-feature risk for one allowlisted operation; native FFmpeg bindings would also expand build and distribution obligations without improving the user-installed-executable contract.
- **Qualification by produced contract.** Startup does not trust encoder names or `ffmpeg -version` alone. The adapter selects only from a versioned allowlist, performs a bounded encode, and validates that the result is an MP4 containing H.264 video. The exact initial encoder order and numeric ceilings remain reversible feature-level contract choices, but there is no generic codec auto-selection or unbounded caller override.
- **Both policies ship together.** Real-time and model-optimized clips share one versioned deterministic presentation plan. Shipping only real-time first was rejected because sparse model sampling is the reason to offer video, while adding an unproven second timing surface later would produce two rounds of public-contract work.
- **Typed video provenance without polluting the visual crate.** Video gets a typed manifest variant/envelope that records presentation and external-encoder identity while reusing common artifact/source identities. The still-only `temporal-vision` manifest remains browser- and process-independent; storage and MCP resource paths generalize additively so existing image artifacts remain readable.
- **One startup availability snapshot.** The composition root resolves the conditional capability once before MCP router construction and injects both the qualified encoder and its availability result. Runtime disappearance becomes a stable tool error; availability does not silently change until restart.
- **Provider-neutral guidance.** The generic local `video/mp4` resource is the only runtime output. Host/model-specific advice stays conditional in the installed skill and references; Krometrail does not add provider upload adapters or claim host capability discovery.

## Capability boundary

The epic owns the public clip request and resource shapes, runtime availability projection, encoder qualification, encoding service/port, retained artifact and manifest integration, cancellation and failure behavior, plugin-skill discoverability, and deterministic plus opt-in live qualification. It should extend the existing capability and artifact registries rather than introduce parallel tool lists or a second storage index.

The selected process boundary is direct `tokio::process::Command`: the contract is one narrow encode operation with strict lifecycle control, so a wrapper crate does not justify another dependency or accidental download surface. Native FFmpeg bindings and managed binary acquisition remain outside this epic.

## Acceptance boundary

- MCP startup succeeds with no FFmpeg, hides every temporal-video tool, and records one bounded actionable capability diagnostic.
- Startup exposes the video tool only after a real bounded MP4/H.264 encode probe, not a version-only check.
- A valid request emits a playable local MP4, sidecar provenance, correct source/time mapping, visible gap handling, declared policy, bounded resources, and retained-resource cleanup behavior consistent with other artifacts.
- Cancellation, timeout, encoder exit, invalid output, vanished executable, output overflow, and store failure leave no child process, partial published artifact, or ambiguous success.
- Still-image generation and frame ingestion remain available and responsive during probe and encoding failures.
- The installed skill accurately explains absent and available states without claiming automatic model-capability detection.
- Installing or replacing FFmpeg after MCP startup requires restarting the Krometrail MCP server so its advertised tool list and qualified encoder identity cannot drift silently.
- Deterministic tests require no FFmpeg or network; opt-in live qualification names the exact executable/build/encoder and cannot silently pass when unavailable.

## Simplification opportunity

Generalize capability registration from static enablement to one registry-owned runtime availability result, then reuse the existing resolved-range, artifact publication, resource, retention, and diagnostics boundaries. Keep external-process concerns in one adapter instead of spreading FFmpeg checks through MCP handlers, the visual crate, or skill-only workarounds.

## Decomposition

The design uses one shared deterministic contract as the root, then lets the security-sensitive FFmpeg adapter and retained artifact service proceed independently against that contract. The final agent-surface feature joins both implementations at the composition root and owns conditional MCP registration plus shipped guidance. A monolithic video feature was rejected because it would couple process lifecycle, storage migration, wire contracts, and prose in one oversized review boundary; layer-only features were rejected because none would deliver a complete capability.

### Child features

- `epic-temporal-video-artifacts-clip-contracts` — deterministic presentation policies, typed provenance, limits, gap/epoch semantics, cache identity inputs, and the injected encoding contract — depends on: `[]`
- `epic-temporal-video-artifacts-ffmpeg-runtime` — user-installed executable discovery, bounded real encode qualification, and cancellation-safe direct process adapter — depends on: `[epic-temporal-video-artifacts-clip-contracts]`
- `epic-temporal-video-artifacts-retained-generation` — resolved-range planning, frame adaptation, encoding orchestration through the injected port, additive artifact persistence, cache validation, and retained video/manifest reads — depends on: `[epic-temporal-video-artifacts-clip-contracts]`
- `epic-temporal-video-artifacts-agent-surface` — startup availability projection, conditional registry-derived MCP tool/resources, stable runtime failure mapping, installed-skill discoverability, and end-to-end qualification — depends on: `[epic-temporal-video-artifacts-ffmpeg-runtime, epic-temporal-video-artifacts-retained-generation]`

### Simplification arcs

- `epic-temporal-video-artifacts-clip-contracts` keeps one typed presentation/provenance model and reuses the existing range, frame, gap, epoch, cancellation, and artifact identities instead of creating a second timeline vocabulary.
- `epic-temporal-video-artifacts-ffmpeg-runtime` keeps all executable discovery and child-process authority in one adapter and avoids downloader features, native bindings, shell invocation, and duplicate lifecycle helpers.
- `epic-temporal-video-artifacts-retained-generation` generalizes the existing image-only artifact publication/read validation additively rather than creating another SQLite index, retention policy, URI authority, or deletion path.
- `epic-temporal-video-artifacts-agent-surface` projects one registry-owned runtime availability snapshot into tool registration, diagnostics, resources, schemas, and guidance instead of maintaining parallel video-tool lists.

### Decomposition risks

- The retained artifact boundary currently validates only PNG bytes and a `temporal-vision` manifest. Its additive generalization is the largest compatibility risk: existing database rows, resource URIs, cache validation, deletion, recovery, and retention accounting must remain readable and authoritative.
- FFmpeg builds expose materially different H.264 encoders. Qualification must prove the complete output contract and record the selected implementation without turning environment diversity into nondeterministic tool registration or unsafe argument fallback.
- The two presentation policies can misstate observed duration if provenance and visible labeling diverge. One canonical plan must drive both encoder input and manifest mapping, with explicit gap slates and no cross-epoch stretching.
- MCP hosts differ in local-video attachment support. The tool can truthfully return a resource even when a host cannot forward it, so guidance must distinguish Krometrail availability from host/model consumption and retain the still-first recovery path.

## Other agent review

- Invoked because: the epic changes a stable public MCP/artifact contract and introduces a security-sensitive external process boundary.
- Skipped/degraded: the active autopilot delegation explicitly made this designer an endpoint and prohibited nested agents or peeragent. Design-time advisory review is therefore unavailable and non-blocking; the direct source-grounded pre-mortem above records the principal risks for feature design and the required standard completion review remains unchanged.

## Implementation summary

- Completed all four child features: deterministic clip/provenance contracts, qualified managed FFmpeg
  runtime, retained generation/storage lifecycle, and the conditional MCP/plugin/evaluation surface.
- Krometrail uses only a bounded user-selected or user-discoverable FFmpeg process; it ships no encoder,
  downloader, wrapper acquisition path, audio support, provider upload adapter, or model-capability guess.
- One immutable startup qualification result controls service construction, capability discovery, the
  video-only output schema, tool/resource registration, and restart recovery while preserving the exact
  legacy still-tool response schemas when video is unavailable.
- Both presentation policies generate retained local MP4/H.264 resources with typed manifests, source and
  timing provenance, visible gaps, bounded reads, cache identity, retention accounting, recovery, deletion,
  cancellation, deadline, and late-publication fencing.
- The shipped skill is still-first, explains conditional availability and user-installed setup, keeps local
  evidence private by default, and separates encoder qualification from exact host/provider/model support.

## Feature completion evidence

- `epic-temporal-video-artifacts-clip-contracts`: done after one standard review and verified provenance/schema corrections.
- `epic-temporal-video-artifacts-ffmpeg-runtime`: done after one standard review and verified lifecycle/timing/platform corrections.
- `epic-temporal-video-artifacts-retained-generation`: done after one standard review and verified publication/cache/budget/gap corrections.
- `epic-temporal-video-artifacts-agent-surface`: done after one standard review and an exact stable-schema correction.
- Full workspace fmt/check/tests/Clippy and docs build passed before the final agent-surface review correction;
  focused MCP, degraded startup, plugin, skill, evaluation, and real selected-FFmpeg lanes pass afterward.

## Aggregate review findings (2026-07-18)

- The single standard aggregate pass found one P1 cross-layer cancellation race: MCP independently raced
  and dropped the generation future on the same cancellation/deadline that the video service must own to
  cancel and drain a durable retained publication. A cancellation after staging could therefore leave a
  recoverable MP4 and staging row.
- No other material end-to-end blockers were found, and the reviewed legacy still-schema correction was
  confirmed. Review source was the same-harness OpenAI-lineage fallback because cross-model Claude OAuth
  was unavailable.

## Aggregate review correction (2026-07-18)

- MCP now performs its initial budget check, passes the exact deadline/token into the service, and awaits
  the service directly. The retained-video service is the single owner of the cancellation race and cannot
  be dropped before it cancels and drains publication cleanup.
- A protocol-boundary regression sends `notifications/cancelled` while a fake durable-publication phase is
  paused and proves the service cleanup completes before the request future ends.
- Focused service cancellation/deadline cleanup tests pass. Store tests that cancel at every durable file
  boundary, reopen storage, and prove no video recovers also pass, as does the no-visible/no-accounted-state
  cancellation test. The accepted finding was fixed and verified without a second aggregate review pass.

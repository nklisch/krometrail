---
id: epic-temporal-video-artifacts
kind: epic
stage: drafting
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

## Strategic decisions

- **User-installed encoder only.** Krometrail owns neither FFmpeg acquisition nor redistribution. The implementation must not enable any dependency feature that downloads FFmpeg and must not place FFmpeg binaries in release assets or plugin packages.
- **Narrow licensing posture.** Keeping FFmpeg out of Krometrail's artifacts avoids taking on FFmpeg binary redistribution as part of this product, but it is not described as eliminating every licensing consideration. Dependency licenses, process-boundary assumptions, notices, and the supported encoder contract still receive release review; the user's FFmpeg installation remains outside Krometrail's managed installation.
- **Conditional MCP surface.** Introduce `temporal-video` as a runtime-qualified capability depending on `temporal-vision`. Register its tool only when startup qualification proves that the resolved executable can produce the supported MP4/H.264 contract. If that executable later disappears or changes, the advertised tool returns a stable structured encoder-unavailable failure with recovery guidance.
- **Derived artifact, not capture.** Video generation consumes the existing resolved range and authoritative retained source frames. Known gaps become labeled gap slates, incompatible visual epochs remain explicit, and no motion interpolation invents unseen state.
- **Two bounded presentation policies.** Real-time presentation preserves relative recorded timing within limits. Model-optimized presentation may hold selected meaningful states so sparse provider sampling is less likely to miss them; the manifest exposes every hold and mapping so it cannot be mistaken for observed duration.
- **Provider-neutral local output.** Return a retained `video/mp4` resource plus a machine-readable manifest. Krometrail does not upload, attach, or adapt the clip for Gemini, Kimi, or another provider; that remains the MCP host's responsibility.
- **External encoder provenance.** Cache and manifest identity include the exact FFmpeg build, selected H.264 encoder, adapter version, arguments, presentation plan, and output limits. The deterministic guarantee applies to the presentation plan; encoded byte equality is claimed only for the exact qualified encoder identity, never across arbitrary installations.
- **Narrow subprocess boundary.** The production adapter invokes the executable directly with allowlisted arguments, not through a shell. It bounds input/output, concurrency, stderr, CPU time, deadline, cancellation, process-tree cleanup, and atomic publication. Logs contain no frame pixels, browser content, raw command input, or unredacted local paths.
- **No UI work.** This epic adds an MCP capability, retained resources, diagnostics, evaluation fixtures, and skill guidance. It has no human screen or browser UI and needs no mockups.

## Capability boundary

The epic owns the public clip request and resource shapes, runtime availability projection, encoder qualification, encoding service/port, retained artifact and manifest integration, cancellation and failure behavior, plugin-skill discoverability, and deterministic plus opt-in live qualification. It should extend the existing capability and artifact registries rather than introduce parallel tool lists or a second storage index.

The design phase must select the smallest maintainable process adapter. Direct `tokio::process::Command` is the baseline because the contract is one narrow encode operation with strict lifecycle control. `ffmpeg-sidecar` may be used only if it materially reduces lifecycle complexity with default download features disabled; native FFmpeg binding crates are out of scope unless design evidence shows that an external process cannot satisfy the contract.

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

## Open design questions

- Which allowlisted H.264 encoders and pixel-format/profile combinations form the portable initial qualification set across supported macOS and Linux installations?
- What exact default caps balance model readability with local generation cost for duration, dimensions, frames, bytes, and deadline?
- Should the public request expose both policies immediately or ship real-time first while retaining the policy/version field for additive expansion?
- Which MCP hosts can currently attach a local `video/mp4` resource, and what host-specific instructions belong in progressive skill references rather than the primary workflow?

Child feature decomposition is intentionally deferred to epic design after these portability and host-ingestion questions are resolved against the supported environments.

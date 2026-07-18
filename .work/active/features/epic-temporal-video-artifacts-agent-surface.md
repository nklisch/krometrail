---
id: epic-temporal-video-artifacts-agent-surface
kind: feature
stage: review
tags: [agent-ux, infra, testing]
parent: epic-temporal-video-artifacts
depends_on: [epic-temporal-video-artifacts-ffmpeg-runtime, epic-temporal-video-artifacts-retained-generation]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Conditional temporal video agent surface

## Brief

Join the qualified encoder and retained generation service at the composition root, project one startup availability snapshot through the capability registry, and register the temporal-video MCP tool and video resources only when qualification succeeds. The public request exposes both bounded policies through generated validated schemas; responses return the local `video/mp4` resource and manifest, while a post-start encoder loss maps to a stable actionable error without affecting still artifacts or capture.

Update the shipped Krometrail skill and progressive evidence guidance so agents can explain an absent tool, tell a user how their own FFmpeg installation enables it after restart, and recommend video only when the host/model is already known to accept it. This feature also owns registry/schema/resource/plugin contract coverage and opt-in end-to-end qualification, but it does not add provider uploads, automatic model-capability detection, a product-managed encoder, or a human UI.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: integration and consumer feature — depends on both the qualified runtime and retained-generation branches

## Simplification opportunity

- Make registry-owned runtime availability the single authority for tool discovery, service injection, diagnostics, schemas, resources, and skill wording; remove the need for a dead placeholder tool or scattered FFmpeg checks in handlers and prose.

## Foundation references

- `docs/VISION.md` — Core Experience and Visual Evidence
- `docs/SPEC.md` — Capabilities, Temporal Queries, Errors and Degraded Operation, and Local Data
- `docs/ARCHITECTURE.md` — Capability Registry, MCP Boundary, Observability, and Dependency Direction
- `docs/VISUAL-EVIDENCE.md` — Temporal Video Clip and Progressive Detail
- `docs/EVALUATION.md` — Optional video conditions and Temporal video evaluation
- `plugin/skills/krometrail/SKILL.md` — installed agent workflow and capability discoverability

## Parent decisions inherited

- Conditional registration is based on one bounded startup qualification snapshot and changes only after MCP restart.
- Krometrail does not infer model video support; skill recommendations are conditional on known host/model capability.
- The runtime returns local provider-neutral resources and never uploads or attaches them.
- No UI surfaces or mockups apply.

## Readiness and dispatch

- Both declared dependencies are implemented and independently approved. The concrete `krometrail-ffmpeg` qualification result, retained-generation port, MP4 handle, typed manifest, and shared artifact store are therefore the design inputs rather than speculative interfaces.
- The active autopilot endpoint prohibits nested agents and peeragent, so the normal design advisory is unavailable. Direct inspection covered the parent and upstream features, capability/MCP registries, generated schemas, resources, response mapping, composition/diagnostics, plugin release coupling, and evaluation harness. Standard independent feature review remains required after implementation.
- The three new child dependencies are a forward-only chain and introduce no path to this feature or its upstreams; direct frontmatter inspection confirms the graph is acyclic.

## Design decisions

- **One immutable startup authority**: add a constructor-validated runtime capability snapshot derived once from registry defaults, operator selection, and the FFmpeg qualification result. `temporal-video` is a runtime-qualified capability depending on `temporal-vision`; `McpConfig`, router registration, resources, and diagnostics consume the same snapshot. Handlers never rediscover FFmpeg.
- **Qualification belongs to MCP startup**: qualify only in the `mcp` command path, with the FFmpeg adapter's bounded deadline and safe result. Doctor remains discovery-only. A missing, unsupported, or unsuitable user executable degrades temporal video to unavailable while the MCP server and every existing capability start normally.
- **Configuration and service cannot drift**: MCP construction accepts `Option<Arc<dyn TemporalVideoGeneration>>` and rejects a snapshot/service mismatch. An enabled capability always has one service; an unavailable capability has none. This preserves registry-derived surfaces without a dead placeholder tool.
- **Stable after-start semantics**: the advertised surface does not mutate during a server lifetime. If the qualified executable disappears or changes after startup, generation maps the adapter failure to stable `video_encoder_unavailable` guidance and asks for MCP restart; still artifacts and other tools remain healthy.
- **One generated operation contract**: add registry operation `generate_temporal_video`, marked read-only/idempotent/open-world under the runtime-qualified capability. Its strict generated input schema is the existing validated `TemporalVideoGenerationRequest`; route registration is derived from the operation definition and uses normal request budget/cancellation context.
- **Local links, never inline video**: structured output is compact clip metadata. Each epoch contributes a `video/mp4` resource link and JSON manifest link; no base64 MP4, provider attachment, filesystem path, model field, or upload side effect enters the response.
- **One retained read authority**: extend the retained-generation boundary with a scoped, bounded video-artifact read method implemented by the same service/store. MCP resource reads do not depend directly on storage or create a parallel video lookup path.
- **Capability-filtered resource registry**: generalize the existing evidence resource definitions with capability ownership, add canonical video and video-manifest templates, and filter both listing and reads from the immutable snapshot. Existing image/frame URI, names, MIME types, and responses stay unchanged.
- **Truthful skill discoverability**: the shipped skill stays still-first. It explains that absent video tools mean startup qualification failed or was unavailable, Krometrail never bundles/downloads FFmpeg, an install/path change requires MCP restart, and tool presence says nothing about whether the host/model accepts video. Returned resources remain user/host-attached and provider-neutral.
- **Optional evaluation only**: add real-time and model-optimized video conditions as explicitly optional, with host/provider/model, encoder build/name, argument-policy, output hash, resource, and manifest evidence. Required still conditions A-E remain unchanged; no model call or capability inference enters runtime qualification.
- **No UI and no managed encoder**: this feature changes MCP, diagnostics, plugin prose, release assertions, and evaluation fixtures only. It does not add a screen, upload adapter, model detector, FFmpeg downloader, bundled binary, or dynamic mid-session tool registration.

## Architectural choice

Three approaches were considered:

1. Register a permanent placeholder tool and check FFmpeg inside each call. This makes discovery lie, duplicates checks, and permits schemas/resources/diagnostics to disagree.
2. Let the MCP router and resource server independently inspect an optional encoder. This avoids a core capability type but creates multiple availability authorities and unstable construction invariants.
3. Resolve one immutable registry-owned capability snapshot at MCP startup, construct the retained service only from a qualified encoder, and inject both through a checked composition boundary.

Choose approach 3. It makes absence visible through normal tool/resource discovery, keeps the server useful under degradation, and gives post-start loss one explicit error path. The only runtime variability after construction is operation success, not advertised availability.

## Implementation units

### Unit 1: Runtime-qualified capability snapshot and composition

**Files**: `crates/krometrail-core/src/capabilities/mod.rs`, `crates/krometrail-core/src/video/generation.rs`, `crates/krometrail-core/src/video/mod.rs`, `src/app.rs`, `src/diagnostics.rs`, `Cargo.toml`

**Story**: `epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition`

- Add `CapabilityId::TemporalVideo`, a runtime-qualified default/state, and immutable registry-ordered snapshot construction that validates dependencies and explicit selections.
- Add the temporal-video operation definition and a scoped, maximum-byte-bounded retained video read method to the application port.
- In the MCP command branch, run one bounded `qualify_ffmpeg`, construct `TemporalVideoGenerationService` only for `Qualified`, resolve the snapshot, and emit exactly one safe final availability diagnostic. Include `krometrail_ffmpeg` in the configured tracing target set.
- Diagnostic fields may identify capability, qualification stage/reason, selected encoder policy/name, and restart recovery; they must not expose executable paths, frames, page data, raw stderr, or command input.
- Keep unsupported platforms and no-FFmpeg installations healthy with temporal video unavailable. Installing/replacing FFmpeg changes availability only on MCP restart.

### Unit 2: Conditional MCP tool, response, and resources

**Files**: `crates/krometrail-mcp/src/config.rs`, `crates/krometrail-mcp/src/dependencies.rs`, `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/resources.rs`, `crates/krometrail-mcp/src/server.rs`, `crates/krometrail-mcp/tests/**`

**Story**: `epic-temporal-video-artifacts-agent-surface-mcp-tool-and-resources`

- Make `McpConfig` wrap the resolved snapshot and validate exact agreement with the optional temporal-video service during router construction.
- Derive `generate_temporal_video` from the core operation definition. Generate and dereference its strict request schema; map request budget/cancellation to `ArtifactGenerationContext`; delegate once to `TemporalVideoGeneration`.
- Add compact per-epoch output metadata and `ResourceRole::Video`/`VideoManifest`. Emit two local links per clip and never inline encoded bytes or provider-specific fields.
- Add canonical scoped URI templates `/videos/{id}` and `/video-manifests/{id}` (using the project's existing URI authority/prefix), with `video/mp4` blob and JSON manifest reads through the same injected generation/read port. URI parsing, scope, hash, length, and maximum encoded bytes remain boundary-validated.
- Filter video operation, schemas, templates, and reads from the same snapshot. Existing still/frame routes are byte-for-byte compatible and stay available when video is unavailable or later fails.
- Preserve the stable `video_encoder_unavailable` domain/code mapping for after-start loss, with actionable restart/install guidance and normal diagnostic correlation.

### Unit 3: Shipped guidance, release assertions, and optional qualification

**Files**: `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/setup.md`, `plugin/skills/krometrail/references/evidence.md`, `plugin/skills/krometrail/agents/openai.yaml`, `plugin/.codex-plugin/plugin.json`, `.agents/plugins/marketplace.json`, `tests/plugin-static.sh`, `tests/plugin-install-smoke.sh`, `crates/temporal-evaluation/src/**`, `crates/temporal-evaluation/tests/**`, root integration tests and qualification fixtures`

**Story**: `epic-temporal-video-artifacts-agent-surface-guidance-and-qualification`

- Teach absence, enablement, restart, still-first selection, local-resource use, model-optimized hold provenance, and the difference between Krometrail encoder availability and known host/model support. Keep catalog/agent descriptions release-coupled without manually bumping a version.
- Static/install tests prove a normal no-FFmpeg plugin startup exposes no video tool/templates, existing tools remain present, prose does not promise downloads/model detection/uploads, and plugin/release assets contain no FFmpeg binary or acquisition path.
- Extend the evaluation vocabulary with optional real-time and model-optimized video conditions and typed resource/manifest/environment evidence. Preserve A-E as the required canonical set; lack of qualified encoder or explicitly known video-capable host/model is skipped/not-applicable, never a substituted pass.
- Hermetic MCP tests use fake qualification/service/read ports. An explicitly invoked opt-in live test uses the user-selected FFmpeg, local retained fixture frames, both policies, the real generation/store/MCP path, resource reads, and safe diagnostics. It fails rather than silently skips once invoked and uses no browser, provider, model, upload, or network.

## Ordering and child graph

1. `epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition` establishes the single authority and read/operation contracts.
2. `epic-temporal-video-artifacts-agent-surface-mcp-tool-and-resources` depends on checkpoint 1 and projects that authority into the public MCP surface.
3. `epic-temporal-video-artifacts-agent-surface-guidance-and-qualification` depends on checkpoints 1 and 2 so prose and evaluation evidence describe the implemented discovery/resource behavior.

The graph is deliberately linear at the public integration seam. It prevents plugin prose or evaluation fixtures from becoming a second contract and prevents MCP code from reimplementing qualification.

## Verification strategy

- Core tests cover snapshot ordering, conditional enable/disable/unavailable states, dependency rejection, explicit unavailable selection, and operation/read validation.
- App tests inject qualified/unavailable results and prove one immutable snapshot, bounded degraded startup, one privacy-safe diagnostic, and snapshot/service consistency.
- MCP contract tests compare unavailable versus qualified registries, generated strict schema, resource templates, compact multi-epoch response links, scoped blob/manifest reads, and stable post-start failure isolation.
- Existing tool/resource/schema golden tests remain unchanged except for conditional additions under a qualified fixture.
- Plugin static/install smoke runs without FFmpeg and asserts absence plus accurate recovery guidance. Deterministic workspace tests require neither FFmpeg nor network.
- The opt-in real integration test records exact executable/build/encoder/policy identity and validates both generated MP4 resources and manifests through the public MCP route.

## Risks and mitigations

- **Snapshot/service drift** could advertise an unusable route. Construction rejects either enabled-without-service or service-without-enabled-capability.
- **Startup probing could delay or break MCP**. Qualification is bounded and failure degrades only temporal video; the final result is logged once.
- **Post-start executable drift** cannot safely unregister tools. The immutable surface returns one stable error and restart recovery while unrelated tools continue.
- **Large MP4 context pressure** is controlled by output limits and resource links; tool responses never inline video bytes.
- **Resource authority could split from retention**. Reads stay behind the retained-generation/store port and validate scope/hash/length before returning bytes.
- **Tool presence could be mistaken for model support**. Skill and evaluation contracts explicitly require independently known host/model support and preserve still-first fallback.
- **Optional video evaluation could weaken A-E**. F/G remain separate optional conditions with skip/not-applicable semantics and cannot satisfy required conditions.
- **Generated schema and registry drift** is prevented by one operation definition plus exact registry/schema/template tests.
- **Release licensing expectations could drift**. Static release/plugin assertions prohibit bundled/downloaded FFmpeg and keep user-installed qualification explicit.

## Other agent review

- Invoked because: this feature completes a stable public MCP/plugin surface and composes both external-process and retained-data branches.
- Skipped/degraded: the active autopilot delegation explicitly prohibited nested agents and peeragent. Source-grounded alternatives and the pre-mortem above substitute for design-time advisory only; normal independent feature review remains mandatory.

## Implementation summary

- One immutable startup capability snapshot now controls retained-video service construction, MCP tool
  discovery, schemas, resource templates, and reads while preserving healthy still evidence when FFmpeg
  is unavailable.
- The conditional `generate_temporal_video` route returns compact per-epoch metadata and local MP4 and
  manifest links; bounded retained reads enforce scope and size through the same injected authority.
- Shipped agent guidance, distribution checks, optional F/G evaluation evidence, deterministic degraded
  startup coverage, and an explicit real-FFmpeg end-to-end lane now describe and qualify the surface.

## Verification evidence

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `bun run docs:build`
- `bash tests/plugin-static.sh`
- skill-creator `quick_validate.py plugin/skills/krometrail` via isolated `uv --with pyyaml`
- explicit selected-FFmpeg live integration for `real_time` and `model_optimized`

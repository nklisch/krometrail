---
id: epic-temporal-video-artifacts-ffmpeg-runtime
kind: feature
stage: review
tags: [infra, security, testing]
parent: epic-temporal-video-artifacts
depends_on: [epic-temporal-video-artifacts-clip-contracts]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Qualified FFmpeg runtime

## Brief

Deliver the optional production adapter for a user-installed `ffmpeg`: safe executable discovery, a bounded real MP4/H.264 qualification encode, exact implementation identity, and direct encoding through the injected clip contract. Process execution must use fixed allowlisted arguments without a shell and must enforce cancellation, deadline, output and diagnostic bounds, child termination/reaping, private temporary state, and atomic handoff of a validated result.

The adapter owns no download, installer, provider upload, MCP routing, retained artifact publication, or global FFmpeg configuration. Missing, unsuitable, changed, or vanished executables become safe qualification/runtime outcomes while the rest of Krometrail remains operational.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: infrastructure consumer of `epic-temporal-video-artifacts-clip-contracts`; can proceed in parallel with retained generation

## Simplification opportunity

- Keep executable discovery, qualification, sanitized diagnostics, and process-tree lifecycle in one direct Tokio adapter; avoid `ffmpeg-sidecar`, downloader features, native FFmpeg bindings, shell helpers, and duplicate subprocess abstractions unless concrete implementation evidence invalidates the simpler boundary.

## Foundation references

- `docs/SPEC.md` — Supported Environment and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Artifact Generation, Capability Registry, Failure Isolation, and Dependency Direction
- `docs/VISUAL-EVIDENCE.md` — Temporal Video Clip determinism and encoder provenance
- `docs/EVALUATION.md` — Temporal video generation, cancellation, and cleanup qualification

## Parent decisions inherited

- Qualification proves the produced MP4/H.264 contract rather than trusting names or version text.
- Only a versioned encoder/argument allowlist may be attempted, and the exact selected implementation is retained.
- Startup resolution creates one immutable availability/encoder identity until MCP restart.
- No bundled or managed FFmpeg and no UI surfaces apply.

## Design decisions

- **Adapter boundary**: add one unpublished `krometrail-ffmpeg` infrastructure crate that implements the committed core port. Keeping executable authority outside the binary's application services makes the security boundary independently testable while the later agent-surface feature remains the sole composition owner.
- **Initial encoder allowlist**: argument policy `krometrail-ffmpeg-h264-v1` permits only the software `libx264` encoder. Hardware encoders and generic fallback negotiation were rejected because their device-specific setup and output identity are not represented by the committed port contract.
- **Discovery precedence**: an explicitly supplied executable is authoritative and does not fall back when invalid; otherwise discovery checks a bounded, deduplicated set of `PATH` candidates followed by fixed platform-default install locations. Candidate paths are canonicalized for execution but never copied into public errors or logs.
- **Produced-contract qualification**: qualification hashes a bounded complete `ffmpeg -version` report, derives a safe version label, and then sends a tiny two-state request through the same job builder, process runner, and MP4 validator used by ordinary encoding. Encoder-list text is neither necessary nor sufficient.
- **Private file-backed input and output**: generated frame names and an `ffconcat` file live in a mode-restricted temporary directory. FFmpeg writes a fixed relative partial-output name in that directory; only fully exited, bounded, parsed bytes cross back into the core result. This keeps path syntax and partial artifacts out of caller control.
- **Presentation timebase**: plan boundaries are deterministically rounded from nanoseconds to a one-microsecond FFconcat/filter/MP4 timebase by rounding cumulative endpoints, never each duration independently. The generated PTS filter assigns those validated boundaries explicitly, and the repeated terminal frame lands at duration minus one tick so FFmpeg's input frame duration closes the track exactly. The v1 argument-policy identity covers this sub-microsecond presentation loss; source/session provenance remains the exact committed plan.
- **Failure mapping**: cancellation remains `cancelled`, an expired deadline or unsuccessful/invalid encode becomes `video_encoding_failed`, output overflow remains `resource_limit_exceeded`, and disappearance or pre-run identity drift becomes `video_encoder_unavailable`. Private causes are reduced to bounded stage, exit class, byte counts, truncation flags, and hashes before tracing.
- **Runtime identity pinning**: qualification stores the canonical executable and a platform file stamp plus executable digest. Each encode rechecks the canonical path and file stamp before launch; ordinary replacement or disappearance therefore fails closed until restart. Defending against a malicious same-user replacement preserving metadata during the final launch race is outside the local-user trust boundary.
- **One active encoder process**: the adapter owns a one-permit semaphore and also fixes FFmpeg/filter thread counts to one. This bounds CPU concurrency without introducing a general scheduler; the application service can impose its independent request concurrency above the port.
- **No UI surface**: this feature is an infrastructure adapter and opt-in qualification lane only. It inherits the epic's explicit no-UI decision and needs no mockups.

## Architectural choice

Three approaches were considered:

1. **Dedicated direct-process adapter crate (chosen).** A small `krometrail-ffmpeg` crate depends inward on `krometrail-core`, owns discovery, private staging, process lifetime, and output validation, and exposes only a qualified concrete encoder plus a safe unavailable result. This matches the existing injected-adapter architecture and gives subprocess tests a narrow boundary.
2. **Put FFmpeg in the root `src/video` module.** This avoids one workspace member, but mixes process authority with deterministic presentation planning and forces the retained-generation and final composition branches to share implementation ownership. The small Cargo saving is not worth weakening the security boundary.
3. **Use a wrapper crate or native FFmpeg bindings.** A wrapper could shorten command assembly, while native bindings could inspect codecs directly. Both expand dependency, build, licensing, and possible managed-download surfaces without replacing the need for Krometrail-specific limits, cancellation, identity, and validation.

The chosen crate is not a reusable subprocess library or media framework. Its only public operation is qualification of, and encoding through, one fixed silent MP4/H.264 policy.

## Implementation units

### Unit 1: Adapter crate and fixed policy

**Files**: `Cargo.toml`, `crates/krometrail-ffmpeg/Cargo.toml`, `crates/krometrail-ffmpeg/src/lib.rs`, `crates/krometrail-ffmpeg/src/policy.rs`

**Story**: `epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation`

```rust
pub const FFMPEG_ARGUMENT_POLICY_VERSION: &str = "krometrail-ffmpeg-h264-v1";
pub const FFMPEG_ENCODER_ALLOWLIST: &[&str] = &["libx264"];
pub const MAX_FFMPEG_VERSION_REPORT_BYTES: usize = 64 * 1024;
pub const MAX_FFMPEG_STDERR_BYTES: usize = 64 * 1024;
pub const MAX_FFMPEG_DISCOVERY_CANDIDATES: usize = 16;
pub const FFMPEG_QUALIFICATION_TIMEOUT: Duration = Duration::from_secs(5);
pub const FFMPEG_TERMINATION_GRACE: Duration = Duration::from_millis(250);
pub const FFMPEG_TIMEBASE_HZ: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum H264Encoder {
    Libx264,
}

impl H264Encoder {
    const fn name(self) -> &'static str;
    fn append_arguments(self, command: &mut Vec<OsString>, job: &PreparedEncodeJob);
}
```

**Implementation notes**:

- Register the unpublished adapter as a workspace member. It depends inward on core and the existing `temporal-vision` output-hash value required by `VideoEncodedClip`, plus the narrow runtime utilities it uses: Tokio process/I/O/sync/time, `tempfile`, SHA-256, tracing, and platform process-control support. It enables no downloader, network, native-codec, or FFmpeg wrapper feature.
- Keep every argument in `policy.rs`. V1 fixes `-nostdin`, no shell, concat-demuxer safe mode, one video map, no audio/subtitle/data, VFR, one-microsecond encoder/track timebase, aspect-preserving scale to the committed `scaled` dimensions, trailing pad to the committed even `canvas`, `yuv420p`, `libx264`, single encoder/filter threads, all-intra GOP, metadata removal, bounded output size, and MP4 fast-start output.
- Inputs and output names are constant relative names below the private working directory. No request string, URL, browser text, local absolute path, caller-supplied filter expression, codec, or arbitrary FFmpeg option enters argv.

**Acceptance criteria**:

- [x] A single exact-argument test fixes ordering and values for v1, including geometry and byte ceilings, without a second production argument list.
- [x] The adapter crate has no HTTP, downloader, shell, native FFmpeg, provider, MCP, storage, or browser dependency.
- [x] `cargo tree` and the lockfile contain no FFmpeg binary or binding package introduced by this feature.

### Unit 2: Private request staging and owned process lifecycle

**Files**: `crates/krometrail-ffmpeg/src/job.rs`, `crates/krometrail-ffmpeg/src/process.rs`

**Story**: `epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation`

```rust
struct PreparedEncodeJob {
    workspace: tempfile::TempDir,
    arguments: Vec<OsString>,
    expected: ExpectedMp4,
    output_limit: u64,
}

impl PreparedEncodeJob {
    async fn from_request(request: &VideoEncodeRequest) -> Result<Self, AdapterFailure>;
    fn output_path(&self) -> &Path;
}

struct ProcessLimits {
    deadline: Instant,
    cpu_seconds: u64,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

enum FfmpegInvocation<'a> {
    VersionProbe,
    Encode(&'a PreparedEncodeJob),
}

struct ManagedFfmpegProcess {
    child: tokio::process::Child,
    tree: ProcessTreeGuard,
}

impl ManagedFfmpegProcess {
    async fn spawn(
        executable: &QualifiedExecutable,
        invocation: FfmpegInvocation<'_>,
        limits: ProcessLimits,
    ) -> Result<Self, AdapterFailure>;
    async fn wait_or_cancel(
        &mut self,
        cancellation: &dyn CancellationSignal,
        deadline: Instant,
    ) -> Result<SanitizedProcessOutcome, AdapterFailure>;
    async fn terminate_and_reap(&mut self) -> Result<(), AdapterFailure>;
}
```

**Implementation notes**:

- `PreparedEncodeJob` creates a fresh private directory, writes numbered PNG/JPEG inputs with create-new semantics, and writes an ASCII FFconcat document containing only those generated relative names. It repeats the terminal file as required by the concat demuxer and derives every duration from monotonically rounded cumulative plan endpoints.
- Write failure or cancellation destroys the entire temporary directory. Symlinks and pre-existing output names are impossible inside the fresh directory; FFmpeg receives a fixed working directory and fixed relative input/output names.
- Spawn with null stdin, piped and bounded stdout/stderr, `kill_on_drop(true)`, a scrubbed environment containing only the platform minimum, and no inherited handles beyond required pipes. Never use `sh`, `cmd.exe`, `powershell`, `Command::arg` with a joined command line, or an external kill helper.
- On Unix create an isolated process group and apply `RLIMIT_CPU` from the bounded remaining deadline before exec. On Windows assign the child immediately to a kill-on-close Job Object with a one-process active limit and per-process CPU limit; if tree ownership cannot be established, kill/reap and fail qualification rather than advertise a weaker boundary.
- `wait_or_cancel` selects among child exit, core cancellation, deadline, and bounded pipe readers. Every non-success path owns termination: TERM/Job termination, a 250 ms grace, force kill, direct-child reap, pipe completion, and a positive tree-empty result. `Drop` provides the final non-async force-kill guard when the encode future itself is dropped.
- `-fs` supplies the encoder-side byte ceiling. A concurrent output metadata check and the post-exit read enforce the same request/profile limit; partial or oversized output never constructs a result.
- Internal errors contain no paths or raw stderr. The runner records a closed failure stage, sanitized exit class, captured length, truncation flag, and SHA-256 of captured diagnostics for local structured tracing.

**Acceptance criteria**:

- [x] Success returns only after exit zero, bounded pipe completion, full child reaping, and an exact-size-bounded read of the private output.
- [x] Cancellation, elapsed deadline, output overflow, diagnostic overflow, spawn failure, and dropped encode futures leave no direct child, owned descendant, or caller-visible partial bytes.
- [x] Frame payloads, FFconcat contents, executable/temp paths, and raw process diagnostics never appear in tracing fields or core errors.
- [x] Tests exercise the same process guard on Unix and Windows; unsupported tree ownership fails closed instead of silently degrading.

### Unit 3: Bounded MP4/H.264 validator

**Files**: `crates/krometrail-ffmpeg/src/mp4.rs`, `crates/krometrail-ffmpeg/tests/fixtures/video/valid-h264.mp4`, `crates/krometrail-ffmpeg/tests/fixtures/video/README.md`

**Story**: `epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation`

```rust
struct ExpectedMp4 {
    canvas: PixelDimensions,
    presentation_duration_micros: u64,
    max_bytes: u64,
}

struct ValidatedMp4 {
    bytes: Arc<[u8]>,
    output_hash: temporal_vision::OutputHash,
}

fn validate_mp4(bytes: Arc<[u8]>, expected: ExpectedMp4)
    -> Result<ValidatedMp4, AdapterFailure>;
```

**Implementation notes**:

- Parse ISO-BMFF box lengths with checked arithmetic, a fixed nesting/box-count ceiling, and no allocation derived from an untrusted declared size. Reject truncated, overlapping, zero-progress, indefinite, or trailing-invalid boxes.
- Require `ftyp`, one `moov`, media data, exactly one video track, `vide` handler, `avc1` or `avc3` sample entry, matching canvas dimensions, a finite movie/track timebase and duration within one microsecond tick of the prepared presentation, and no `soun` handler. Magic bytes or an encoder exit code alone do not qualify output.
- The checked-in fixture is a small redistributable test artifact generated by the repository's opt-in live command with its provenance and hash recorded in a neighboring README during implementation. Mutation tests cover wrong codec, audio track, wrong dimensions, corrupt box sizes, missing media, truncation, and excessive nesting.
- If legitimate fixed-policy FFmpeg output exposes a parser gap, extend the bounded parser against a retained fixture. Do not weaken qualification to substring scanning or version/encoder-name claims.

**Acceptance criteria**:

- [x] Valid fixed-policy output proves MP4 + H.264 + one matching-dimension video track + no audio before a core result can be constructed.
- [x] Malformed container lengths and nesting cannot panic, loop, allocate by attacker-declared size, or read out of bounds.
- [x] Fixture mutations fail at the expected closed validation stage without leaking raw payloads.

### Unit 4: Executable discovery, identity, and real qualification

**Files**: `crates/krometrail-ffmpeg/src/discovery.rs`, `crates/krometrail-ffmpeg/src/qualification.rs`

**Story**: `epic-temporal-video-artifacts-ffmpeg-runtime-discovery-and-qualification`

```rust
pub struct FfmpegDiscoveryOptions {
    explicit_executable: Option<PathBuf>,
    search_path: Option<OsString>,
}

impl FfmpegDiscoveryOptions {
    pub fn from_process_environment() -> Self;
    pub fn with_explicit_executable(path: PathBuf) -> Self;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegQualificationStage {
    Discovery,
    ExecutableIdentity,
    VersionProbe,
    EncodeProbe,
    OutputValidation,
    ProcessCleanup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegUnavailableReason {
    NotFound,
    InvalidCandidate,
    ChangedCandidate,
    TimedOut,
    UnsupportedEncoder,
    InvalidOutput,
    ProcessFailed,
    Cancelled,
}

pub struct FfmpegUnavailable {
    pub stage: FfmpegQualificationStage,
    pub reason: FfmpegUnavailableReason,
}

pub enum FfmpegQualification {
    Qualified(Arc<QualifiedFfmpegEncoder>),
    Unavailable(FfmpegUnavailable),
}

pub async fn qualify_ffmpeg(
    options: FfmpegDiscoveryOptions,
    cancellation: Arc<dyn CancellationSignal>,
    deadline: Instant,
) -> FfmpegQualification;
```

**Implementation notes**:

- `from_process_environment` accepts `KROMETRAIL_FFMPEG_PATH` as the explicit override and otherwise snapshots `PATH`; no environment is reread after qualification. An explicit relative, missing, non-file, non-executable, or uncanonicalizable path is an `InvalidCandidate` result without fallback.
- PATH/default discovery is internal—no `which`/`where` subprocess—and is bounded to 16 canonical, deduplicated candidates named exactly `ffmpeg`/`ffmpeg.exe`. Platform defaults are fixed policy data, not recursive filesystem searches.
- Hash the executable once under a bounded maximum file size and retain a platform file stamp privately. Run `-nostdin -version` through the same managed runner with 64 KiB stdout/stderr ceilings; the complete bounded report digest becomes `build_report_sha256`, while `implementation_version` is a restricted first-line token or a digest-derived fallback label.
- Construct identity only after the tiny two-state 2x2 PNG request succeeds through the v1 `libx264` arguments and `validate_mp4`. Identity records `libx264`, the crate/product adapter version, and `FFMPEG_ARGUMENT_POLICY_VERSION`; it never records candidate or temp paths or raw build configuration.
- Candidate failures remain local and bounded. With no explicit override, continue deterministically to the next candidate until one qualifies; return only the final safe unavailable stage/reason aggregate, never a vector of path-bearing causes.

**Acceptance criteria**:

- [x] Missing and unsuitable FFmpeg are ordinary `Unavailable` outcomes, not startup errors or panics.
- [x] A version-reporting binary that cannot produce the validated fixed-policy clip never qualifies.
- [x] Qualification identity changes with the bounded build report, selected encoder, adapter version, or argument-policy version and exposes no executable path.
- [x] Installing or replacing FFmpeg cannot mutate an already returned qualification object; a changed stamp fails the next encode as unavailable until restart.

### Unit 5: Qualified core-port adapter and qualification ladder

**Files**: `crates/krometrail-ffmpeg/src/encoder.rs`, `crates/krometrail-ffmpeg/tests/ffmpeg_adapter.rs`, `crates/krometrail-ffmpeg/tests/ffmpeg_live.rs`, `crates/krometrail-ffmpeg/tests/support/fixture_main.rs`

**Story**: `epic-temporal-video-artifacts-ffmpeg-runtime-encoder-port-adapter`

```rust
pub struct QualifiedFfmpegEncoder {
    executable: QualifiedExecutable,
    identity: VideoEncoderIdentity,
    permit: Arc<tokio::sync::Semaphore>,
}

impl TemporalVideoEncoder for QualifiedFfmpegEncoder {
    fn identity(&self) -> &VideoEncoderIdentity;

    fn encode(
        &self,
        request: VideoEncodeRequest,
        context: VideoEncodingContext,
    ) -> PortFuture<'_, krometrail_core::Result<VideoEncodedClip>>;
}
```

**Implementation notes**:

- Before waiting for the sole permit, check immediate cancellation/deadline. Permit acquisition itself selects against both signals. Revalidate the executable stamp after acquiring the permit and before staging inputs.
- Use `PreparedEncodeJob`, `ManagedFfmpegProcess`, and `validate_mp4`; compute the exact output hash once, then construct `VideoEncodedClip` with the qualified identity and request profile. There is no alternate encode path after qualification.
- Map only at this adapter boundary: cancellation to `ErrorCode::Cancelled`, output limit to `ResourceLimitExceeded`, vanished/changed executable to `VideoEncoderUnavailable`, and every other sanitized encode failure to `VideoEncodingFailed` with the stable core recovery advice.
- A small compiled test executable emulates fixed version/exit/output/hang/overflow/descendant modes without a shell, network, user home, or real FFmpeg. It is test-only and is not selected by the installer or release asset workflow.
- Default tests cover discovery precedence, exact argv, valid/invalid output, permit cancellation, deadline, exit failure, stderr overflow, output overflow, executable drift, future drop, tree cleanup, and temporary-state removal. The real FFmpeg test is feature-gated and ignored by default; when explicitly run it requires a selected executable, fails rather than skips if qualification is unavailable, and prints only safe identity/stage evidence.

**Acceptance criteria**:

- [x] The concrete adapter is object-safe through `Arc<dyn TemporalVideoEncoder>` and returns bytes/identity/profile/hash accepted by the committed core constructors.
- [x] Queue wait and active encode both honor the exact core deadline and cancellation signal; dropping the returned future also triggers process/tree cleanup.
- [x] Deterministic workspace tests require no FFmpeg or network and cannot mutate the operator's installation, home, or PATH.
- [x] The explicit live command proves a playable fixed-policy clip and names the safe build/encoder/policy identity; it cannot report success when FFmpeg is absent or unsuitable.

## Implementation order

1. `epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation`
2. `epic-temporal-video-artifacts-ffmpeg-runtime-discovery-and-qualification`
3. `epic-temporal-video-artifacts-ffmpeg-runtime-encoder-port-adapter`

These are sequential security/acceptance checkpoints inside one cohesive adapter implementation, not separate default worker assignments.

## Simplification

- Keep one crate, one qualified executable object, one argument policy, one active-process permit, one process guard, and one output validator. Do not introduce generic media, codec, process-runner, discovery, or capability frameworks.
- Reuse `TemporalVideoEncoder`, `VideoEncodeRequest`, `VideoEncodingContext`, `VideoEncodedClip`, `VideoEncoderIdentity`, plan geometry/timing, stable core errors, and `CancellationSignal` without modifying the committed contracts in this feature.
- Reuse Tokio, SHA-256, tracing, libc/platform APIs, and the repository's hermetic-boundary/opt-in-qualification testing pattern. Add only `tempfile` and narrow Windows process-control support if not already reachable.
- Do not reuse browser-named `ManagedChromeProcess`: its synchronous child type, browser-specific lifecycle, and different ownership contract would create a misleading shared abstraction. Keep the FFmpeg guard private unless a third concrete process boundary later proves a real common shape.
- No downloader, package manager invocation, FFmpeg config parser, provider adapter, retained artifact store, MCP registration, UI, or dynamic encoder registry belongs here.

## Testing

- Exact policy and job-document tests protect the command-injection boundary and cumulative endpoint rounding without launching FFmpeg.
- Bounded MP4 parser tests protect the produced container/codec/no-audio contract and malformed external output handling.
- Hermetic compiled-process tests protect cancellation, deadline, overflow, exit classification, future-drop cleanup, process-tree ownership, and private temporary cleanup on supported platforms.
- Discovery/qualification tests protect explicit precedence, candidate bounding/deduplication, safe identity derivation, version-only rejection, and immutable startup identity.
- One ignored opt-in live test is the interoperability proof for a user-selected FFmpeg build. Default workspace gates remain deterministic and FFmpeg-free.
- No existing test is removed. Browser process tests remain authoritative for Chrome ownership and are not generalized or duplicated into production code.

## Risks

- **Future drop can bypass ordinary async cleanup.** Tokio cancellation may destroy the encode future at any await. Mitigation: process-group/Job ownership and kill-on-drop live in a guard created immediately after spawn; explicit branches still terminate, reap, and positively verify cleanup.
- **FFconcat and MP4 cannot represent arbitrary nanoseconds exactly.** Mitigation: cumulative endpoint rounding to a declared microsecond timebase is deterministic, bounded below one tick per boundary, and included in the argument-policy identity. The exact source plan remains manifest authority.
- **A narrow MP4 parser can reject legitimate FFmpeg variation or accept malformed structure.** Mitigation: the argument/muxer policy is fixed, box walking is bounded and checked, fixtures retain qualified output, and the fallback is to extend proof for a concrete fixture—not weaken it to magic bytes.
- **`libx264` is not present in every user build.** This intentionally makes video unavailable rather than selecting hardware or a codec path whose identity/setup is not represented by v1. A later additive policy can add a separately qualified encoder when its stable identity and arguments are designed.
- **The executable may change after qualification.** Canonical path plus platform stamp catches normal removal/replacement. The remaining same-user final-launch race is not fully preventable portably without managed binaries or native handle execution, both outside the user-installed trust model.
- **External decoders process retained browser-derived bytes.** Fixed generated filenames/arguments, private staging, one process, hard input/output/geometry/duration limits, CPU/deadline bounds, and no network or inherited stdin reduce blast radius; the adapter never treats FFmpeg stderr or output structure as trusted.
- **Windows Job Object assignment has stricter host behavior than direct child launch.** Qualification fails closed if kill-on-close ownership cannot be established. The capability remains optional, so preserving cleanup authority is preferable to advertising a degraded encoder.

## Other agent review

- Invoked because: direct executable authority, cancellation, cross-platform tree cleanup, and external binary parsing are security-sensitive stable-runtime work.
- Skipped/degraded: the active autopilot delegation explicitly prohibits nested agents and peeragent. The non-blocking design-time degradation is recorded here; the caller's standard independent implementation review remains required.

## Implementation summary

- Execution capability: GPT-5.6 Sol at xhigh reasoning, the caller-selected fallback because Luna was unavailable, coordinated as one sequential cohesive feature owner under autopilot.
- Review weight: `standard`. This implementation is ready for the independent feature review; the implementing worker did not self-close the feature.
- Completed child stories and commits:
  - `epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation` — `eb052a9`
  - `epic-temporal-video-artifacts-ffmpeg-runtime-discovery-and-qualification` — `538ef05`
  - `epic-temporal-video-artifacts-ffmpeg-runtime-encoder-port-adapter` — `58e9764`
- Added the unpublished `krometrail-ffmpeg` workspace crate with one fixed `libx264` v1 policy, private generated input/output staging, direct Tokio process ownership, bounded diagnostics/output, checked MP4/H.264/no-audio validation, deterministic discovery, produced-contract qualification, immutable safe identity, and an object-safe qualified core-port adapter.
- The process boundary uses no shell, wrapper, downloader, network client, native FFmpeg binding, or managed binary. Unix process groups plus `RLIMIT_CPU` and Windows kill-on-close Job Objects own cleanup; unsupported ownership fails qualification. Default tests use a compiled Rust fixture, and the real interoperability lane is feature-gated, ignored, and requires an explicit executable.
- The opt-in live lane exposed FFmpeg image-demuxer 25 fps timestamp quantization that the compiled fixture could not model. The fixed policy now supplies a 1 MHz input cadence and generated cumulative `setpts` values while retaining concat `-safe 1`; the real validator then proved the exact 350 ms track contract. This stayed within the adapter and required no core-contract change.
- No adjacent issue was discovered or parked. Windows Job Object behavior remains exercised by the shared platform-gated test source on Windows CI; this macOS implementation run exercised the Unix process-group path and live FFmpeg 8.0.1 path.

## Verification evidence

- `cargo fmt --all -- --check` — passed on the clean integrated workspace.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed across the workspace; only the existing explicitly manual/performance tests remained ignored.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- `cargo test -p krometrail-ffmpeg` — passed 32 deterministic tests and doc tests with no real FFmpeg or network.
- `KROMETRAIL_FFMPEG_PATH=/opt/homebrew/bin/ffmpeg cargo test -p krometrail-ffmpeg --features qualification-support --test ffmpeg_live -- --ignored --nocapture` — qualified FFmpeg 8.0.1 with `libx264` and produced a validated 1,508-byte H.264 MP4 at exactly 350 ms; output SHA-256 was `8b3905f2acd80fc1f4c2a476e8339ca0c17d79c0efa668ba0f6894f6fad2c762`.
- `cargo tree -p krometrail-ffmpeg` plus a lockfile audit found no FFmpeg binary, binding, wrapper, downloader, or network dependency; the only FFmpeg-named package is this workspace crate.
- No existing test was removed, weakened, or skipped to obtain these results.
- `.work/bin/work-view` is an x86-64 Linux executable in this checkout and cannot execute on the macOS host. Dependency readiness and child stages were therefore verified directly from item frontmatter; this did not block implementation or verification.

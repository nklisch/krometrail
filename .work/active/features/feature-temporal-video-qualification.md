---
id: feature-temporal-video-qualification
kind: feature
stage: implementing
tags: [temporal]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Temporal-video qualification failure diagnosis

## Brief

The optional temporal-video capability fails startup qualification on this
Linux host (Nobara/Fedora 43, kernel 7.0.9): diagnostics log at every server
start shows `capability.availability` for `temporal-video` as `unavailable`,
`qualification_stage: OutputValidation`, `reason: InvalidOutput`,
`restart_required_for_change: true`. The tool is correctly not advertised, so
agents cannot use video at all on this machine.

Worth root-causing: OutputValidation/InvalidOutput suggests the qualification
probe ran FFmpeg but rejected what it produced (encoder/pixel-format/container
mismatch on this distro's ffmpeg build?), rather than FFmpeg being absent.
Diagnostics currently record only the stage and reason — a bounded detail of
what validation failed (expected vs. observed properties, no raw stderr) would
make this diagnosable from the log alone.

Two deliverables:

1. Root-cause the qualification failure on this host (the design pass should
   reproduce the probe locally and identify exactly which validated property of
   the probe output diverges) and fix the product side if the validator is
   wrong for a legitimate FFmpeg build; if the local FFmpeg genuinely produces
   invalid output, the fix is diagnostics-only plus documented remediation.
2. Regardless of root cause: extend the qualification diagnostics with bounded
   detail — which validation check failed and the expected vs. observed
   property values (dimensions, codec, pixel format, duration, container),
   never raw FFmpeg stderr or paths beyond existing privacy bounds.

## Simplification opportunity

None identified beyond keeping the validator's property checks table-driven if
they are not already.

## Root cause

Reproduced empirically on this host (Nobara/Fedora 43, kernel 7.0.9).

**Environment.** The qualified executable is `/usr/bin/ffmpeg`, Fedora's
`ffmpeg version 7.1.2` (libavformat 61.7.100), built `--enable-gpl` with
`libx264` present as an encoder (`ffmpeg -encoders` lists `libx264`). So the
probe does *not* fail for a missing encoder — that would surface as
`EncodeProbe`/`UnsupportedEncoder`. The process exits 0 and produces an MP4;
the failure is a genuine output-property divergence at `OutputValidation`.

**Reconstructed probe.** `qualify_ffmpeg` → `qualify_candidate` →
`encode_validated` stages two 2x2 RGBA PNGs and one `frames.ffconcat`, then runs
the exact `encode_arguments` sequence from
`crates/krometrail-ffmpeg/src/policy.rs` (`-c:v libx264 -pix_fmt yuv420p
-video_track_timescale 1000000 -enc_time_base 1:1000000 -g 1 -movflags
+faststart -t 0.350000`, output `output.partial.mp4`). The intended timeline is
two source samples plus a synthetic terminal-hold sample: presentation PTS
`[0, 100000, 349999]` micros and expected sample deltas `[100000, 249999, 1]`
(the trailing `1` is the sentinel `push(1)` in `job.rs::quantize_boundaries`),
total duration `350000` micros.

**Divergence — exactly one property.** Running that invocation verbatim into the
scratchpad and parsing the resulting `moov`/`stbl` boxes:

| Property (validator check) | Expected | Observed (ffmpeg 7.1.2) | Verdict |
| --- | --- | --- | --- |
| container: 1×`ftyp`, 1×`moov`, `mdat`>0 | yes | yes | pass |
| video codec (`stsd`) | `avc1`/`avc3` | `avc1` (+`avcC`) | pass |
| video dimensions | 2×2 | 2×2 | pass |
| pixel format | (requested `yuv420p`) | `yuv420p` | pass |
| video tracks / audio tracks | 1 / 0 | 1 / 0 | pass |
| track timescale (`mdhd`) | 1000000 | 1000000 | pass |
| movie duration (`mvhd`, ts=1000) | ≈350 ms | 350 | pass |
| track duration (`mdhd`) | 350000 ±1 tick | 349999 | pass (within 1-tick tol) |
| `ctts` | absent | absent | pass |
| `stsz` sample count | 3 | 3 | pass |
| **`stts` sample deltas** | **`[100000, 249999, 1]`** | **`[100000, 249999, 0]`** | **FAIL** |

The only diverging property is the **terminal `stts` sample delta**: ffmpeg
7.1.2's `mov` muxer writes the final held sample's duration as **0** (making the
track duration `349999`), whereas the design expects the sentinel value **1**
(track duration `350000`). This trips the validator in `mp4.rs` twice:

1. `parse_stts` rejects *any* zero delta unconditionally
   (`if run == 0 || delta == 0 || …`), so parsing fails before comparison — this
   is the check that actually fires at runtime and yields `InvalidOutput`.
2. The top-level exact equality
   `movie.video_sample_durations == expected.sample_durations_micros` would also
   reject `…,0` ≠ `…,1`.

**Why it is a legitimate build, not a broken one.** The committed fixture
`crates/krometrail-ffmpeg/tests/fixtures/video/valid-h264.mp4` was generated
with **Homebrew ffmpeg 8.0.1 (macOS)**, whose `mov` muxer writes the terminal
delta as **1** (track duration `350000`) — which is why the validator passes on
the fixture but rejects real 7.1.2 output. The final sample of a held terminal
frame has no successor PTS, so its stored duration is muxer-inferred and varies
across FFmpeg builds. No encoder flag fixes it: I tested `-t 0.350000`,
`-t 0.350001`, dropping `-t`, and giving the trailing `ffconcat` frame an
explicit `duration` — every variant still emitted a terminal delta of `0` on
7.1.2. This matches VISUAL-EVIDENCE.md's stated posture that Krometrail makes no
byte-equality claim across FFmpeg builds/platforms; pinning a muxer-inferred
trailing duration over-constrains that boundary.

**Blast radius.** `encode_validated` is shared by qualification *and* real clip
encoding (`crates/krometrail-ffmpeg/src/encoder.rs:57`). So this same divergence
would reject every real temporal-video clip on ffmpeg 7.1.2 — qualification is
simply where it is first caught, correctly failing closed so the tool is not
advertised.

Reproduction artifact (scratch): the verbatim probe output is 1508 bytes,
sha256 `b3ac999ae30d653ea4ca1e19a5102154db9e0546d8f620794d84f5a4a4d32b50`,
`stts = [(1,100000),(1,249999),(1,0)]`. This is the exact byte sequence to
retain as the new terminal-zero fixture (see Testing).

## Architectural choice

### Options considered

- **A. Make the probe deterministic via encoder flags.** Rejected on evidence:
  no flag combination makes ffmpeg 7.1.2 emit a nonzero terminal delta, and the
  8.0.1 build already emits `1`, so there is no single deterministic value to
  request. The trailing held sample's duration is inherently muxer-defined.
- **B. Accept the legitimate variant in the validator (chosen).** Treat the
  final sample's stored duration as muxer-defined: validate all leading deltas
  exactly (those are the real inter-frame presentation intervals) and accept the
  terminal delta as either `0` or the sentinel value. This admits both the
  8.0.1 and 7.1.2 outputs without weakening any structural, codec, dimension,
  timescale, or total-duration guarantee. Consistent with the "no byte-equality
  across builds" contract.
- **C. Diagnostics-only + documented remediation.** Rejected as the primary fix:
  the local FFmpeg produces a *correct* clip; refusing it would leave the
  capability permanently unavailable on a mainstream distro for no correctness
  reason. Diagnostics are still delivered (deliverable 2), but as an addition,
  not the remedy.

**Chosen: B + diagnostics.** Relax exactly the terminal-sample check to accept a
muxer-defined final duration, and thread bounded which-check/expected-vs-observed
detail from the validator to the startup availability log via a single declared
table of validation checks (registry-declared-surfaces).

### Contract impact

- `FfmpegUnavailable` gains a public `output_check: Option<OutputValidationDetail>`
  field. This is an internal (unpublished) crate surface consumed only by
  `src/app.rs`; per Current Contract Discipline it is edited directly, not
  shimmed.
- No change to the MCP surface, capability schema, or persisted formats.
- No change to `encode_arguments` (the emitted FFmpeg command is unchanged); the
  fix is purely in output acceptance + diagnostics.

## Implementation Units

Single bundle. All paths absolute under `/home/nathan/dev/krometrail`.

### Unit 1 — Validation-check registry + bounded detail type

File: `crates/krometrail-ffmpeg/src/mp4.rs` (new public types near the top).

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mp4Check {
    OutputBytes,
    ContainerStructure,
    MovieDuration,
    VideoTrackCount,
    AudioTrackCount,
    VideoCodec,
    VideoDimensions,
    TrackTimescale,
    TrackDuration,
    SampleCount,
    SampleDurations,
}

impl Mp4Check {
    /// Stable snake-case identity used verbatim in the availability log.
    pub const fn name(self) -> &'static str { /* one arm per variant */ }
}

/// Bounded, privacy-safe property value. No stderr, no paths — container-level
/// integers, a 4-byte codec tag, or pixel dimensions only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mp4Property {
    Count(u64),
    Micros(u64),
    Codec([u8; 4]),
    Dimensions { width: u32, height: u32 },
    Sample { index: u16, micros: u64 },
    Bytes(u64),
    Absent,
}

impl std::fmt::Display for Mp4Property { /* count=…, micros=…, codec=avc1,
    2x2, sample[2]=0, bytes=…, absent — codec bytes rendered ASCII-or-hex */ }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputValidationDetail {
    pub check: Mp4Check,
    pub expected: Mp4Property,
    pub observed: Mp4Property,
}
```

All `Copy`, so `FfmpegUnavailable` stays `Copy`. Re-export
`Mp4Check`, `Mp4Property`, `OutputValidationDetail` from
`crates/krometrail-ffmpeg/src/lib.rs`.

### Unit 2 — Carry detail on `AdapterFailure`

File: `crates/krometrail-ffmpeg/src/error.rs`.

- Add field `pub(crate) output_check: Option<OutputValidationDetail>` to
  `AdapterFailure`; initialize `None` in `new`.
- Add `pub(crate) fn with_output_check(mut self, detail: OutputValidationDetail)
  -> Self`.
- Extend `trace()` to emit `failed_check`, `expected`, `observed` when present
  (bounded, via `Mp4Property`'s `Display`).

### Unit 3 — Accept the muxer-defined terminal sample + emit detail

File: `crates/krometrail-ffmpeg/src/mp4.rs`.

- `parse_stts`: drop the blanket `delta == 0` rejection from the per-entry guard
  (keep `run == 0` and the `MAX_MP4_SAMPLE_COUNT` bound). After run-expansion,
  reject if **any non-final** duration is `0` (a zero delta anywhere but the last
  sample means two frames at one instant). Keep the `durations.is_empty()`
  rejection. Signature unchanged: `fn parse_stts(data, control) -> Result<Arc<[u64]>, _>`.
- Replace the exact equality at the `video_sample_durations` comparison with a
  helper:

  ```rust
  fn match_sample_durations(
      observed: &[u64],
      expected: &[u64],
  ) -> Result<(), OutputValidationDetail>;
  ```

  Rule: lengths must match; every leading element (`0..len-1`) equal exactly;
  the terminal element must be `0` **or** the expected sentinel. On the first
  divergence return `OutputValidationDetail { check: SampleDurations,
  expected: Sample{index, micros}, observed: Sample{index, micros} }`.
- Split the combined boolean assertions (current `video_tracks/audio_tracks/
  codec/dimensions` block and both `validate_duration` calls) into individual
  named checks, each attaching an `OutputValidationDetail` on failure:
  `VideoTrackCount`, `AudioTrackCount`, `VideoCodec`, `VideoDimensions`,
  `MovieDuration`, `TrackDuration`, `TrackTimescale`, plus `OutputBytes` for the
  size guards. `validate_duration` gains a `check: Mp4Check` parameter and, on
  failure, reports `Micros(expected)` vs the observed duration renormalized to
  micros. Deep recursive-parse failures keep returning bare
  `failure(InvalidOutput)` (detail `None`) — they map to a coarse
  `ContainerStructure` signal, not a per-property one; the recursive walker is
  not rewritten.

Net behavior change: an MP4 whose only deviation is a terminal `stts` delta of
`0` (total `349999`) now validates; all other current rejections are preserved
and now additionally carry bounded detail.

### Unit 4 — Thread detail through qualification mapping

File: `crates/krometrail-ffmpeg/src/qualification.rs`.

- Add `pub output_check: Option<OutputValidationDetail>` to `FfmpegUnavailable`.
- `map_failure` copies `failure.output_check` into the returned
  `FfmpegUnavailable`.
- Set the new field on the other construction sites (`unavailable(…)` helper and
  the `final_failure` initializer) to `None`.

### Unit 5 — Startup availability log detail

File: `/home/nathan/dev/krometrail/src/app.rs`, `log_temporal_video_availability`.

Extend the `unavailable` branch with three bounded fields derived from
`unavailable.and_then(|u| u.output_check)`:

```rust
failed_check = unavailable
    .and_then(|value| value.output_check)
    .map(|detail| detail.check.name()),
expected_property = unavailable
    .and_then(|value| value.output_check)
    .map(|detail| detail.expected.to_string()),
observed_property = unavailable
    .and_then(|value| value.output_check)
    .map(|detail| detail.observed.to_string()),
```

No new imports beyond the re-exported types; no paths or stderr are logged.

### Unit 6 — Fixtures + provenance

Files under `crates/krometrail-ffmpeg/tests/fixtures/video/`.

- Add `terminal-hold-zero-h264.mp4`: the verbatim 7.1.2 probe output (1508
  bytes, sha256 `b3ac999a…4d32b50`, terminal `stts` delta `0`). Regenerate
  byte-identically on this host with the documented probe (2x2 black/white PNGs,
  the `frames.ffconcat` from `job.rs`, and the exact `encode_arguments`
  sequence); libx264 is deterministic for fixed input/params/build.
- Update `README.md` in that directory with the new fixture's provenance
  (ffmpeg 7.1.2, terminal-zero exemplar, sha256), mirroring the existing
  `valid-h264.mp4` entry. Keep both fixtures — they are the two muxer variants
  the validator must accept.

## Implementation Order

1. Unit 1 (types + re-exports) — no behavior change; compiles standalone.
2. Unit 2 (`AdapterFailure` field + builder + trace).
3. Unit 3 (validator relaxation + per-check detail) — the behavioral fix.
4. Unit 4 (`FfmpegUnavailable` field + mapping).
5. Unit 5 (app log).
6. Unit 6 (fixtures + README).
7. Tests (below) alongside the units they cover; run the Rust gate last.

## Simplification

- One `Mp4Check` enum is the single table driving both validation-check identity
  and log naming — no parallel string lists (registry-declared-surfaces).
- The terminal-sample rule lives in one helper (`match_sample_durations`); the
  `parse_stts` change is a single guard relocation, not a parser rewrite.
- No `encode_arguments` change, no MCP/schema/persistence change, no
  compatibility shim — the superseded exact-terminal expectation is replaced in
  place per Current Contract Discipline.
- Deep structural failures deliberately stay detail-less rather than
  instrumenting the recursive walker, keeping the diagnostic surface
  proportionate to what an operator can act on.

## Testing

Smallest useful set; hermetic (hermetic-release-boundary-fixtures /
layered-cdp-qualification — real-build artifacts retained as bytes, no network,
no user-home mutation).

Unit tests in `crates/krometrail-ffmpeg/src/mp4.rs` `#[cfg(test)]`:

- **accepts_muxer_defined_terminal_zero** — extend the `valid_mp4` helper to
  take the terminal delta; a variant with terminal delta `0` (track duration
  `349999`) validates OK. This is the regression that pins the fix.
- **rejects_zero_delta_on_non_terminal_samples** — a `0` delta on a leading
  sample still fails.
- **retained_terminal_zero_fixture_satisfies_validator** — read the new
  `terminal-hold-zero-h264.mp4` bytes and validate OK (real 7.1.2 artifact).
- **retained_real_ffmpeg_fixture_satisfies_the_same_validator** — unchanged;
  the 8.0.1 fixture (terminal `1`) still validates OK.
- **sample_duration_mismatch_reports_diverging_index** — a wrong *leading* delta
  yields `Err` with `check == SampleDurations` and `Sample { index, … }` naming
  the divergent position.
- **codec_mismatch_reports_expected_and_observed_fourcc** — extend the existing
  wrong-codec case to assert `check == VideoCodec`, `expected == Codec(avc1)`,
  `observed == Codec(hvc1)`.
- **dimension_mismatch_reports_expected_and_observed** — assert
  `check == VideoDimensions` detail on the existing wrong-dimensions case.

Adapter/integration in `crates/krometrail-ffmpeg/tests/` (fake-ffmpeg shadow):

- Add a `terminal-hold-zero` mode to
  `tests/support/fixture_main.rs` that writes the new fixture bytes, and a test
  in `tests/ffmpeg_adapter.rs` asserting `qualify_ffmpeg` returns
  `FfmpegQualification::Qualified` for it — proving the *whole* qualification
  path (not just `validate_mp4`) accepts 7.1.2-class output.
- Add a mode that writes a structurally-valid MP4 with wrong (4x2) dimensions,
  and assert the resulting `FfmpegUnavailable` carries
  `output_check == Some(detail)` with `check == VideoDimensions` — proving the
  bounded diagnostic reaches the mapped unavailable value that `app.rs` logs.

`src/app.rs` log wiring is covered indirectly by the mapping test above; the
formatting is exercised by `Mp4Property`'s `Display` unit assertions. No
app-level tracing test is added (low value, high harness cost).

## Risks

- **Over-permissive terminal acceptance.** Accepting a `0` terminal delta could
  in principle admit a malformed tail. Bounded by: only the *final* sample may be
  `0`; all leading deltas remain exact; total duration is still checked against
  the expected micros (±1 tick) via both `mvhd` and `mdhd`; and `stsz` sample
  count is still cross-checked against the `stts` sample count. A truncated or
  extra-sample tail changes the count or total and is still rejected.
- **Fixture is build-specific.** `terminal-hold-zero-h264.mp4` must remain a
  terminal-zero exemplar; regenerating it on a build that emits terminal `1`
  (e.g. 8.0.1) would silently turn it into a duplicate of `valid-h264.mp4`. The
  README provenance names the required build and sha to prevent that drift.
- **Public field addition.** `FfmpegUnavailable` gaining a field touches its
  construction sites and the `app.rs` log; all are in-tree and updated in Unit 4
  and Unit 5. No external consumer exists.
- **Determinism/provenance posture unchanged.** The fix does not alter
  `encode_arguments` or any manifest field; the clip's recorded encoder identity
  and presentation plan are unaffected. Accepting a muxer-defined terminal
  duration is consistent with the existing no-byte-equality-across-builds
  contract and does not weaken determinism *for a fixed encoder identity*.
- **Diagnostic content safety.** `Mp4Property` renders only container integers,
  a 4-byte codec tag, and pixel dimensions — no stderr, no filesystem paths, no
  frame content — staying inside existing privacy bounds.

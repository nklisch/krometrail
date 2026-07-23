---
id: feature-temporal-video-qualification
kind: feature
stage: drafting
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

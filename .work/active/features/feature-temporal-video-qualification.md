---
id: idea-temporal-video-qualification-failure
created: 2026-07-23
updated: 2026-07-23
tags: [temporal, capability]
---

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

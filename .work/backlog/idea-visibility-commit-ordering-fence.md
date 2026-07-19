---
id: idea-visibility-commit-ordering-fence
created: 2026-07-19
updated: 2026-07-19
tags: [browser]
---

Cross-model review finding (major, adjudicated as parkable) on
`feature-fix-preserve-visibility-wedge`: the activation visibility write-back
(`operations.rs` commit of `SupervisorInput::VisibilityChanged { Visible }`) can be
overwritten by a stale queued visibility event that was captured before activation but
reduced after it (`runtime.rs` queue drain; `reducer.rs::visibility_changed` accepts
inputs without freshness ordering). Practical impact is low: the window needs a queued
pre-activation event while capture was already running, the failure mode is one
recoverable `target_hidden` on the next pointer op, and the running screencast stream
re-emits Visible and self-heals. Fix direction if promoted: add a monotonic observation
sequence (or generation fence) to visibility inputs so older observations cannot
overwrite newer ones, plus a deterministic race test proving an activated target stays
visible and recording. Single-writer reducer remains the sole authority.

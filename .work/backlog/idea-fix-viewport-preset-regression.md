---
id: idea-fix-viewport-preset-regression
created: 2026-07-18
updated: 2026-07-18
tags: []
---

Krometrail 1.1.0 reproducibly fails to apply `responsive_small` to its own public documentation at `https://nklisch.github.io/krometrail/`. A temporary managed Chrome 150 session navigated successfully, but `set_viewport {"viewport":{"mode":"preset","preset":"responsive_small"}}` returned `target_failed` with `browser did not apply the requested viewport metrics`. Following the advertised recovery by reloading the target and retrying produced the same failure. Correlations: `3684395d-dc57-413d-959d-bdb7cdc679d8` and `8f341cd6-ff4c-417e-b718-410b6c71273b`. The bounded diagnostics show `set_viewport` failing at the operation stage and live observation degrading; they do not expose a lower-level cause. In the same pass, the Codex in-app and Chrome-extension viewport surfaces both applied 390×844 successfully and observed the expected small breakpoint.

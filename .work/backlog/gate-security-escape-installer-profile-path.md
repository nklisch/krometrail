---
id: gate-security-escape-installer-profile-path
created: 2026-07-15
updated: 2026-07-15
tags: [security, infra]
gate_origin: security
release_binding: null
---

Harden automatic PATH-profile updates when a user supplies a custom install directory containing control characters, whitespace, or shell metacharacters. Validate or shell-escape the literal path separately for POSIX shells and fish while retaining automatic PATH setup for ordinary safe local paths. This is local self-input hardening and does not block the 1.0 release.

---
id: feature-boundary-hardening
kind: feature
stage: drafting
tags: [security, browser, distribution, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Boundary hardening: redaction, installer paths, upload policy, release immutability

## Brief

Cluster of five parked security-gate items. All are defense-in-depth on the
local-tool boundary — none is a demonstrated active leak — but they share one
theme: the boundary contract should be enforced or stated explicitly rather than
left implicit.

Absorbed items:

- **`gate-security-redact-nested-browser-event-secrets`** — browser-event text
  redaction can miss a sensitive key nested inside a compact JSON fragment such
  as `{"outer":{"token":"secret"}}`. The bounded whitespace/token sanitizer is
  not structure-aware. Add structure-aware regression coverage while retaining
  bounded useful console evidence.
- **`idea-redact-windows-drive-relative-paths`** — extend the redactor's path
  corpus to Windows drive-relative forms (`C:foo`) and rooted single-backslash
  paths (`\Users\alice\file`). Table-driven, without weakening existing URL,
  credential, query, fragment, POSIX, UNC, or absolute-drive redaction.
- **`gate-security-escape-installer-profile-path`** — harden automatic
  PATH-profile updates when the install directory contains control characters,
  whitespace, or shell metacharacters. Validate or shell-escape the literal path
  separately for POSIX shells and fish, retaining automatic PATH setup for
  ordinary safe paths.
- **`idea-upload-symlink-policy`** — decide and *document* the upload symlink
  policy. Today the boundary canonicalizes operator-supplied paths and follows
  symlinks. Either retain that local-operator-authority contract and state it
  clearly, or introduce an explicit configured upload root with containment
  checks. Do not add an implicit working-directory root, and do not claim
  containment the runtime does not enforce.
- **`gate-security-enforce-immutable-plugin-releases`** — enforce immutable
  plugin binary releases, consistent with the existing
  `exact-release-managed-activation` pattern.

## Simplification opportunity

The two redaction items are one corpus extension against one sanitizer; do them
as a single table-driven change, not two. The upload-symlink item may resolve to
*documentation only* — an explicit statement of the existing contract is a valid
terminal outcome and is preferable to inventing a containment root that the
local-tool threat model does not earn. Record that as a design decision rather
than assuming code must change.

## Acceptance

- Structure-aware nested-secret redaction with regression coverage.
- Windows drive-relative and rooted-backslash paths redacted; existing corpus
  unweakened.
- Installer PATH updates safe for hostile literal paths, ordinary paths still
  automatic.
- Upload symlink policy explicitly decided and documented; runtime claims match
  runtime behavior.
- Plugin release immutability enforced.

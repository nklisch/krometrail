---
id: feature-boundary-hardening
kind: feature
stage: review
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

## Implementation

Done directly rather than through a design pass; each item was small and the
evidence was already in the brief.

- **Installer PATH quoting** (`scripts/install.sh`). The defect was worse than
  the brief described: the POSIX line interpolated the install directory inside
  double quotes, so a quote, `$`, or backtick escaped the literal — and the fish
  line emitted it **entirely unquoted**, where whitespace alone splits it. A
  hostile `--install-dir` could inject shell code into `.bashrc`, `.zshrc`,
  `.profile`, or `config.fish`. Added `posix_shell_quote` and
  `fish_shell_quote`, used for both emitted lines and the manual-instruction
  hint. Verified against a hostile corpus — spaces, double and single quotes,
  `$HOME`, `$(id -u)`, backticks, `;`-injection, backslashes, and embedded
  newlines all round-trip to the exact literal, and an injection attempt no
  longer executes.

- **Nested-secret and Windows-path redaction**
  (`crates/krometrail-core/src/browser/privacy.rs`). The sanitizer splits on
  whitespace, so `{"outer":{"token":"secret"}}` was one token and only the
  outermost key was ever inspected. Added a structure-aware pass that yields to
  the existing URL/path handling unless it actually redacted something. Cross-model
  review then found three further defects, all confirmed by probe before fixing:
  a quoted value containing whitespace leaked everything after the first token;
  `access_token` / `refresh_token` / `client_secret` were not recognised at all
  (`normalize_key` collapses to `accesstoken`); and the single-letter drive
  heuristic redacted ordinary prose such as `A:todo`. All three fixed with
  regressions pinning the exact inputs.

- **Upload symlink policy** — resolved as **documentation, no code change**.
  `docs/SPEC.md` now states the contract plainly: upload canonicalizes
  caller-supplied paths, follows symlinks, and applies no containment root.
  Rationale recorded there — the caller already holds the operator's filesystem
  access, and upload paths are always caller-supplied, so a page cannot influence
  which path is uploaded. A containment root would remove legitimate local
  uploads while adding no boundary the caller could not already cross directly.
  Krometrail claims no containment guarantee rather than implying one it never
  enforced.

- **Immutable plugin releases** — partially done. The release workflow now
  publishes **draft-first**: create the release as a draft, assert the asset set
  exactly matches the expected matrix and that it is still a draft, verify tag
  identity, publish, then verify identity again. Static distribution contracts
  assert the lifecycle so a future edit cannot regress to direct publication.
  This closes the window where an exact-version consumer could observe a
  partially uploaded release.

  **Not done, and it needs the repository owner:** GitHub repository-level
  immutable releases are still disabled (`immutable_releases: null` via the API).
  That is the control that actually prevents post-publication replacement of a
  binary *and* its checksum under an unchanged tag, which is the threat the
  original gate item named. Draft-first does not substitute for it. Enabling it
  is a repository setting, not a workflow change.

## Risks

- Redaction remains heuristic and token-based. It is defence in depth over
  bounded console evidence, not a guarantee; a sufficiently unusual encoding can
  still pass through. The corpus is the contract, so extend it when a new shape
  is observed rather than assuming coverage.

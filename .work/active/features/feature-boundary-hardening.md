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
updated: 2026-07-21
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

## Second cross-model review round: five redaction leaks

Five inputs reached the redactor and came out carrying their secret. All five are
now regressions in `crates/krometrail-core/src/browser/privacy.rs`
(`secrets_do_not_escape_through_nesting_escaping_or_quoting` and
`windows_drive_paths_redact_without_swallowing_colon_separated_prose`), each
confirmed to leak against the previous implementation.

1. **Nested values under a sensitive key.** `{"outer":{"token":{"inner":"LEAK"}}}`
   descended into the nested object and judged it on the inner key, which is not
   sensitive. `redact_structured_token` is now depth-aware: a sensitive key whose
   value has not started yet governs the whole structure that follows, which is
   replaced entire and consumed with its closing delimiter so brackets stay
   balanced.
2. **Escaped keys.** `normalize_key` filtered characters without decoding, so a
   key spelled `"tok\u0065n"` normalized to `toku0065n` and missed the sensitive
   set. It now decodes escapes first; named single-character escapes decode to
   their real characters rather than contributing a stray letter.
3. **Single-quoted spaced values.** `{'token':'secret value'}` leaked everything
   after the first space, because only double quotes opened a continuation. The
   continuation now carries the quote character, either style.
4. **Escaped embedded quotes.** `{"token":"one \" two"}` ended the continuation at
   the escaped quote. `find_unescaped_quote` threads escape state across tokens,
   because a quoted value spans the whitespace between them.
5. **Windows path regression.** Narrowing the drive-designator heuristic to
   require a separator stopped `A:todo` being redacted but also stopped
   `C:secret.txt`. The discriminator is now a filename-extension signal in
   addition to a separator: a non-empty stem, a dot, and a short suffix carrying
   at least one letter. Both directions are asserted — `C:secret.txt` and
   `C:foo\bar` redact; `A:todo`, `note:something`, `status:ok`, and `v:1.0` do
   not.

One interaction worth naming: `[redacted]` opens with a structural delimiter, so
the new nested-value branch had to be guarded against treating the redactor's own
output as a nested value. Re-running the redactor over its output is a no-op, and
`RedactedText::new` depends on that.

## Third cross-model review round: idempotence was not actually held

The guard named at the end of the previous round did not hold, and the failure was
functional rather than cosmetic. `is_redacted_placeholder` accepts `[redacted]`
followed only by punctuation, so a value such as `[redacted]-ok` was *not*
recognised as the redactor's own output. The nested-value branch then replaced it
with the identical placeholder: text unchanged, one redaction reported.

That combination is exactly what `RedactedText::new` rejects. `EventRedactor`
builds through `from_redactor` and skips validation, but the `Deserialize` impl
calls `new`. So a console message containing `token:[redacted]-ok` was written to
the browser-event store successfully and then failed to deserialize — the retained
evidence became permanently unreadable. Reproduced before fixing:

    input  "token:[redacted]-ok"
    output "token:[redacted]-ok"  (unchanged, redaction_count 1)
    RedactedText::new(...) -> Err("event text has not passed the privacy redactor")

Three distinct causes, all in `crates/krometrail-core/src/browser/privacy.rs`:

1. **The placeholder guard was the wrong test.** `redact_structured_token` now
   detects an already-redacted value by `strip_prefix(REDACTED_VALUE)` and resumes
   the scan past it without counting. Text between the placeholder and the next
   structural delimiter is *not* part of it, so it is dropped and counted — which
   also closes a leak the old guard left open:
   `{"a":1,"token":[redacted]MYSECRET}` used to emit `MYSECRET` verbatim, because
   the depth-consuming branch terminated at the placeholder's own `]`.
2. **The quoted-value continuation emitted an orphaned quote.** On closing a
   secret that spanned whitespace, the closing quote was re-emitted while its
   opening partner had been swallowed by the replacement, so
   `{'token':'secret LEAK'}` produced `{'token':[redacted]'}`. A later pass reads
   that stray quote as trailing content and strips it — unstable output. The
   closing quote is now consumed with the value it closes.
3. **Truncation could split a stable token into an unstable one.** Truncation runs
   after redaction, so cutting `Authorization: Bearer [redacted]` at 87 bytes left
   `Authorization: Bearer [` — a bare opening bracket the next pass reads as an
   unredacted value. Same deserialization failure, different route.
   `truncate_to_redaction_stable` now shrinks to the longest prefix the redactor
   leaves alone. It terminates because the empty string is always stable.

The invariant is now pinned by
`redaction_is_idempotent_so_persisted_evidence_stays_readable`, which runs over a
shared `REDACTION_CORPUS` constant covering every input any redaction test uses,
plus the new placeholder shapes. For each input it asserts the second pass changes
nothing, reports zero redactions, and that `RedactedText::new` accepts the first
pass's output. The comment states why: idempotence is what keeps persisted
evidence legible, not a style preference.
`truncated_redactor_output_is_still_accepted_by_its_own_validator` sweeps every
truncation length over the same corpus. Both failed before the fix.

### Dotfile paths and double-decoded keys

- **`C:.bashrc` was not redacted.** `ends_with_filename_extension` requires a
  non-empty stem, and a dotfile has none, so a drive-relative path to a user's
  shell configuration was emitted verbatim. Added `is_dotfile_name`: a leading
  dot, then a name of alphanumerics, dots, dashes, or underscores carrying at
  least one letter. Both directions are in the corpus — `C:.bashrc`,
  `C:.config\x`, and `C:.config/x` redact; `A:todo`, `v:1.0`, `note:something`,
  `status:ok`, and `C:.5` do not. *Correction to the review item:* `C:.config/x`
  and `C:.config\x` already redacted before the fix, because the remainder
  contains a path separator; only the separator-free dotfile form was missed.
  They are pinned anyway so the distinction is not rediscovered.
- **`normalize_key` decoded escapes once.** A key spelled `tok\\u0065n` decoded to
  the literal text `e`, whose letters compare as `toku0065n` and miss the
  sensitive set. Chose a **bounded fixed-point loop** (four passes) over rejecting
  keys with residual escape syntax: the only error looping can introduce is
  over-normalizing a key, which redacts a value that did not need it, and that is
  the safe direction for a redactor. Rejecting would have meant either dropping
  legitimate keys or redacting far more broadly. `decode_escape` now yields a real
  backslash for `\\` rather than dropping it, which is what makes a second pass
  see the escape hiding behind it. Pinned by a new case in
  `secrets_do_not_escape_through_nesting_escaping_or_quoting`.

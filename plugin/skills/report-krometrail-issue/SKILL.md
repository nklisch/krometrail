---
name: report-krometrail-issue
description: >
  Prepare and optionally submit privacy-safe Krometrail defect reports to the canonical GitHub
  repository with authenticated gh. Use when a user asks to report, file, or draft a Krometrail bug,
  especially from a failed or degraded MCP response with a diagnostic correlation identifier.
---

# Report a Krometrail issue

Target `nklisch/krometrail` regardless of the current repository or working directory. This workflow
has one external-write boundary: never create or comment on an issue until the user has reviewed the
complete draft and explicitly confirmed that write.

## 1. Verify reporting access

Run:

```bash
gh auth status
```

If authentication is unavailable, prepare the draft but do not attempt a write. Do not change GitHub
authentication or scopes unless the user separately requests it.

## 2. Collect the minimum reproducible facts

Gather:

- Krometrail version (`krometrail --version`, or the exact managed release reported by the plugin)
- OS and architecture (`uname -srm`)
- MCP tool/operation and stable error code
- expected behavior and actual behavior
- shortest reliable reproduction, including whether it reproduces from a clean session/profile
- `diagnostics.correlation_id` and `diagnostics.log_path`, when the failed/degraded response supplied
  them
- capture state/gaps only when the report concerns retained temporal evidence

Ask for missing expected-versus-actual or reproduction detail. Do not infer private page content.

## 3. Extract only sanitized diagnostics

Use the exact returned log path; do not search the user's home directory for logs. Search for the one
correlation identifier with a narrow context window:

```bash
rg -n -m 5 -C 2 --fixed-strings '<correlation-id>' '<diagnostics.log_path>'
```

Do not paste raw lines into the issue. Transcribe at most 20 relevant entries and retain only:
timestamp, severity, event name, route, outcome, stable error code, failure stage, and correlation ID.
Remove all other fields. Never include a whole log, browser text/title/content, form or prompt values,
screenshots/image bytes, cookies, tokens, credentials, request/response headers, raw CDP traffic,
filesystem paths other than the fact that a private log existed, or unredacted URLs. Reduce any needed
URL to origin plus a redacted path, and omit it when origin is not material.

If safe extraction is uncertain, omit diagnostics and report the correlation ID alone.

Before any GitHub command, apply the same privacy filter to every outbound field: search query,
title, body, labels, and comments. Never send free-form page symptoms, page text, user values, local
paths, or URLs in a duplicate query. Build the query only from the stable operation name, stable
error code, and one generic class from this fixed set: `startup`, `capture`, `interaction`, `schema`,
or `shutdown`. Omit any field that cannot be reduced safely.

## 4. Search for duplicates

Search open and closed issues using only the sanitized operation, stable code, and generic class:

```bash
gh issue list --repo nklisch/krometrail --state all --limit 100 --search '<operation> <error-code> <generic-class>'
```

Inspect plausible matches with:

```bash
gh issue view <number> --repo nklisch/krometrail --comments
```

If an issue covers the same behavior and environment, return its link and explain the match. Do not
create a duplicate or add a comment without a separate explicit request and confirmation. If the new
case differs, state the distinguishing condition in the draft.

## 5. Draft for review

Use a concise title: `<area>: <observable failure>`. Draft this body:

```markdown
## Environment
- Krometrail: <version>
- Platform: <OS/architecture>
- Operation: <tool>
- Error code: <stable code or none>
- Correlation: <id or unavailable>

## Expected
<observable result>

## Actual
<observable result and action/live/capture distinction>

## Reproduction
1. <minimal step>
2. <minimal step>

## Sanitized diagnostics
<bounded whitelist-only entries, or "Not included; correlation retained locally.">

## Duplicate search
<queries/candidates and why this differs>
```

Show the complete title and body to the user. Explicitly ask whether to create this issue in
`nklisch/krometrail`. A request to "draft" is not confirmation to submit.

Before showing the draft, re-run the outbound privacy filter over every field. Replace sensitive
facts with stable codes or generic descriptions rather than relying on the user to notice them.

## 6. Submit only after confirmation

After explicit confirmation, put the approved body in a private temporary file, then run:

```bash
gh issue create --repo nklisch/krometrail --title '<approved-title>' --body-file '<private-temp-file>'
```

Delete the temporary file and return the created issue URL. If submission fails, report the failure
without silently retrying with broader content or a different repository.

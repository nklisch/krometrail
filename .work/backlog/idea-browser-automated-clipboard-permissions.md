---
id: idea-browser-automated-clipboard-permissions
created: 2026-08-15
updated: 2026-09-05
tags: [browser, security]
---

# Automated Clipboard Permissions Grant for Managed Browser Sessions

## The Finding
Calling `write_clipboard` or `read_clipboard` in a managed browser session repeatedly fails with:
`write_clipboard failed [interaction_failed]: browser clipboard permission denied the explicit request. Recovery: focus the managed page, allow clipboard access in Chrome, and retry (retry: after_recovery)`

Because managed browser sessions run automated headless or background workflows, there is no interactive user present to click browser permission dialog prompts. Consequently, clipboard tools are effectively unusable in unattended MCP sessions.

## Review disposition — 2026-09-05

The user-observed failures above remain useful evidence, but a permission-denied message is not proof of the underlying permission state. Review at `eb5b4656` found that clipboard exception classification can misread the CDP response shape and label focus/secure-context failures as permission denial. That correction belongs to [clipboard error classification](epic-a-grade-reliability-clipboard-error-shape.md); this item retains the distinct unattended-permission policy question.

## Proposed direction and safety boundaries

- Verify actual permission, focus, secure-context, browser mode, and managed-versus-attached ownership behavior before selecting a mechanism. Do not assume the previously suggested launch flag exists or fixes the problem without browser-version evidence.
- **Do not use `--disable-web-security` as a clipboard workaround**, including for temporary profiles that may navigate real sites. It disables unrelated protections and is not a scoped clipboard permission mechanism.
- Evaluate a supported, origin/context-scoped CDP permission mechanism only with explicit caller authority and a defined lifecycle. Do not blanket-grant clipboard reads during every session start or silently modify an attached external browser's policy.
- If permission policy becomes configurable, report its effective state and verify apply, failure, navigation/origin change, clear, and shutdown behavior. Permission grants do not by themselves guarantee OS focus or settle a pending browser prompt.
- Preserve clipboard privacy: no raw content in logs, no silent reads, and no host-clipboard access implied merely by launching a browser.

This remains unimplemented backlog work, not a settled design. [A-grade operational reliability](epic-a-grade-reliability.md) links it as existing related work rather than creating a duplicate permission feature.

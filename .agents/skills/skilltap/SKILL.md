---
name: skilltap
description: Manage agent skills with the skilltap CLI — install, update, remove, and list skills from any git host or npm.
license: MIT
---
# skilltap

Installs, updates, and removes agent skills. Designed for agent-mode operation: no prompts, plain-text output, hard-fail on security warnings.

## Before using any other command

Run `skilltap status` first. If `agent-mode` is disabled, **stop immediately** and tell the user:

> "skilltap agent mode is not enabled. Please run `skilltap config agent-mode` to configure it, then try again."

Do not attempt to install, update, or remove skills until agent mode is enabled.

## Status

```bash
skilltap status        # plain text key:value
skilltap status --json # JSON
```

Fields:
- `agent-mode` — `enabled` or `disabled`
- `scope` — `project` or `global` (the default scope for operations)
- `scan` — `static`, `semantic`, or `off`
- `agent` — configured LLM agent for semantic scan (or `null`)
- `also` — agent dirs to symlink into (e.g. `claude-code cursor`)
- `taps` — number of configured taps

## Install

```bash
skilltap install <source>
skilltap install <source> --project
skilltap install <source> --global
```

Exits 1 immediately if security warnings are found. All skills in multi-skill repos are installed automatically.

Source formats:
- `user/repo` — GitHub shorthand
- `github:user/repo` — GitHub explicit
- `https://host/u/r` — any git URL
- `git@github.com:user/repo.git` — SSH
- `skill-name` — tap name (requires a tap to be configured)
- `skill-name@v1.2.0` — tap name at ref
- `npm:@scope/pkg` — npm registry
- `npm:@scope/pkg@1.2.3` — npm pinned

Flags:
- `--project` / `--global` — override configured scope
- `--ref <ref>` — branch or tag
- `--also <agent>` — also symlink into agent dir; repeatable (`claude-code`, `cursor`, `codex`, `gemini`, `windsurf`)
- `--semantic` — run LLM-based semantic security scan (requires agent config)

Do not pass `--yes` (no-op), `--strict` (forced), or `--skip-scan` (rejected with error) in agent mode.

## Update

```bash
skilltap update               # all installed skills
skilltap update <name>        # one skill
skilltap update --semantic    # with semantic scan
```

Security warnings on update are reported but do not block — the update is skipped for that skill and the run continues.

## Remove

```bash
skilltap remove <name>
```

No confirmation prompt in agent mode.

## List

```bash
skilltap list         # table: name, scope, source, trust, date
skilltap list --json  # JSON array
```

## Info

```bash
skilltap info <name>  # source, path, SHA, trust tier, symlinks
```

## Output format

Plain text, one line per skill:
- `OK: Installed <name> → <path>`
- `OK: <name> is already up to date.`
- `SKIP: <name> is linked.`
- `ERROR: <message>` (stderr)
- `SECURITY ISSUE FOUND — INSTALLATION BLOCKED` followed by details (stderr, exit 1)

## Exit codes

- `0` — success
- `1` — error or security block
- `2` — cancelled

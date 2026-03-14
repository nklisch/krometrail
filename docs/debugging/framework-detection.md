---
title: Framework Detection
description: How Krometrail auto-detects pytest, jest, mocha, Django, Flask, and go test.
---

# Framework Detection

Krometrail detects which test framework or web framework is being used and adjusts its behavior accordingly — so agents don't need to configure debug launch settings for common cases.

## Detected Frameworks

<!--@include: ../../.generated/frameworks.md-->

## How Detection Affects Behavior

**pytest** — the adapter warns about incompatible modes like pytest-xdist (`-n`) and `--forked`, which spawn parallel workers that can't be individually debugged.

**Django** — the adapter adds `--nothreading --noreload` to the `runserver` command and sets `PYTHONDONTWRITEBYTECODE=1`. Django's auto-reloader and threading conflict with debugpy.

**Flask** — the adapter adds `--no-reload` and sets `WERKZEUG_RUN_MAIN=true` and `FLASK_DEBUG=0` to prevent the Werkzeug reloader from forking, which would break the DAP connection.

**jest** — the adapter adds `--runInBand` to run tests serially. Jest workers run in separate processes that can't be individually debugged.

**mocha** — detected and surfaced in the viewport/logs. Mocha runs in the same process, so no special configuration is needed.

**go test** — detected and surfaced in the viewport/logs. The Go adapter's `parseGoCommand` handles `go test` → `mode: "test"`. Tips about `-count=1` to disable test caching during debugging.

## Overriding Detection

If detection produces incorrect behavior, override the language explicitly:

```bash
krometrail launch "cargo test" --language rust
```

```json
{
	"command": "cargo test",
	"language": "rust"
}
```

## Manual Framework Configuration

For frameworks not auto-detected, or for custom launch configurations, the adapter uses the raw command as provided. The debugger will still attach correctly — you may just need to ensure the framework is configured to wait for the debugger.

For Python frameworks not in the list:
```bash
# Launch with debugpy wait explicitly
krometrail launch "python -m debugpy --wait-for-client my_framework_command"
```

## Checking Detection

Run `krometrail doctor` to see which adapters are installed. When you launch, the session status message confirms which adapter was selected:

```
Session started: abc123
Adapter: python (debugpy 1.8.0)
Framework: pytest (auto-detected)
```

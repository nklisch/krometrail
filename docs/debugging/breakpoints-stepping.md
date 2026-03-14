---
title: Breakpoints & Stepping
description: Set breakpoints with conditions, hit counts, and logpoints. Step over, into, and out of functions.
---

# Breakpoints & Stepping

## Setting Breakpoints

::: code-group

```bash [CLI]
# Simple line breakpoint
krometrail debug break order.py:147

# Multiple lines in one file
krometrail debug break order.py:147,150,155

# Conditional breakpoint — only fires when expression is true
krometrail debug break "order.py:147 when discount < 0"

# Hit count — fires after N hits
krometrail debug break "order.py:147 hit >=100"

# Logpoint — logs a message instead of stopping
krometrail debug break "order.py:147 log 'discount={discount}, total={total}'"

# Exception breakpoints
krometrail debug break --exceptions uncaught
krometrail debug break --exceptions raised    # Python: all raised exceptions
```

```json [MCP: debug_set_breakpoints]
// Simple
{
	"session_id": "...",
	"file": "order.py",
	"breakpoints": [{ "line": 147 }]
}

// Conditional
{
	"session_id": "...",
	"file": "order.py",
	"breakpoints": [{ "line": 147, "condition": "discount < 0" }]
}

// Hit count
{
	"session_id": "...",
	"file": "order.py",
	"breakpoints": [{ "line": 147, "hit_condition": ">=100" }]
}

// Logpoint
{
	"session_id": "...",
	"file": "order.py",
	"breakpoints": [{ "line": 147, "log_message": "discount={discount}, total={total}" }]
}
```

:::

::: warning DAP semantics
`debug_set_breakpoints` **replaces** all existing breakpoints in the specified file. To keep existing breakpoints, include them in the call alongside new ones.
:::

## Setting Breakpoints at Launch

Pass initial breakpoints with `debug_launch` to avoid a separate call:

```json
{
	"command": "python -m pytest tests/test_order.py -x",
	"breakpoints": [
		{ "file": "order.py", "line": 147 },
		{ "file": "discount.py", "line": 23, "condition": "tier == 'gold'" }
	]
}
```

## Listing Breakpoints

```bash
# CLI
krometrail debug breakpoints

# MCP: debug_list_breakpoints
{ "session_id": "..." }
```

Returns all active breakpoints with current hit counts.

## Removing Breakpoints

```bash
# Remove all breakpoints in a file
krometrail debug break --clear order.py

# To remove specific breakpoints, set the file with only the ones you want to keep
krometrail debug break order.py:150,155
# (line 147 is now removed)
```

## Stepping

::: code-group

```bash [CLI]
# Step over — execute current line, stay in current function
krometrail debug step over

# Step into — enter the function being called
krometrail debug step into

# Step out — run to end of current function, return to caller
krometrail debug step out

# Step multiple times
krometrail debug step over --count 5
```

```json [MCP: debug_step]
{ "session_id": "...", "direction": "over" }
{ "session_id": "...", "direction": "into" }
{ "session_id": "...", "direction": "out" }
{ "session_id": "...", "direction": "over", "count": 5 }
```

:::

Every step returns the viewport snapshot at the new location.

## Run to Line

Run to a specific line without setting a permanent breakpoint:

::: code-group

```bash [CLI]
krometrail debug run-to order.py:155
```

```json [MCP: debug_run_to]
{ "session_id": "...", "file": "order.py", "line": 155 }
```

:::

## Continue

Resume execution until the next breakpoint or program exit:

::: code-group

```bash [CLI]
krometrail debug continue
krometrail debug continue --timeout 10000
```

```json [MCP: debug_continue]
{ "session_id": "..." }
{ "session_id": "...", "timeout_ms": 10000 }
```

:::

## Exception Breakpoints

::: code-group

```bash [CLI]
krometrail debug break --exceptions uncaught    # only uncaught exceptions
krometrail debug break --exceptions raised      # all raised exceptions (Python)
krometrail debug break --exceptions all         # all exceptions (JS)
```

```json [MCP: debug_set_exception_breakpoints]
{ "session_id": "...", "filters": ["uncaught"] }
{ "session_id": "...", "filters": ["raised"] }
```

:::

## Tips

- **Prefer conditional breakpoints over stepping through loops** — `when i == 99` is far more efficient than stepping 99 times
- **Logpoints as non-intrusive probes** — logpoints don't stop execution, making them useful for monitoring without disrupting the control flow
- **Re-launch with earlier breakpoints** — if you step past the bug site, stop the session and re-launch with an earlier breakpoint rather than trying to navigate backwards

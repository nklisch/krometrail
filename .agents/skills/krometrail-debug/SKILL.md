---
name: krometrail-debug
description: Runtime debugging via the krometrail CLI. Use when a test fails and reading the code isn't enough, when you need to inspect runtime values, or when a bug is in logic you can't trace statically. Set breakpoints, step through code, inspect variables, evaluate expressions — across 10 languages.
license: MIT
compatibility: Requires debugger binaries for the target language (e.g., debugpy for Python, dlv for Go).
metadata:
  author: krometrail
  version: "0.2"
allowed-tools: Bash(krometrail:*)
---

# Krometrail — Runtime Debugging (CLI)

Use `krometrail debug` commands when you need to inspect runtime state to diagnose a bug — especially when static code reading and test output aren't enough to identify the root cause.

## When to use

- A test fails but the code looks correct — inspect runtime values at the failure point
- You suspect a wrong calculation or off-by-one — set a breakpoint and check locals
- A function returns an unexpected value — step into it and trace the data flow
- An exception occurs deep in a call chain — break on exceptions to see the exact state

## Quick start

```bash
krometrail debug launch "python3 -m pytest test_discount.py -x" --break discount.py:12
# → Viewport shows source, locals, and stack at line 12

krometrail debug eval "rate"
# → rate = 1.0  (should be 0.1!)

krometrail debug stop
# Fix the bug with confidence
```

## Commands

### Launch and lifecycle

```bash
krometrail debug launch "python app.py"
krometrail debug launch "pytest tests/test_order.py -s" --break order.py:147
krometrail debug launch "node server.js" --stop-on-entry
krometrail debug launch "go run ./cmd/server"
krometrail debug launch --config-name "My Config"          # from .vscode/launch.json

krometrail debug attach --language python --port 5678
krometrail debug attach --language node --port 9229
krometrail debug attach --language go --pid 12345

krometrail debug status
krometrail debug stop                                      # always call when done
```

### Execution control

```bash
krometrail debug continue [--timeout <ms>]
krometrail debug step over
krometrail debug step into
krometrail debug step out
krometrail debug step over --count 5
krometrail debug run-to order.py:150
```

### Breakpoints

```bash
krometrail debug break order.py:147
krometrail debug break order.py:147,152,160
krometrail debug break "order.py:147 when discount < 0"
krometrail debug break "order.py:147 hit >=5"
krometrail debug break "order.py:147 log processed {count} items"
krometrail debug break --exceptions uncaught
krometrail debug break --clear order.py
krometrail debug breakpoints                    # list all
```

**Breakpoint spec:** `file:line[,line,...] [when <expr>] [hit <n>] [log <msg>]`

Exception filters: Python: `raised`, `uncaught`, `userUnhandled` · Node.js: `all`, `uncaught` · Go: `panic`

### State inspection

```bash
krometrail debug vars                           # local scope
krometrail debug vars --scope global
krometrail debug vars --filter "^user" --frame 2
krometrail debug stack
krometrail debug stack --frames 5 --source
krometrail debug source order.py
krometrail debug source order.py:140-160
krometrail debug eval "cart.total"
krometrail debug eval "order.total" --frame 1 --depth 3
```

### Session intelligence

```bash
krometrail debug watch "order.total" "cart.item_count"
krometrail debug unwatch "cart.item_count"
krometrail debug log                            # review investigation history
krometrail debug log --detailed
krometrail debug output                         # stdout/stderr from the target
krometrail debug output --stderr
krometrail debug threads                        # goroutines, Python threads, etc.
```

## Language support

Each language has specific setup requirements and features:

- [Python](references/python.md) — debugpy, pytest/Flask/Django auto-detection
- [JavaScript / TypeScript](references/javascript.md) — js-debug, Jest/Mocha, attach via `--inspect`
- [Go](references/go.md) — Delve, go run/test/exec modes, goroutine threads
- [Rust](references/rust.md) — CodeLLDB, cargo run/test, auto-build
- [C/C++](references/cpp.md) — GDB v14+ / LLDB, auto-compile source files
- [Java](references/java.md) — java-debug-adapter, JDWP attach

## Debugging servers and services

For HTTP servers (Flask, Express, Go net/http, etc.), `launch` starts the server and returns immediately — it does NOT block waiting for a breakpoint. Use this workflow:

1. **Launch the server under the debugger with breakpoints set**
2. **Send HTTP requests via Bash** (curl, wget, etc.) to trigger the code path
3. **Call `continue`** — it will catch the breakpoint hit and return the viewport

```bash
krometrail debug launch "python app.py" --break pricing.py:45
# → Session started, status: running (server is listening)

# Send a request to trigger the breakpoint:
curl -X POST http://localhost:5001/price -H 'Content-Type: application/json' -d '{"item": "ABC", "qty": 5}'

krometrail debug continue
# → Viewport shows source, locals, and stack at line 45

krometrail debug vars
# → Inspect the request data and computed values

krometrail debug stop
```

### Tips for multi-service architectures

- Debug **one service at a time** — launch it under the debugger while running the others normally
- Write a small script that calls the function directly, or sends HTTP requests to trigger the code path
- Use `eval` to test corrected expressions before editing code

## Debugging strategy

1. **Start with a hypothesis.** Read the failing test and code. Form a theory about what's wrong.
2. **Set a breakpoint at the decision point.** Where does the code choose the path that leads to the wrong result?
3. **Inspect locals.** Look for values that don't match expectations.
4. **Trace upstream.** If a variable has the wrong value, where did it come from? Set a breakpoint there and re-launch.
5. **Use `eval` to test fixes.** Evaluate corrected expressions before modifying code.
6. **Stop the session and apply the fix.**

### Tips

- Prefer conditional breakpoints over stepping through loops: `--break "order.py:42 when i == 99"`
- Use `watch` to track key expressions across multiple stops
- Use `log` to review what you've already checked
- Each action returns a viewport — source context, locals, stack, and watches — in one view
- Sessions auto-expire after 5 minutes of inactivity
- **Always call `stop` when finished** to clean up debugger processes

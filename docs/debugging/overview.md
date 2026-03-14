---
title: Runtime Debugging Overview
description: What DAP-based debugging means, the viewport abstraction, and why it matters for LLM context windows.
---

# Runtime Debugging Overview

## The Problem

AI coding agents debug software through static code analysis and trial-and-error test execution. This works for surface-level bugs but fails for problems that are only visible at runtime — incorrect runtime values, unexpected mutations, race conditions, off-by-one errors deep in call chains.

Human developers reach for debuggers in exactly these situations. Agents currently cannot.

## How Krometrail Bridges DAP

Krometrail translates MCP tool calls into [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) (DAP) messages, enabling any DAP-compatible debugger to be used by any MCP-compatible agent. The agent calls `debug_launch`, `debug_step`, and `debug_evaluate` — the server handles DAP protocol details, adapter lifecycle, and state formatting.

Ten language adapters are included: Python (debugpy), Node.js (js-debug), Go (Delve), Rust (GDB/LLDB), Java (JDWP), C/C++ (GDB/lldb-dap), Ruby (rdbg), C# (netcoredbg), Swift (lldb), and Kotlin (JDWP).

## The Viewport Abstraction

The central design decision in Krometrail is the **viewport**: every time the debugee stops, the server constructs a compact, structured snapshot optimized for agent consumption.

```
── STOPPED at order.py:147 (process_order) ──
Reason: breakpoint

Call Stack (3 of 8 frames):
  → order.py:147     process_order(cart=<Cart>, user=<User:482>)
    router.py:83     handle_request(req=<Request:POST /order>)
    middleware.py:42  auth_wrapper(fn=<function>)

Source (140–154):
  143│   discount = calculate_discount(user, subtotal)
  144│   tax = subtotal * tax_rate
  146│   total = subtotal - discount + tax
 →147│   charge_result = payment.charge(user.card, total)

Locals:
  subtotal  = 149.97
  discount  = -149.97
  total     = 314.937
  user      = <User: id=482, tier="gold">
```

**Token budget: ~300–400 tokens per stop.** Sustainable over dozens of steps.

This is returned automatically after every execution control operation (`debug_continue`, `debug_step`, `debug_run_to`). The agent does not need to make separate calls to get source context, stack frames, or local variables.

## Drill-Down on Demand

The viewport provides a shallow overview. When something looks suspicious, the agent expands selectively:

- `debug_evaluate` — evaluate an arbitrary expression (`"tier_multipliers"`, `"cart.items[0].__dict__"`)
- `debug_variables` — get all variables in a specific scope with optional regex filter
- `debug_stack_trace` — full call stack with optional source context per frame

This pattern — compact default + selective expansion — keeps token usage low while allowing deep inspection when needed.

## Context Compression

Over a long session, accumulated viewports can consume significant context. Three mechanisms help:

**Investigation log** — the server maintains a running summary of actions and observations. Call `debug_action_log` to retrieve a compressed history. Drop earlier raw viewports from context, keep the log.

**Viewport diffing** — consecutive stops in the same function show only what changed, not the full snapshot:
```
── STEP at order.py:148 (same frame) ──
Changed:
  charge_result = <ChargeResult: success=False, error="card_declined">
  (5 locals unchanged)
```

**Progressive compression** — as action count increases, the viewport automatically reduces detail: fewer stack frames, shorter string previews.

## Next Steps

- [Breakpoints & Stepping](./breakpoints-stepping) — set breakpoints, step through code
- [Variables & Evaluation](./variables-evaluation) — inspect variables, evaluate expressions
- [Watch Expressions](./watch-expressions) — track values across every stop
- [Context Compression](./context-compression) — manage token budget in long sessions
- [Multi-threaded Debugging](./multi-threaded) — thread/goroutine selection

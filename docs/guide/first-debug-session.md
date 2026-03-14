---
title: First Debug Session
description: Step-by-step walkthrough of a debug session — set a breakpoint, inspect variables, find the bug.
---

# First Debug Session

This walkthrough shows how to diagnose a discount calculation bug using both MCP tools and CLI commands. The program charges customers the full order total as the "discount" — something only visible at runtime.

## The Bug

A Python order processing function calculates discounts incorrectly for gold-tier customers. The test `test_gold_discount` is failing but the traceback doesn't reveal why.

## Step 1: Launch with a Breakpoint

::: code-group

```bash [CLI]
krometrail launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
	--break order.py:147
```

```json [MCP]
// debug_launch
{
	"command": "python -m pytest tests/test_order.py::test_gold_discount -x",
	"breakpoints": [{ "file": "order.py", "line": 147 }]
}
```

:::

The session starts and waits for the breakpoint to be hit.

## Step 2: Inspect the Viewport

When the breakpoint fires, you get a compact snapshot (~400 tokens):

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
  145│
  146│   total = subtotal - discount + tax
 →147│   charge_result = payment.charge(user.card, total)

Locals:
  subtotal  = 149.97
  discount  = -149.97
  total     = 314.937
  user      = <User: id=482, tier="gold">
```

`discount = -149.97` is wrong — it should be a positive 10% discount (~15.00). The bad value came from `calculate_discount`.

## Step 3: Trace Into the Function

Re-launch with an earlier breakpoint to step into `calculate_discount`:

::: code-group

```bash [CLI]
krometrail stop
krometrail launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
	--break order.py:143
krometrail step into
```

```json [MCP]
// debug_stop, then debug_launch with line 143, then debug_step
{
	"session_id": "...",
	"direction": "into"
}
```

:::

```
── STOPPED at discount.py:15 (calculate_discount) ──
Locals:
  user      = <User: tier="gold">
  subtotal  = 149.97
```

## Step 4: Set a Conditional Breakpoint

Skip to the line that computes the discount, but only for gold-tier customers:

::: code-group

```bash [CLI]
krometrail break "discount.py:23 when tier == 'gold'"
krometrail continue
```

```json [MCP]
// debug_set_breakpoints
{
	"session_id": "...",
	"file": "discount.py",
	"breakpoints": [{ "line": 23, "condition": "tier == 'gold'" }]
}
```

:::

```
── STOPPED at discount.py:23 ──
Locals:
  tier              = "gold"
  base_rate         = 1.0
  discount_amount   = 149.97
```

`base_rate = 1.0` — that's 100%, not 10%. The tier multiplier table has the wrong value.

## Step 5: Confirm the Root Cause

::: code-group

```bash [CLI]
krometrail eval "tier_multipliers"
```

```json [MCP]
// debug_evaluate
{
	"session_id": "...",
	"expression": "tier_multipliers"
}
```

:::

```json
{"bronze": 0.05, "silver": 0.1, "gold": 1.0, "platinum": 0.2}
```

`gold` should be `0.1` (matching `silver`), not `1.0`. Fix `discount.py` line 18.

## Step 6: Clean Up

```bash
krometrail stop
```

Total debug actions: 6–8. Total tokens for viewports: ~2,400. Time to root cause: seconds.

## Key Patterns

- **Set breakpoints before function calls** — catching bad state at the call site before stepping in is more efficient than trying to trace afterward
- **Use conditional breakpoints for loops** — `when tier == 'gold'` skips irrelevant iterations
- **Evaluate expressions to test hypotheses** — `krometrail eval "tier_multipliers"` confirms the bug without modifying code
- **Watch expressions persist across steps** — add `krometrail watch "discount / subtotal"` to track a ratio automatically

See [Breakpoints & Stepping](../debugging/breakpoints-stepping) and [Variables & Evaluation](../debugging/variables-evaluation) for the full tool reference.

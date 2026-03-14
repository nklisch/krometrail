---
title: Viewport Format
description: Specification of the viewport format — sections, token budget, truncation rules, and diff mode.
---

# Viewport Format

The viewport is a compact, structured text snapshot returned on every debug stop. It is the primary output format for both MCP tools and CLI commands.

## Full Viewport Example

```
── STOPPED at app/services/order.py:147 (process_order) ──
Reason: breakpoint

Call Stack (3 of 8 frames):
  → order.py:147     process_order(cart=<Cart>, user=<User:482>)
    router.py:83     handle_request(req=<Request:POST /order>)
    middleware.py:42  auth_wrapper(fn=<function>)

Source (140–154):
  140│   for item in cart.items:
  141│       subtotal += item.price * item.quantity
  142│
  143│   discount = calculate_discount(user, subtotal)
  144│   tax = subtotal * tax_rate
  145│
  146│   total = subtotal - discount + tax
 →147│   charge_result = payment.charge(user.card, total)
  148│
  149│   if charge_result.success:

Locals:
  subtotal  = 149.97
  discount  = -149.97
  tax       = 14.997
  total     = 314.937
  cart      = <Cart: 3 items>
  user      = <User: id=482, tier="gold">
  tax_rate  = 0.1

Watch:
  len(cart.items)    = 3
  user.tier          = "gold"
  total > 0          = True
```

**Token budget: ~300–400 tokens** in typical programs.

## Sections

### Header

```
── STOPPED at <file>:<line> (<function>) ──
Reason: <stop_reason>
```

Stop reasons: `breakpoint`, `step`, `exception`, `entry`, `pause`.

### Call Stack

Shows the top N frames (controlled by `stack_depth`, default 5). Arrow (`→`) marks the current frame.

```
Call Stack (N of M frames):
  → <file>:<line>  <function>(<args>)
    <file>:<line>  <function>(<args>)
```

Arguments are rendered using the same value rendering rules as locals, truncated at `string_truncate_length`. When the total frame count exceeds `stack_depth`, the count is shown: `(3 of 8 frames)`.

### Source Context

Shows `source_context_lines` lines (default 15) centered on the current line. Arrow (`→`) marks the current line.

```
Source (<start>–<end>):
  <line>│ <code>
 →<line>│ <code>
  <line>│ <code>
```

### Locals

Variables from the current stack frame, up to `locals_max_items` (default 20). Values are rendered at depth `locals_max_depth` (default 1).

```
Locals:
  <name>  = <value>
```

### Watch (when expressions are set)

Auto-evaluated watch expressions, shown after Locals.

```
Watch:
  <expression>  = <value>
```

## Value Rendering

| Type | Rendering |
|------|-----------|
| Numbers, booleans | As-is: `149.97`, `true`, `None` |
| Strings | Quoted, truncated: `"hello, world"` |
| Long strings | Truncated with ellipsis: `"long string..."` |
| Collections | Type + length + preview: `[1, 2, 3, ... (47 items)]` |
| Objects (depth 0) | Type + key fields: `<User: id=482, tier="gold">` |
| Objects (depth > 0) | Expanded: `{id: 482, tier: "gold", ...}` |
| Expandable values | Hint: `<Cart: 3 items> [use debug_evaluate to expand]` |

## Diff Mode

When `diff_mode` is enabled and consecutive stops are in the same frame, the viewport shows only changes:

```
── STEP at order.py:148 (same frame) ──
Changed:
  charge_result = <ChargeResult: success=False, error="card_declined">
  (5 locals unchanged)
```

## Configuration Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `source_context_lines` | 15 | Lines of source around current line |
| `stack_depth` | 5 | Max call stack frames |
| `locals_max_depth` | 1 | Object nesting depth |
| `locals_max_items` | 20 | Max variables shown |
| `string_truncate_length` | 120 | Max string length |
| `collection_preview_items` | 5 | Items previewed in arrays |
| `diff_mode` | false | Show only changes in consecutive stops |

Set via `viewport_config` in `debug_launch` or `debug_status`.

## JSON Mode

With `--json` (CLI) or when using MCP tools that support `format: "json"`, the same information is returned as structured JSON:

```json
{
	"status": "stopped",
	"reason": "breakpoint",
	"location": { "file": "order.py", "line": 147, "function": "process_order" },
	"stack": [
		{ "file": "order.py", "line": 147, "function": "process_order" }
	],
	"locals": {
		"discount": { "type": "float", "value": "-149.97" }
	},
	"source": { "file": "order.py", "start_line": 140, "current_line": 147, "lines": ["..."] }
}
```

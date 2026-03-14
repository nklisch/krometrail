---
title: Watch Expressions
description: Add persistent watch expressions that auto-evaluate on every stop.
---

# Watch Expressions

Watch expressions are evaluated automatically at every debug stop and displayed in the viewport below the locals section. Use them to track values of interest across multiple steps without repeating `debug_evaluate` calls.

## Adding Watch Expressions

::: code-group

```bash [CLI]
# Add one expression
krometrail watch "len(cart.items)"

# Add multiple at once
krometrail watch "len(cart.items)" "user.tier" "total > 0"

# Add a computed expression
krometrail watch "discount / subtotal"
```

```json [MCP: debug_watch]
{
	"session_id": "...",
	"expressions": ["len(cart.items)", "user.tier", "total > 0"]
}
```

:::

## Watch Expressions in the Viewport

Once set, watched expressions appear in every viewport snapshot:

```
Locals:
  subtotal  = 149.97
  discount  = -149.97
  total     = 314.937

Watch:
  len(cart.items)    = 3
  user.tier          = "gold"
  total > 0          = True
  discount / subtotal = -1.0
```

## Managing Watches

```bash
# View current watches (they appear in every viewport)
krometrail watch

# Remove specific watches
krometrail watch --remove "user.tier"

# Clear all watches
krometrail watch --clear
```

```json
// MCP: debug_watch — remove
{
	"session_id": "...",
	"expressions": [],
	"remove": ["user.tier"]
}
```

## Persistence

Watch expressions persist for the lifetime of the session. They survive across:

- `debug_step` (any direction)
- `debug_continue`
- `debug_run_to`
- Breakpoint hits

They are cleared when the session ends (`debug_stop`).

## Use Cases

**Tracking a suspicious value across multiple steps:**
```bash
krometrail watch "discount" "base_rate" "tier_multipliers.get(tier, 'missing')"
# Now every step shows these three values without extra calls
```

**Boolean sentinels:**
```bash
krometrail watch "discount < 0" "len(results) == 0" "user.is_active"
# Quick sanity checks at each stop
```

**Cross-frame tracking:**

Watch expressions evaluate in frame 0 (the current frame). For values from outer frames, use `debug_evaluate` with `frame_index` instead — watches always run in the innermost frame.

## Efficiency

Watch expressions are evaluated server-side on each stop. They count against the session's `max_evaluate_time_ms` limit per expression. Prefer simple expressions; avoid deeply nested traversals in watch expressions (use `debug_evaluate` for those when needed).

---
title: Variables & Evaluation
description: Inspect variables by scope, filter with regex, and evaluate expressions at the current frame.
---

# Variables & Evaluation

The viewport shows a shallow snapshot of local variables at every stop. Use `debug_variables` and `debug_evaluate` to go deeper when you need it.

## Viewing Variables

::: code-group

```bash [CLI]
# Locals (default)
krometrail vars

# All scopes
krometrail vars --scope all

# Global scope
krometrail vars --scope global

# Closure variables
krometrail vars --scope closure

# Filter by name prefix
krometrail vars --filter "^user"

# Different stack frame
krometrail vars --frame 2
```

```json [MCP: debug_variables]
// Locals
{ "session_id": "...", "scope": "local" }

// All scopes
{ "session_id": "...", "scope": "all" }

// With regex filter
{ "session_id": "...", "scope": "local", "filter": "^user" }

// Expand objects deeper
{ "session_id": "...", "scope": "local", "max_depth": 3 }

// From a different frame
{ "session_id": "...", "scope": "local", "frame_index": 2 }
```

:::

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `scope` | string | `"local"` | `"local"`, `"global"`, `"closure"`, or `"all"` |
| `frame_index` | integer | `0` | Stack frame (0 = current, 1 = caller, etc.) |
| `filter` | string | | Regex filter on variable names |
| `max_depth` | integer | `1` | Object expansion depth |

## Evaluating Expressions

`debug_evaluate` is the primary drill-down tool. Use it to inspect nested objects, call methods, compute derived values, or test hypotheses — without modifying the code.

::: code-group

```bash [CLI]
# Simple expression
krometrail eval "discount"

# Method call
krometrail eval "cart.total()"

# Dict/object access
krometrail eval "tier_multipliers['gold']"

# Deep object expansion
krometrail eval "user.__dict__" --depth 3

# In a different frame
krometrail eval "request.headers" --frame 2
```

```json [MCP: debug_evaluate]
// Simple
{ "session_id": "...", "expression": "discount" }

// Deep expansion
{ "session_id": "...", "expression": "cart.__dict__", "max_depth": 3 }

// Caller's frame
{ "session_id": "...", "expression": "request.headers", "frame_index": 2 }
```

:::

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `expression` | string | | Expression to evaluate |
| `frame_index` | integer | `0` | Stack frame context |
| `max_depth` | integer | `2` | Object expansion depth |

## Value Rendering

Variable values are rendered consistently across all languages:

| Type | Rendering |
|------|-----------|
| Primitives | Displayed as-is: `149.97`, `true`, `None` |
| Strings | Quoted, truncated at 120 chars: `"hello..."` |
| Collections | Type + length + preview: `[1, 2, 3, ... (47 items)]` |
| Objects | Type + key fields: `<User: id=482, tier="gold">` |
| Expandable | Truncated values show a hint that `debug_evaluate` can expand them |

## Workflow: Hypothesis Testing

The most effective pattern is to form a hypothesis about a bad value, then use `debug_evaluate` to confirm or refute it without modifying code:

```bash
# Hypothesis: tier_multipliers has wrong value for gold
krometrail eval "tier_multipliers"
# → {"bronze": 0.05, "silver": 0.1, "gold": 1.0, "platinum": 0.2}
# Confirmed: gold should be 0.1, not 1.0

# Hypothesis: the calculation is done correctly but the result has wrong sign
krometrail eval "calculate_discount(user, 100.0)"
# → -100.0 (confirms the sign is inverted in the multiplier application)
```

Evaluate expressions to test hypotheses at the current stop before deciding whether to step further.

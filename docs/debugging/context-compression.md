---
title: Context Compression
description: How Krometrail manages token budget across long debug sessions — auto-summarization, viewport diffing, and progressive compression.
---

# Context Compression

A long debug session accumulates viewport snapshots that can consume significant LLM context. Krometrail provides three mechanisms to keep context usage sustainable.

## Automatic Investigation Log

The server maintains a running log of every action taken and the key observation at each stop. After every 10 actions, a compressed summary is appended.

Retrieve it at any time:

::: code-group

```bash [CLI]
krometrail log
krometrail log --detailed
```

```json [MCP: debug_action_log]
{ "session_id": "...", "format": "summary" }
{ "session_id": "...", "format": "detailed" }
```

:::

Example output:

```
Session Log (12 actions, 45s elapsed):
 1. Launched: python tests/test_order.py::test_gold_discount
 2. BP hit: order.py:147 — locals show discount=-149.97 (unexpected negative)
 3. Hypothesis: calculate_discount returns wrong sign for gold tier
 4. Stepped into calculate_discount (discount.py:23)
 5. Evaluated: base_rate=1.0 — should be 0.1 for 10% discount
 6. ROOT CAUSE: discount.py:18 uses tier_multipliers["gold"] = 1.0
    instead of 0.1. Multiplier applied as subtotal * rate yielding
    full subtotal as "discount" (sign inverted on return).
```

**Usage pattern:** After accumulating many viewport snapshots, retrieve the log and drop earlier raw viewports from context. The log preserves the reasoning chain at a fraction of the token cost.

## Viewport Diffing

When consecutive stops are in the same function, the viewport can show only what changed:

```
── STEP at order.py:148 (same frame) ──
Changed:
  charge_result = <ChargeResult: success=False, error="card_declined">
  (5 locals unchanged)
```

This is controlled by the `diff_mode` session parameter. Enable it per session:

```json
// debug_launch with viewport_config
{
	"command": "python app.py",
	"viewport_config": { "diff_mode": true }
}
```

## Progressive Compression

As the action count increases, the viewport automatically reduces detail:

| Action count | Change |
|---|---|
| 1–20 | Full detail (defaults) |
| 21–50 | Stack depth reduced, string previews shortened |
| 51+ | More aggressive object summarization |

The agent can override this at any stop by using `debug_variables` or `debug_evaluate` with explicit `max_depth` to get full detail on demand.

## Viewport Configuration

Configure viewport parameters at launch or per-stop:

```json
{
	"command": "python app.py",
	"viewport_config": {
		"source_context_lines": 10,
		"stack_depth": 3,
		"locals_max_depth": 1,
		"locals_max_items": 15,
		"string_truncate_length": 80,
		"collection_preview_items": 3
	}
}
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `source_context_lines` | 15 | Lines of source around current line |
| `stack_depth` | 5 | Max call stack frames |
| `locals_max_depth` | 1 | Object nesting depth |
| `locals_max_items` | 20 | Max variables shown before truncation |
| `string_truncate_length` | 120 | Max string length |
| `collection_preview_items` | 5 | Items previewed in arrays/lists |

Smaller values mean fewer tokens per stop. The defaults are tuned for ~300–400 tokens in typical programs.

## Strategy

For long debug sessions:

1. Set watch expressions for key values you want tracked — they appear automatically without extra calls
2. Use conditional breakpoints to skip to the relevant state — fewer stops means fewer viewports
3. Call `debug_action_log` periodically and drop earlier viewports from context
4. Enable `diff_mode` if you're stepping through a long code path in a single function

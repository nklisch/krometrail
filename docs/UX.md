# Agent Lens — Agent UX

This document describes the agent-facing experience: the viewport abstraction that makes debugging token-efficient, the interaction patterns agents should use, and the gap analysis motivating these design choices.

---

## Why Agent Ergonomics Matter

Existing MCP-DAP bridges solve the plumbing problem. None address the agent ergonomics problem:

| Gap | Impact | Agent Lens Approach |
|-----|--------|----------------------|
| No viewport abstraction | Agents receive raw DAP state, consuming excessive tokens and requiring manual parsing | Compact, configurable viewport snapshot (~400 tokens) returned on every stop |
| No context compression | Long debug sessions blow up the agent's context window | Automatic investigation logging, viewport diffing, session summarization |
| No drill-down protocol | Agents either get too little or too much state | Default shallow view + explicit expand tools for selective depth |
| No token budget awareness | Sessions have no feedback loop with agent resource constraints | Configurable limits, progressive compression, diff mode |
| No investigation memory | Each stop is stateless; the agent must re-derive its reasoning chain | Session log preserving hypotheses, observations, and action history |

---

## The Viewport Abstraction

The viewport is the central design innovation. Rather than exposing raw DAP state (which can be enormous), every debug stop produces a compact, structured snapshot optimized for agent consumption.

> **Prior art note:** The closest analog is mcp-dap-server's `getFullContext`, which dumps current location, stack trace, and all variables as markdown. However, it includes *all* scopes and variables with no truncation — a moderately complex program can produce thousands of tokens per stop. mcp-debugger requires 3–4 separate tool calls (stack trace, scopes, variables, source context) to assemble the same information. Agent Lens combines both insights: automatic full-context return (like mcp-dap-server) with token-budgeted rendering (unique to this project). See [PRIOR_ART.md](PRIOR_ART.md).

### Default Viewport

Every time the debugee stops (breakpoint, step completion, exception), the server automatically constructs and returns this snapshot:

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
  150│       order.status = "confirmed"
  151│   else:
  152│       order.status = "failed"
  153│
  154│   db.session.commit()

Locals:
  subtotal  = 149.97
  discount  = -149.97
  tax       = 14.997
  total     = 314.937
  cart      = <Cart: 3 items>
  user      = <User: id=482, tier="gold">
  tax_rate  = 0.1
```

**Token budget: ~300–400 tokens.** Sustainable over dozens of steps.

### Viewport Configuration

The viewport parameters are tunable per session or per stop:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `source_context_lines` | integer | 15 | Lines of source shown around current line (±7) |
| `stack_depth` | integer | 5 | Maximum call stack frames to include |
| `locals_max_depth` | integer | 1 | Object nesting depth for local variables |
| `locals_max_items` | integer | 20 | Maximum variables/fields shown before truncation |
| `string_truncate_length` | integer | 120 | Maximum characters for string values |
| `collection_preview_items` | integer | 5 | Items shown for arrays/lists/maps |

These defaults keep the viewport under 400 tokens in typical cases while providing enough information for the agent to form hypotheses without drilling down on every stop.

### Value Rendering

Variable values are rendered following consistent rules across all languages:

- **Primitives:** Displayed as-is. Numbers, booleans, null/nil/None.
- **Strings:** Quoted, truncated to `string_truncate_length` with ellipsis.
- **Collections:** Type and length shown, first N items previewed. E.g., `[1, 2, 3, ... (47 items)]`
- **Objects:** Type name and key fields at depth 1. E.g., `<User: id=482, tier="gold">`
- **Expandable marker:** Truncated values include a marker indicating the agent can call `debug_evaluate` to see the full value.

### Watch Expressions in Viewport

When watch expressions are active, they appear after locals:

```
Watch:
  len(cart.items)    = 3
  user.tier          = "gold"
  total > 0          = True
  discount / subtotal = -1.0
```

---

## Agent Interaction Patterns

The tool interface supports several debugging strategies. These patterns can be included in tool descriptions to help agents reason about when to use each approach.

> **Prior art note:** These patterns are documented here as agent guidance. mcp-debugger demonstrates the value of embedding guidance in tool descriptions — their `set_breakpoint` tool warns about non-executable lines, which helps agents avoid common mistakes. Our tool descriptions should similarly encode these strategic patterns. See [PRIOR_ART.md](PRIOR_ART.md).

### Hypothesis-Driven Debugging

The most common pattern. The agent has a hypothesis about the bug and uses the debugger to confirm or refute it:

1. Set a breakpoint at the suspected location.
2. Continue to the breakpoint and inspect locals.
3. Evaluate expressions to test the hypothesis.
4. If confirmed, stop debugging and fix the code. If refuted, set new breakpoints and continue.

### Bisection Debugging

For bugs where the agent knows a value is wrong at point B but correct at point A:

1. Set breakpoints at A and B, confirm value correctness.
2. Set a breakpoint at the midpoint, check the value.
3. Repeat, halving the search space each iteration.

This is a pattern where agents can be *more systematic* than most human debuggers.

### Exception Tracing

When a traceback doesn't reveal root cause:

1. Set an exception breakpoint to catch the exception before unwinding.
2. Inspect local state at the throw site.
3. Walk up the stack to understand how the bad state was constructed.

### Watchpoint Convergence

For state mutation bugs:

1. Set a watch on the variable of interest.
2. Use conditional breakpoints to catch the specific mutation (e.g., `discount < 0`).
3. Trace the mutation backward through the call chain.

### Trace Mapping

For understanding unfamiliar code paths:

1. Set breakpoints at every entry point to a module (`debug_set_breakpoints` with all function entries).
2. Run the program and observe the call sequence.
3. Use the session log to reconstruct the execution flow.

This is tedious for humans but trivial for an agent.

---

## Agent Skill File

For agents that use the CLI path, this skill file teaches the agent how to use Agent Lens. It can be loaded as a Claude Code skill, a Codex system prompt addition, or any agent's instruction set:

```markdown
# Agent Lens — Debugging Skill

You have access to `agent-lens`, a CLI debugger. Use it when you need to
inspect runtime state to diagnose a bug — especially when static code
reading and test output aren't enough to identify the root cause.

## Quick start
  agent-lens launch "<command>" --break <file>:<line>
  agent-lens continue          # run to next breakpoint
  agent-lens step into|over|out
  agent-lens eval "<expr>"     # evaluate expression at current stop
  agent-lens vars              # show local variables
  agent-lens stop              # end session

## Conditional breakpoints
  agent-lens break "<file>:<line> when <condition>"

## Strategy
1. Start by setting a breakpoint where you expect the bug to manifest.
2. Inspect locals. Look for unexpected values.
3. If the bad value came from a function call, set a breakpoint inside
   that function and re-launch.
4. Use `agent-lens eval` to test hypotheses without modifying code.
5. Once you identify the root cause, stop the session and fix the code.

## Key rules
- Always call `agent-lens stop` when done to clean up.
- Prefer conditional breakpoints over stepping through loops.
- Each command prints a viewport showing source, locals, and stack.
- If a session times out (5 min default), re-launch.
```

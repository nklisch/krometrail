# Agent Lens — Interface Reference

Agent Lens exposes two equivalent interface surfaces: MCP tools (for agents with native MCP support) and CLI commands (for agents with bash access). Both share the same core and produce identical viewport output.

---

## MCP Tool Interface

Every tool that operates on a running debug session accepts a `session_id` parameter (returned by `debug_launch`) to support multiple concurrent sessions.

> **Prior art note:** Tool interface design varies significantly across projects. mcp-debugger exposes 19 tools, many requiring DAP-internal knowledge (`variablesReference`, `frameId`). mcp-dap-server uses 13 tools with a standout pattern: dynamic tool registration removes `debug` and adds session tools after launch, and capability-gated tools only appear when the debugger supports them. dap-mcp uses 12 tools with XML output. debugger-mcp uses just 7 tools (the "SBCTED" mnemonic). Agent Lens uses 15 tools — more complete than debugger-mcp but hiding DAP internals behind the viewport abstraction unlike mcp-debugger. See [PRIOR_ART.md](PRIOR_ART.md).

### Session Lifecycle

#### `debug_launch`

Launch a debug target process and return a session handle. The server selects the appropriate language adapter based on file extension or the explicit language parameter. Initial breakpoints can be set before execution begins.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `command` | string | yes | Command to execute (e.g., `"python app.py"`, `"node index.js"`) |
| `language` | string | | Override language detection. One of: `python`, `javascript`, `typescript`, `go`, `rust`, `java`, `cpp` |
| `breakpoints` | Breakpoint[] | | Initial breakpoints to set before execution |
| `cwd` | string | | Working directory for the debug target |
| `env` | Record\<string, string\> | | Additional environment variables |
| `viewport_config` | ViewportConfig | | Override default viewport parameters |
| `stop_on_entry` | boolean | | Pause on first executable line. Default: `false` |

**Returns:** Session object with `session_id`, initial viewport if `stop_on_entry` is true, or `"running"` status.

#### `debug_stop`

Terminate the debug session and clean up all resources.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The session to terminate |

**Returns:** Confirmation with session duration and total debug actions taken.

#### `debug_status`

Check the current state of a debug session without taking any action.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The session to query |

**Returns:** Current state: `running`, `stopped` (with viewport), `terminated`, or `error`.

---

### Execution Control

#### `debug_continue`

Resume execution until the next breakpoint, exception, or program exit.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `timeout_ms` | integer | | Max wait time for next stop. Default: `30000` |

**Returns:** Viewport snapshot at next stop, or termination status if program exits.

#### `debug_step`

Execute a single step in the specified direction.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `direction` | `"over"` \| `"into"` \| `"out"` | yes | Step granularity |
| `count` | integer | | Number of steps to take. Default: `1` |

**Returns:** Viewport snapshot after the final step completes.

> **Prior art note:** mcp-dap-server's `step` and `continue` tools automatically return `getFullContext` (stack + scopes + variables) on every stop — the agent gets full state without a separate call. Agent Lens follows this pattern: every execution control tool returns the viewport. This eliminates the 3–4 tool call round trip that mcp-debugger requires. See [PRIOR_ART.md](PRIOR_ART.md).

#### `debug_run_to`

Continue execution until a specific location is reached. Equivalent to a temporary breakpoint.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `file` | string | yes | Target file path |
| `line` | integer | yes | Target line number |
| `timeout_ms` | integer | | Max wait time. Default: `30000` |

**Returns:** Viewport snapshot at target location, or timeout/termination status.

---

### Breakpoint Management

#### `debug_set_breakpoints`

Set breakpoints in a file. **Replaces** all existing breakpoints in that file (DAP semantics). To add without removing existing ones, include them in the call.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `file` | string | yes | Source file path |
| `breakpoints` | Breakpoint[] | yes | Array of breakpoint definitions |

**Returns:** Array of confirmed breakpoints with verified line numbers (DAP may adjust lines).

#### `debug_set_exception_breakpoints`

Configure which exceptions cause the debugger to stop.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `filters` | string[] | yes | Exception filter IDs. Common: `"uncaught"`, `"raised"` (Python), `"all"` (JS) |

**Returns:** Confirmed exception breakpoint configuration.

#### `debug_list_breakpoints`

Return all active breakpoints across all files.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |

**Returns:** Map of file paths to breakpoint arrays with current hit counts.

---

### State Inspection (Drill-Down)

These tools provide selective depth. The default viewport gives a shallow overview; these let the agent go deeper when something looks suspicious.

#### `debug_evaluate`

Evaluate an arbitrary expression in the current stack frame context. This is the **primary drill-down tool** — it can inspect nested objects, call methods, compute derived values, or test hypotheses.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `expression` | string | yes | Expression to evaluate. E.g., `"cart.items[0].__dict__"` |
| `frame_index` | integer | | Stack frame context (0 = current, 1 = caller). Default: `0` |
| `max_depth` | integer | | Object expansion depth for result. Default: `2` |

**Returns:** Rendered value following viewport value rendering rules.

#### `debug_variables`

Retrieve variables for a specific scope, with more detail or broader scope than the default viewport.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `scope` | `"local"` \| `"global"` \| `"closure"` \| `"all"` | | Variable scope. Default: `"local"` |
| `frame_index` | integer | | Stack frame context. Default: `0` |
| `filter` | string | | Regex filter on variable names. E.g., `"^user"` |
| `max_depth` | integer | | Object expansion depth. Default: `1` |

**Returns:** Formatted variable listing for the requested scope.

#### `debug_stack_trace`

Retrieve the full call stack with more detail than the default viewport.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `max_frames` | integer | | Maximum frames. Default: `20` |
| `include_source` | boolean | | Include source context around each frame. Default: `false` |

**Returns:** Full stack trace with function signatures, file locations, and optional source context.

#### `debug_source`

Retrieve source code for any file in the debugee's scope, not just the current frame.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `file` | string | yes | Source file path |
| `start_line` | integer | | Start of range. Default: `1` |
| `end_line` | integer | | End of range. Default: `start_line + 40` |

**Returns:** Numbered source listing for the requested range.

---

### Session Intelligence

#### `debug_watch`

Add expressions to a watch list. Watched expressions are automatically evaluated and included in every viewport snapshot, below the locals section.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `expressions` | string[] | yes | Expressions to watch. E.g., `["len(cart.items)", "user.tier", "total > 0"]` |

**Returns:** Confirmed watch list.

#### `debug_session_log`

Retrieve the compressed investigation log for the current session. As the agent takes debug actions, the server maintains a running summary of key observations.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `format` | `"summary"` \| `"detailed"` | | Level of detail. Default: `"summary"` |

**Returns:** Session history — actions taken, observations, and hypothesis evolution.

**Example summary log:**

```
Session Log (12 actions, 45s elapsed):
 1. Launched: python tests/test_order.py::test_gold_discount
 2. BP hit: order.py:147 — locals show discount=-149.97 (unexpected negative)
 3. Hypothesis: calculate_discount returns wrong sign for gold tier
 4. Stepped into calculate_discount (discount.py:23)
 5. Evaluated: base_rate=1.0 — should be 0.1 for 10% discount
 6. ROOT CAUSE: discount.py:18 uses `tier_multipliers["gold"] = 1.0`
    instead of 0.1. Multiplier is applied as `subtotal * rate` yielding
    full subtotal as "discount" (sign inverted on return).
```

#### `debug_output`

Retrieve captured stdout/stderr from the debugee process.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | yes | The active session |
| `stream` | `"stdout"` \| `"stderr"` \| `"both"` | | Which output stream. Default: `"both"` |
| `since_action` | integer | | Only show output since action N. Default: `0` (all) |

**Returns:** Captured output text, truncated to configured limit.

---

## CLI Interface

### Why Both Interfaces

**MCP path:** Install as an MCP server, the agent discovers tools automatically via MCP tool listing. Best for agents with native MCP support (Claude Code with MCP config, Cursor, etc.). Zero prompting needed — the tool descriptions guide the agent.

**CLI path:** Install via `npx agent-lens`, `bunx agent-lens`, or download the compiled single-file binary from GitHub releases (built with `bun build --compile` — zero runtime dependencies). Load a skill/instruction file that teaches the agent the commands. Best for agents that are already good at bash (Claude Code, Codex), for CI/CD integration, for human debugging, and for environments where MCP setup is inconvenient. No server lifecycle to manage — each command is stateless from the shell's perspective (sessions are managed by a lightweight background daemon).

The CLI is not a secondary interface. It's a first-class path designed so that an agent with nothing more than bash access and a one-paragraph skill description can debug as effectively as one using MCP.

> **Prior art note:** No existing MCP-DAP project offers a CLI interface. All require MCP server configuration. The CLI path is unique to Agent Lens and addresses the friction of MCP setup in environments where bash is the path of least resistance.

### Command Reference

Every command outputs the viewport to stdout as structured plain text (the same format described in [UX.md](UX.md)). Exit codes follow standard conventions: 0 for success, 1 for errors, 2 for timeouts.

#### Session Lifecycle

```bash
# Launch a debug session with initial breakpoints
agent-lens launch "python app.py" \
  --break order.py:147 \
  --break discount.py:23 \
  --stop-on-entry

# Launch with a conditional breakpoint
agent-lens launch "python -m pytest tests/test_order.py -x" \
  --break "order.py:147 when discount < 0"

# Launch with language override
agent-lens launch "cargo test" --language rust

# Check session status
agent-lens status

# Stop the active session
agent-lens stop

# Stop a specific session
agent-lens stop --session abc123
```

#### Execution Control

```bash
# Continue to next breakpoint
agent-lens continue

# Step over / into / out
agent-lens step over
agent-lens step into
agent-lens step out

# Step multiple times
agent-lens step over --count 5

# Run to a specific line
agent-lens run-to order.py:150

# Continue with a timeout
agent-lens continue --timeout 10000
```

#### Breakpoint Management

```bash
# Set breakpoints (replaces existing in that file)
agent-lens break order.py:147
agent-lens break order.py:147,150,155

# Conditional breakpoints
agent-lens break "order.py:147 when discount < 0"
agent-lens break "order.py:147 hit >=100"

# Log points (log instead of breaking)
agent-lens break "order.py:147 log 'discount={discount}, total={total}'"

# Exception breakpoints
agent-lens break --exceptions uncaught
agent-lens break --exceptions raised    # Python: all raised exceptions

# List all breakpoints
agent-lens breakpoints

# Remove breakpoints from a file
agent-lens break --clear order.py
```

#### State Inspection

```bash
# Evaluate an expression in current frame
agent-lens eval "cart.items[0].__dict__"
agent-lens eval "len(results)" --depth 3

# Evaluate in a different stack frame
agent-lens eval "request.headers" --frame 2

# Show variables (current frame locals by default)
agent-lens vars
agent-lens vars --scope global
agent-lens vars --scope closure
agent-lens vars --filter "^user"

# Full stack trace
agent-lens stack
agent-lens stack --frames 20 --source

# View source for any file
agent-lens source discount.py
agent-lens source discount.py:15-30
```

#### Session Intelligence

```bash
# Add watch expressions
agent-lens watch "len(cart.items)" "user.tier" "total > 0"

# View the session investigation log
agent-lens log
agent-lens log --detailed

# View captured program output
agent-lens output
agent-lens output --stderr
agent-lens output --since-action 5
```

#### Utility

```bash
# Check which adapters/debuggers are available
agent-lens doctor

# Show version and config
agent-lens --version

# JSON output mode (for programmatic consumption)
agent-lens launch "python app.py" --break order.py:147 --json

# Quiet mode (viewport only, no chrome)
agent-lens continue --quiet
```

### Session Daemon

The CLI manages sessions via a lightweight background daemon that starts automatically on the first `agent-lens launch` and shuts down after the last session ends (or after an idle timeout). This allows sequential commands to operate on a persistent debug session without the user managing server lifecycle:

```bash
# These are separate shell commands that share a session:
agent-lens launch "python app.py" --break order.py:147
# daemon starts, session created, viewport printed

agent-lens continue
# daemon already running, continues the existing session

agent-lens eval "discount"
# evaluates in the stopped session

agent-lens stop
# session ends, daemon idles then shuts down
```

The daemon listens on a Unix domain socket at `$XDG_RUNTIME_DIR/agent-lens.sock` (or `~/.agent-lens/agent-lens.sock` as fallback). Multiple concurrent sessions are supported — when more than one session is active, commands require `--session <id>` to disambiguate.

### Output Formats

**`--json` flag:** Every command can output structured JSON instead. Useful for agents that prefer to parse structured data, or for piping into other tools:

```json
{
  "status": "stopped",
  "reason": "breakpoint",
  "location": {
    "file": "order.py",
    "line": 147,
    "function": "process_order"
  },
  "stack": [
    {"file": "order.py", "line": 147, "function": "process_order"},
    {"file": "router.py", "line": 83, "function": "handle_request"}
  ],
  "locals": {
    "subtotal": {"type": "float", "value": "149.97"},
    "discount": {"type": "float", "value": "-149.97"},
    "total": {"type": "float", "value": "314.937"}
  },
  "source": {
    "file": "order.py",
    "start_line": 140,
    "end_line": 154,
    "current_line": 147,
    "lines": ["..."]
  }
}
```

**`--quiet` flag:** Suppresses everything except the viewport itself — no banners, session IDs, or hints. Useful when piping output to an agent that just needs the state.

---

## Example: MCP Debugging Session

A complete example showing how an agent uses the MCP tools to diagnose a discount bug:

```
Agent: The test test_gold_discount is failing. Let me debug it.

→ debug_launch(
    command: "python -m pytest tests/test_order.py::test_gold_discount -x",
    breakpoints: [{ file: "order.py", line: 147 }],
    stop_on_entry: false
  )

← Session created. Waiting for breakpoint...
← STOPPED at order.py:147 (process_order)
   Locals: subtotal=149.97, discount=-149.97, total=314.937 ...

Agent: discount is -149.97, that's wrong. Let me check calculate_discount.

→ debug_step(session_id: "...", direction: "into")
  [Oops, already past that call. Let me restart with an earlier breakpoint.]

→ debug_stop(session_id: "...")
→ debug_launch(
    command: "python -m pytest tests/test_order.py::test_gold_discount -x",
    breakpoints: [{ file: "order.py", line: 143 }]
  )

← STOPPED at order.py:143 — about to call calculate_discount

→ debug_step(session_id: "...", direction: "into")

← STOPPED at discount.py:15 (calculate_discount)
   Locals: user=<User: tier="gold">, subtotal=149.97

→ debug_set_breakpoints(
    file: "discount.py",
    breakpoints: [{ line: 23, condition: "tier == 'gold'" }]
  )

→ debug_continue(session_id: "...")

← STOPPED at discount.py:23
   Locals: tier="gold", base_rate=1.0, discount_amount=149.97

Agent: Found it. base_rate is 1.0 (100%) instead of 0.1 (10%).
       The tier_multipliers dict has the wrong value.

→ debug_evaluate(expression: "tier_multipliers")

← {"bronze": 0.05, "silver": 0.1, "gold": 1.0, "platinum": 0.2}

Agent: Confirmed. gold should be 0.1 like silver, not 1.0.

→ debug_stop(session_id: "...")
```

Total debug actions: 8. Total tokens for viewports: ~2,400. Time to root cause: seconds.

---

## Example: CLI Debugging Session

The same discount bug diagnosed via the CLI:

```bash
$ agent-lens launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
    --break order.py:147

Session started: abc123
Waiting for breakpoint...

── STOPPED at order.py:147 (process_order) ──
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

Locals:
  subtotal  = 149.97
  discount  = -149.97
  tax       = 14.997
  total     = 314.937
  cart      = <Cart: 3 items>
  user      = <User: id=482, tier="gold">
  tax_rate  = 0.1
```

Agent sees `discount = -149.97` and wants to inspect the function that produced it:

```bash
$ agent-lens stop
$ agent-lens launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
    --break order.py:143

── STOPPED at order.py:143 (process_order) ──
...

$ agent-lens step into

── STOPPED at discount.py:15 (calculate_discount) ──
Locals:
  user      = <User: tier="gold">
  subtotal  = 149.97

$ agent-lens break "discount.py:23 when tier == 'gold'"
Breakpoint set: discount.py:23 (conditional: tier == 'gold')

$ agent-lens continue

── STOPPED at discount.py:23 (calculate_discount) ──
Reason: conditional breakpoint

Locals:
  tier              = "gold"
  base_rate         = 1.0
  discount_amount   = 149.97

$ agent-lens eval "tier_multipliers"
{"bronze": 0.05, "silver": 0.1, "gold": 1.0, "platinum": 0.2}

$ agent-lens stop
Session abc123 ended. Duration: 12s, Actions: 6
```

Root cause identified in 6 commands.

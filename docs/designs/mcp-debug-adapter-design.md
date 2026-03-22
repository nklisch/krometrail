# Krometrail — Design Document

**Runtime Debugging Viewport for AI Coding Agents**

Version 1.0 — March 2026 | Draft

---

## 1. Executive Summary

AI coding agents such as Claude Code and Codex currently debug software through static code analysis and trial-and-error test execution. They lack the ability to inspect runtime state, set breakpoints, or step through executing code. This makes entire categories of bugs—incorrect runtime values, unexpected mutations, race conditions, off-by-one errors deep in call chains—significantly harder to diagnose.

This document specifies **Krometrail**, a Model Context Protocol (MCP) server that exposes a language-agnostic debugging interface to AI agents. The server translates MCP tool calls into Debug Adapter Protocol (DAP) messages, enabling any DAP-compatible debugger to be used by any MCP-compatible agent without either side needing awareness of the other.

The design prioritizes three qualities: a **compact default viewport** that minimizes token consumption per debug step, a **drill-down-on-demand** pattern that lets the agent selectively expand its view, and a **pluggable adapter layer** that makes adding new languages a bounded, well-defined task.

### 1.1 Prior Art & Differentiation

Several projects have emerged in this space, all converging on the same MCP-over-DAP architecture:

- **AIDB** (ai-debugger-inc) — Python, JS/TS, Java. Supports `launch.json` configs, framework auto-detection (pytest, jest, django), conditional breakpoints. Most polished positioning as a "debugging standard for AI."
- **mcp-debugger** (debugmcp) — TypeScript. Clean adapter pattern, Python/JS/Rust/Go support, 1000+ tests. Expression evaluation and conditional breakpoints still in progress.
- **mcp-dap-server** (go-delve) — Go. From the Delve team. Generic DAP bridge with demos of autonomous agentic debugging.
- **debugger-mcp** (Govinda-Fichtner) — Rust/Tokio. Python, Ruby, Node.js, Go, Rust. Integration tests using real Claude Code and Codex agents.
- **dap-mcp** (Kashun Cheng) — Python. Config-driven, one of the earlier entries.

All of these solve the **plumbing** problem: bridging MCP to DAP. None of them address the **agent ergonomics** problem, which is the primary focus of this design:

| Gap | Impact | This Design's Approach |
|-----|--------|----------------------|
| No viewport abstraction | Agents receive raw DAP state, consuming excessive tokens and requiring manual parsing | Compact, configurable viewport snapshot (~400 tokens) returned on every stop |
| No context compression | Long debug sessions blow up the agent's context window | Automatic investigation logging, viewport diffing, session summarization |
| No drill-down protocol | Agents either get too little or too much state | Default shallow view + explicit expand tools for selective depth |
| No token budget awareness | Sessions have no feedback loop with agent resource constraints | Configurable limits, progressive compression, diff mode |
| No investigation memory | Each stop is stateless; the agent must re-derive its reasoning chain | Session log preserving hypotheses, observations, and action history |

This document can serve as a specification for a new project or as an agent-ergonomics layer contributed to an existing one.

---

## 2. Problem Statement

Today's coding agents operate in a fundamentally limited debugging loop:

1. Read static source code and error output.
2. Form a hypothesis about the bug.
3. Edit the code based on that hypothesis.
4. Run the test suite and observe pass/fail.
5. Repeat until tests pass or the agent gives up.

This loop works for many surface-level bugs but fails for problems where the root cause is only visible at runtime. A negative discount value, an unexpectedly null reference three frames deep, a loop that executes one too many times—these require observing actual program state during execution. Human developers reach for debuggers in exactly these situations. Agents currently cannot.

The gap is not in reasoning capability but in tooling. Agents already know how to form hypotheses, test them, and iterate. They simply lack the instruments to observe runtime behavior directly.

---

## 3. Architecture Overview

### 3.1 System Layers

The system consists of four layers, each with a single responsibility:

**Agent Layer.** The AI coding agent (Claude Code, Codex, or any MCP-compatible client). It reasons about bugs and invokes debug tools when runtime inspection would help. No modifications to the agent are required — it can connect via MCP (discovering tools automatically) or via the CLI (using bash commands with a loaded skill file). Both paths produce identical viewport output.

**MCP Transport Layer.** Standard MCP communication (stdio or SSE). The debug server registers its tools on startup and responds to tool invocations. Event notifications (breakpoint hit, exception thrown) are delivered as the return value of blocking wait calls.

**Debug Server Core.** The central orchestration layer. It manages session lifecycle, translates tool calls into DAP requests, maintains the viewport abstraction, enforces safety limits (timeouts, step budgets), and handles session compression. This layer is language-agnostic.

**Adapter Layer.** Thin, language-specific modules that implement a standard interface for launching a debug target and connecting to its DAP server. Each adapter encapsulates the setup quirks of a specific debugger (debugpy for Python, node-inspect for Node.js, delve for Go, etc.) while exposing a uniform connection surface to the core.

### 3.2 Data Flow

A typical interaction follows this sequence:

1. Agent calls `debug_launch` with a target command and optional initial breakpoints.
2. Debug Server Core selects the appropriate adapter based on file extension or explicit language parameter.
3. Adapter launches the debugee process and establishes a DAP connection.
4. Core sets initial breakpoints via DAP `setBreakpoints` and issues `configurationDone`.
5. The debugee runs until a breakpoint is hit or an exception occurs.
6. Core receives the DAP `stopped` event and constructs a **Viewport Snapshot**.
7. The snapshot is returned to the agent as structured text.
8. The agent reasons about the state and issues further debug commands.
9. Repeat until the agent has enough information, then call `debug_stop`.

```
┌─────────────────────────────────────────────────────────┐
│                     AI Coding Agent                      │
│              (Claude Code / Codex / etc.)                 │
└──────────┬──────────────────────────────┬───────────────┘
           │ MCP (stdio / SSE)            │ bash / shell
           ▼                              ▼
┌─────────────────────────────────────────────────────────┐
│                       krometrail                         │
│  ┌────────────────────┐    ┌─────────────────────────┐  │
│  │  MCP Server        │    │  CLI                     │  │
│  │  (tool interface)  │    │  krometrail launch ...   │  │
│  │                    │    │  krometrail step ...     │  │
│  └─────────┬──────────┘    └────────────┬────────────┘  │
│            └──────────┬─────────────────┘               │
│  ┌────────────────────┴──────────────────────────────┐  │
│  │              Debug Server Core                     │  │
│  │   Session Manager · Viewport Renderer              │  │
│  │   Context Compressor · Safety Limits               │  │
│  └──────────────────────┬────────────────────────────┘  │
│  ┌──────────────────────┴────────────────────────────┐  │
│  │             Adapter Registry                       │  │
│  │   ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐    │  │
│  │   │ Python │ │ Node   │ │  Go    │ │ Rust   │    │  │
│  │   │debugpy │ │inspect │ │ delve  │ │codelldb│    │  │
│  │   └───┬────┘ └───┬────┘ └───┬────┘ └───┬────┘    │  │
│  └───────┼──────────┼──────────┼──────────┼──────────┘  │
└──────────┼──────────┼──────────┼──────────┼─────────────┘
           │ DAP      │ DAP      │ DAP      │ DAP
           ▼          ▼          ▼          ▼
       Debugee     Debugee    Debugee    Debugee
```

---

## 4. The Viewport Abstraction

The viewport is the central design innovation. Rather than exposing raw DAP state (which can be enormous), every debug stop produces a compact, structured snapshot optimized for agent consumption. **This is the primary differentiator from existing MCP-DAP bridges.**

### 4.1 Default Viewport

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

### 4.2 Viewport Configuration

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

### 4.3 Value Rendering

Variable values are rendered following consistent rules across all languages:

- **Primitives:** Displayed as-is. Numbers, booleans, null/nil/None.
- **Strings:** Quoted, truncated to `string_truncate_length` with ellipsis.
- **Collections:** Type and length shown, first N items previewed. E.g., `[1, 2, 3, ... (47 items)]`
- **Objects:** Type name and key fields at depth 1. E.g., `<User: id=482, tier="gold">`
- **Expandable marker:** Truncated values include a ▶ marker indicating the agent can call `debug_evaluate` to see the full value.

### 4.4 Watch Expressions in Viewport

When watch expressions are active, they appear after locals:

```
Watch:
  len(cart.items)    = 3
  user.tier          = "gold"
  total > 0          = True
  discount / subtotal = -1.0
```

---

## 5. MCP Tool Interface

The server exposes the following tools via MCP. Every tool that operates on a running debug session accepts a `session_id` parameter (returned by `debug_launch`) to support multiple concurrent sessions.

### 5.1 Session Lifecycle

#### `debug_launch`

Launch a debug target process and return a session handle. The server selects the appropriate language adapter based on file extension or the explicit language parameter. Initial breakpoints can be set before execution begins.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `command` | string | ✓ | Command to execute (e.g., `"python app.py"`, `"node index.js"`) |
| `language` | string | | Override language detection. One of: `python`, `javascript`, `typescript`, `go`, `rust`, `java`, `cpp` |
| `breakpoints` | Breakpoint[] | | Initial breakpoints to set before execution |
| `cwd` | string | | Working directory for the debug target |
| `env` | Record\<string, string\> | | Additional environment variables |
| `viewport_config` | ViewportConfig | | Override default viewport parameters |
| `stop_on_entry` | boolean | | Pause on first executable line. Default: `false` |

**Returns:** Session object with `session_id`, initial viewport if `stop_on_entry` is true, or `"running"` status.

#### `debug_stop`

Terminate the debug session and clean up all resources.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The session to terminate |

**Returns:** Confirmation with session duration and total debug actions taken.

#### `debug_status`

Check the current state of a debug session without taking any action.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The session to query |

**Returns:** Current state: `running`, `stopped` (with viewport), `terminated`, or `error`.

---

### 5.2 Execution Control

#### `debug_continue`

Resume execution until the next breakpoint, exception, or program exit.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `timeout_ms` | integer | | Max wait time for next stop. Default: `30000` |

**Returns:** Viewport snapshot at next stop, or termination status if program exits.

#### `debug_step`

Execute a single step in the specified direction.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `direction` | `"over"` \| `"into"` \| `"out"` | ✓ | Step granularity |
| `count` | integer | | Number of steps to take. Default: `1` |

**Returns:** Viewport snapshot after the final step completes.

#### `debug_run_to`

Continue execution until a specific location is reached. Equivalent to a temporary breakpoint.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `file` | string | ✓ | Target file path |
| `line` | integer | ✓ | Target line number |
| `timeout_ms` | integer | | Max wait time. Default: `30000` |

**Returns:** Viewport snapshot at target location, or timeout/termination status.

---

### 5.3 Breakpoint Management

#### `debug_set_breakpoints`

Set breakpoints in a file. **Replaces** all existing breakpoints in that file (DAP semantics). To add without removing existing ones, include them in the call.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `file` | string | ✓ | Source file path |
| `breakpoints` | Breakpoint[] | ✓ | Array of breakpoint definitions |

**Returns:** Array of confirmed breakpoints with verified line numbers (DAP may adjust lines).

**The Breakpoint type:**

```typescript
interface Breakpoint {
  line: number;                // Required. Line number.
  condition?: string;          // Expression that must be true to trigger.
                               // E.g., "discount < 0"
  hit_condition?: string;      // Break after N hits. E.g., ">=100"
  log_message?: string;        // Log instead of breaking.
                               // Supports {expression} interpolation.
}
```

#### `debug_set_exception_breakpoints`

Configure which exceptions cause the debugger to stop.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `filters` | string[] | ✓ | Exception filter IDs. Common: `"uncaught"`, `"raised"` (Python), `"all"` (JS) |

**Returns:** Confirmed exception breakpoint configuration.

#### `debug_list_breakpoints`

Return all active breakpoints across all files.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |

**Returns:** Map of file paths to breakpoint arrays with current hit counts.

---

### 5.4 State Inspection (Drill-Down)

These tools provide selective depth. The default viewport gives a shallow overview; these let the agent go deeper when something looks suspicious.

#### `debug_evaluate`

Evaluate an arbitrary expression in the current stack frame context. This is the **primary drill-down tool**—it can inspect nested objects, call methods, compute derived values, or test hypotheses.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `expression` | string | ✓ | Expression to evaluate. E.g., `"cart.items[0].__dict__"` |
| `frame_index` | integer | | Stack frame context (0 = current, 1 = caller). Default: `0` |
| `max_depth` | integer | | Object expansion depth for result. Default: `2` |

**Returns:** Rendered value following viewport value rendering rules.

#### `debug_variables`

Retrieve variables for a specific scope, with more detail or broader scope than the default viewport.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `scope` | `"local"` \| `"global"` \| `"closure"` \| `"all"` | | Variable scope. Default: `"local"` |
| `frame_index` | integer | | Stack frame context. Default: `0` |
| `filter` | string | | Regex filter on variable names. E.g., `"^user"` |
| `max_depth` | integer | | Object expansion depth. Default: `1` |

**Returns:** Formatted variable listing for the requested scope.

#### `debug_stack_trace`

Retrieve the full call stack with more detail than the default viewport.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `max_frames` | integer | | Maximum frames. Default: `20` |
| `include_source` | boolean | | Include source context around each frame. Default: `false` |

**Returns:** Full stack trace with function signatures, file locations, and optional source context.

#### `debug_source`

Retrieve source code for any file in the debugee's scope, not just the current frame.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `file` | string | ✓ | Source file path |
| `start_line` | integer | | Start of range. Default: `1` |
| `end_line` | integer | | End of range. Default: `start_line + 40` |

**Returns:** Numbered source listing for the requested range.

---

### 5.5 Session Intelligence

#### `debug_watch`

Add expressions to a watch list. Watched expressions are automatically evaluated and included in every viewport snapshot, below the locals section.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `expressions` | string[] | ✓ | Expressions to watch. E.g., `["len(cart.items)", "user.tier", "total > 0"]` |

**Returns:** Confirmed watch list.

#### `debug_action_log`

Retrieve the compressed investigation log for the current session. As the agent takes debug actions, the server maintains a running summary of key observations.

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `session_id` | string | ✓ | The active session |
| `format` | `"summary"` \| `"detailed"` | | Level of detail. Default: `"summary"` |

**Returns:** Session history—actions taken, observations, and hypothesis evolution.

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
| `session_id` | string | ✓ | The active session |
| `stream` | `"stdout"` \| `"stderr"` \| `"both"` | | Which output stream. Default: `"both"` |
| `since_action` | integer | | Only show output since action N. Default: `0` (all) |

**Returns:** Captured output text, truncated to configured limit.

---

## 6. CLI Interface

Krometrail provides a complete CLI that mirrors every MCP tool as a shell command. This enables two integration paths: agents that support MCP connect via the MCP server, while agents that are good at bash (or humans who want a better debugging UX) use the CLI directly. Both interfaces share the same core — identical viewport rendering, session management, and adapter layer.

### 6.1 Why Both Interfaces

**MCP path:** Install as an MCP server, the agent discovers tools automatically via MCP tool listing. Best for agents with native MCP support (Claude Code with MCP config, Cursor, etc.). Zero prompting needed — the tool descriptions guide the agent.

**CLI path:** Install via `npx krometrail`, `bunx krometrail`, or download the compiled single-file binary from GitHub releases (built with `bun build --compile` — zero runtime dependencies). Load a skill/instruction file that teaches the agent the commands. Best for agents that are already good at bash (Claude Code, Codex), for CI/CD integration, for human debugging, and for environments where MCP setup is inconvenient. No server lifecycle to manage — each command is stateless from the shell's perspective (sessions are managed by a lightweight background daemon).

The CLI is not a secondary interface. It's a first-class path designed so that an agent with nothing more than bash access and a one-paragraph skill description can debug as effectively as one using MCP.

### 6.2 Command Reference

Every command outputs the viewport to stdout as structured plain text (the same format described in Section 4). Exit codes follow standard conventions: 0 for success, 1 for errors, 2 for timeouts.

#### Session Lifecycle

```bash
# Launch a debug session with initial breakpoints
krometrail launch "python app.py" \
  --break order.py:147 \
  --break discount.py:23 \
  --stop-on-entry

# Launch with a conditional breakpoint
krometrail launch "python -m pytest tests/test_order.py -x" \
  --break "order.py:147 when discount < 0"

# Launch with language override
krometrail launch "cargo test" --language rust

# Check session status
krometrail status

# Stop the active session
krometrail stop

# Stop a specific session
krometrail stop --session abc123
```

#### Execution Control

```bash
# Continue to next breakpoint
krometrail continue

# Step over / into / out
krometrail step over
krometrail step into
krometrail step out

# Step multiple times
krometrail step over --count 5

# Run to a specific line
krometrail run-to order.py:150

# Continue with a timeout
krometrail continue --timeout 10000
```

#### Breakpoint Management

```bash
# Set breakpoints (replaces existing in that file)
krometrail break order.py:147
krometrail break order.py:147,150,155

# Conditional breakpoints
krometrail break "order.py:147 when discount < 0"
krometrail break "order.py:147 hit >=100"

# Log points (log instead of breaking)
krometrail break "order.py:147 log 'discount={discount}, total={total}'"

# Exception breakpoints
krometrail break --exceptions uncaught
krometrail break --exceptions raised    # Python: all raised exceptions

# List all breakpoints
krometrail breakpoints

# Remove breakpoints from a file
krometrail break --clear order.py
```

#### State Inspection

```bash
# Evaluate an expression in current frame
krometrail eval "cart.items[0].__dict__"
krometrail eval "len(results)" --depth 3

# Evaluate in a different stack frame
krometrail eval "request.headers" --frame 2

# Show variables (current frame locals by default)
krometrail vars
krometrail vars --scope global
krometrail vars --scope closure
krometrail vars --filter "^user"

# Full stack trace
krometrail stack
krometrail stack --frames 20 --source

# View source for any file
krometrail source discount.py
krometrail source discount.py:15-30
```

#### Session Intelligence

```bash
# Add watch expressions
krometrail watch "len(cart.items)" "user.tier" "total > 0"

# View the session investigation log
krometrail log
krometrail log --detailed

# View captured program output
krometrail output
krometrail output --stderr
krometrail output --since-action 5
```

#### Utility

```bash
# Check which adapters/debuggers are available
krometrail doctor

# Show version and config
krometrail --version

# JSON output mode (for programmatic consumption)
krometrail launch "python app.py" --break order.py:147 --json

# Quiet mode (viewport only, no chrome)
krometrail continue --quiet
```

### 6.3 Session Daemon

The CLI manages sessions via a lightweight background daemon that starts automatically on the first `krometrail launch` and shuts down after the last session ends (or after an idle timeout). This allows sequential commands to operate on a persistent debug session without the user managing server lifecycle:

```bash
# These are separate shell commands that share a session:
krometrail launch "python app.py" --break order.py:147
# daemon starts, session created, viewport printed

krometrail continue
# daemon already running, continues the existing session

krometrail eval "discount"
# evaluates in the stopped session

krometrail stop
# session ends, daemon idles then shuts down
```

The daemon listens on a Unix domain socket at `$XDG_RUNTIME_DIR/krometrail.sock` (or `~/.krometrail/krometrail.sock` as fallback). Multiple concurrent sessions are supported — when more than one session is active, commands require `--session <id>` to disambiguate.

### 6.4 Agent Skill File

For agents that use the CLI path, a skill file teaches the agent how to use Krometrail. This can be loaded as a Claude Code skill, a Codex system prompt addition, or any agent's instruction set:

```markdown
# Krometrail — Debugging Skill

You have access to `krometrail`, a CLI debugger. Use it when you need to
inspect runtime state to diagnose a bug — especially when static code
reading and test output aren't enough to identify the root cause.

## Quick start
  krometrail launch "<command>" --break <file>:<line>
  krometrail continue          # run to next breakpoint
  krometrail step into|over|out
  krometrail eval "<expr>"     # evaluate expression at current stop
  krometrail vars              # show local variables
  krometrail stop              # end session

## Conditional breakpoints
  krometrail break "<file>:<line> when <condition>"

## Strategy
1. Start by setting a breakpoint where you expect the bug to manifest.
2. Inspect locals. Look for unexpected values.
3. If the bad value came from a function call, set a breakpoint inside
   that function and re-launch.
4. Use `krometrail eval` to test hypotheses without modifying code.
5. Once you identify the root cause, stop the session and fix the code.

## Key rules
- Always call `krometrail stop` when done to clean up.
- Prefer conditional breakpoints over stepping through loops.
- Each command prints a viewport showing source, locals, and stack.
- If a session times out (5 min default), re-launch.
```

### 6.5 Output Format

The CLI defaults to the same plain-text viewport format described in Section 4. Additionally:

**`--json` flag:** Every command can output structured JSON instead. This is useful for agents that prefer to parse structured data, or for piping into other tools:

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

## 7. Language Adapter Interface

Each language adapter implements a single interface. The adapter's sole responsibility is to launch the debugee and return a DAP connection. All subsequent DAP communication is handled by the core.

### 7.1 Adapter Contract

```typescript
interface DebugAdapter {
  /** Unique identifier, e.g., "python", "node", "go" */
  id: string;

  /** File extensions this adapter handles */
  fileExtensions: string[];

  /** Human-readable name for error messages */
  displayName: string;

  /** Check if the adapter's debugger is available on this system */
  checkPrerequisites(): Promise<PrerequisiteResult>;

  /** Launch the debugee and return a DAP connection */
  launch(config: LaunchConfig): Promise<DAPConnection>;

  /** Attach to an already-running process */
  attach(config: AttachConfig): Promise<DAPConnection>;

  /** Clean up adapter-specific resources */
  dispose(): Promise<void>;
}

interface PrerequisiteResult {
  satisfied: boolean;
  missing?: string[];         // e.g., ["debugpy not installed"]
  installHint?: string;       // e.g., "pip install debugpy"
}

interface DAPConnection {
  reader: ReadableStream;     // DAP messages from debugger
  writer: WritableStream;     // DAP messages to debugger
  process?: ChildProcess;     // The debugee process, if launched
}

interface LaunchConfig {
  command: string;            // Full command to execute
  cwd?: string;
  env?: Record<string, string>;
  args?: string[];
  port?: number;              // Allocated by core, adapter should use this
}

interface AttachConfig {
  pid?: number;
  port?: number;
  host?: string;
}
```

### 7.2 Reference Adapters

| Language | Debugger | Extensions | Launch Pattern |
|----------|----------|------------|----------------|
| Python | debugpy | `.py` | `python -m debugpy --listen 0:PORT --wait-for-client script.py` |
| Node.js | built-in inspector | `.js`, `.ts`, `.mjs` | `node --inspect-brk=PORT script.js` |
| Go | delve (dlv) | `.go` | `dlv dap --listen :PORT` |
| Rust | codelldb | `.rs` | `codelldb --port PORT` |
| Java | java-debug-adapter | `.java` | `java -agentlib:jdwp=... -jar target.jar` |
| C/C++ | cppdbg (GDB/LLDB) | `.c`, `.cpp`, `.h` | `gdb --interpreter=dap ./binary` |

### 7.3 Adding a New Adapter

Adding support for a new language requires implementing the `DebugAdapter` interface. The typical effort involves:

1. Identifying the language's DAP-compatible debugger and its launch protocol.
2. Implementing `launch`/`attach` methods to start the debugger and return a DAP socket or stream.
3. Implementing `checkPrerequisites` to verify the debugger is installed.
4. Registering the adapter with the core's adapter registry.

No changes to the core, viewport logic, or MCP tool definitions are required. The adapter boundary is intentionally narrow to make contributions straightforward.

---

## 8. Session Management and Safety

### 8.1 Resource Limits

Debug sessions consume system resources and agent context. The server enforces configurable safety limits:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `session_timeout_ms` | `300000` | Max wall-clock time for a session (5 min) |
| `max_actions_per_session` | `200` | Max debug actions before forced termination |
| `max_concurrent_sessions` | `3` | Per-agent concurrent session limit |
| `step_timeout_ms` | `30000` | Max time to wait for a single stop event |
| `max_output_bytes` | `1048576` | Max debugee stdout/stderr captured (1 MB) |
| `max_evaluate_time_ms` | `5000` | Max time for expression evaluation |

When a limit is hit, the server returns a structured error with the limit name, the current value, and a suggestion (e.g., "Consider using conditional breakpoints to reduce step count").

### 8.2 Context Compression

Over a long debug session, accumulated viewport snapshots can consume significant context. The server provides three mechanisms:

**Automatic summarization.** The server maintains a running investigation log summarizing each action and its key observation. After every 10 actions, a compressed summary is appended. The agent can retrieve this via `debug_action_log` at any time, allowing earlier raw viewports to be dropped from context while preserving the reasoning chain.

**Viewport diffing.** When consecutive stops are in the same function, the viewport can optionally show only what changed (modified variables, new stack frames) rather than the full snapshot. Controlled by the `diff_mode` session parameter.

```
── STEP at order.py:148 (same frame) ──
Changed:
  charge_result = <ChargeResult: success=False, error="card_declined">
  (5 locals unchanged)
```

**Progressive compression.** As the action count increases, the viewport automatically reduces detail: fewer stack frames, shorter string previews, more aggressive object summarization. The agent can override this by explicitly requesting full detail via the drill-down tools.

### 8.3 Process Isolation

The debugee runs as a child of the MCP server. Key isolation considerations:

- The debugee inherits server permissions. For untrusted code, run inside a container or sandbox.
- Debugee stdout/stderr is captured up to the configured limit, available via `debug_output`.
- If the debugee crashes or hangs, the session transitions to `terminated`/`error` state with diagnostics.
- The server cleans up all child processes on shutdown, even if sessions are not explicitly stopped.

---

## 9. Agent Interaction Patterns

The tool interface supports several debugging strategies. These are documented here both as usage guidance and as patterns that could be included in the MCP server's tool descriptions to help agents reason about when to use each approach.

### 9.1 Hypothesis-Driven Debugging

The most common pattern. The agent has a hypothesis about the bug and uses the debugger to confirm or refute it:

1. Set a breakpoint at the suspected location.
2. Continue to the breakpoint and inspect locals.
3. Evaluate expressions to test the hypothesis.
4. If confirmed, stop debugging and fix the code. If refuted, set new breakpoints and continue.

### 9.2 Bisection Debugging

For bugs where the agent knows a value is wrong at point B but correct at point A:

1. Set breakpoints at A and B, confirm value correctness.
2. Set a breakpoint at the midpoint, check the value.
3. Repeat, halving the search space each iteration.

This is a pattern where agents can be *more systematic* than most human debuggers.

### 9.3 Exception Tracing

When a traceback doesn't reveal root cause:

1. Set an exception breakpoint to catch the exception before unwinding.
2. Inspect local state at the throw site.
3. Walk up the stack to understand how the bad state was constructed.

### 9.4 Watchpoint Convergence

For state mutation bugs:

1. Set a watch on the variable of interest.
2. Use conditional breakpoints to catch the specific mutation (e.g., `discount < 0`).
3. Trace the mutation backward through the call chain.

### 9.5 Trace Mapping

For understanding unfamiliar code paths:

1. Set breakpoints at every entry point to a module (`debug_set_breakpoints` with all function entries).
2. Run the program and observe the call sequence.
3. Use the session log to reconstruct the execution flow.

This is tedious for humans but trivial for an agent.

---

## 10. Implementation Roadmap

### Phase 1: Foundation

Core server with viewport abstraction and the Python adapter, built in **TypeScript on Bun**. Bun is chosen for direct access to the DAP/MCP TypeScript ecosystem (`@vscode/debugadapter`, `@modelcontextprotocol/sdk`), single-file compiled binaries via `bun build --compile`, and dual distribution via npm and standalone binary. Minimum viable product: an agent can launch a Python script, set breakpoints, step, inspect state, and evaluate expressions.

- MCP server scaffold with tool registration and session management
- **CLI with full command parity** — every MCP tool available as a shell command
- DAP client library in TypeScript, leveraging `@vscode/debugadapter` ecosystem
- Viewport renderer with configurable parameters
- Python adapter using debugpy
- Session daemon for CLI state persistence
- Agent skill file for CLI-based integration
- Compiled binary distribution via `bun build --compile` (Linux, macOS, Windows)
- Integration test suite for the full agent-to-debugger path (both MCP and CLI)
- Tool descriptions optimized for agent discovery and usage

### Phase 2: Multi-Language + Intelligence

- Node.js and Go adapters
- Session intelligence: watch expressions, session logging, viewport diffing
- Conditional breakpoint support verified across all adapters
- Context compression (automatic summarization, diff mode)

### Phase 3: Advanced Capabilities

- Rust, Java, and C/C++ adapters
- Attach-to-process for debugging running services
- Multi-threaded debugging with thread selection in viewport
- Remote debugging via DAP over TCP
- Progressive compression tied to action count

### Phase 4: Ecosystem

- Community adapter SDK with documentation and templates
- Adapter contribution guidelines and test harness
- Performance benchmarking: tokens per session, time to diagnosis, fix rate improvement
- Integration guides for Claude Code, Codex, and other MCP clients
- Published tool description patterns for optimal agent behavior

---

## 11. Open Questions

1. **Event delivery model.** Should the server use MCP notifications for async events (breakpoint hit while agent is thinking), or deliver all events synchronously via blocking tool calls? Blocking is simpler but prevents the agent from doing other work while waiting.

2. **Multi-threaded debugging.** How should the viewport handle multiple threads stopped at different locations? Show all threads? Only the active one? A thread selector tool?

3. **Attach vs. launch.** For long-running processes, attach mode is essential. How should the agent discover available processes? PID? Port? Service name?

4. **Security boundaries.** `debug_evaluate` can execute arbitrary code in the debugee's context. Should there be restrictions, or is this acceptable given agents already have code execution capability?

5. **Token budget awareness.** Should the server be aware of the agent's remaining token budget and proactively compress viewports as budget decreases? This would require a non-standard MCP extension.

6. **Integration with existing projects.** Given the active ecosystem (AIDB, mcp-debugger, mcp-dap-server, etc.), should the viewport/compression layer be built as a standalone middleware that wraps any existing MCP debug server, or as contributions to a specific project?

7. **Viewport format.** Should the viewport output be plain text (as shown in this document), structured JSON that the agent parses, or a hybrid? Plain text is more natural for LLMs; JSON is more precise for programmatic consumption.

---

## Appendix A: Viewport Rendering Pseudocode

```typescript
function renderViewport(
  session: DebugSession,
  config: ViewportConfig
): string {
  const frame = session.currentFrame;
  const lines: string[] = [];

  // Header
  lines.push(`── STOPPED at ${frame.file}:${frame.line} (${frame.function}) ──`);
  lines.push(`Reason: ${session.stopReason}`);
  lines.push('');

  // Call stack (truncated)
  const frames = session.stackFrames.slice(0, config.stack_depth);
  const totalFrames = session.stackFrames.length;
  lines.push(`Call Stack (${frames.length} of ${totalFrames} frames):`);
  for (const [i, f] of frames.entries()) {
    const marker = i === 0 ? '→' : ' ';
    const args = renderArgs(f.arguments, config);
    lines.push(`  ${marker} ${f.shortFile}:${f.line}  ${f.function}(${args})`);
  }
  lines.push('');

  // Source context
  const halfCtx = Math.floor(config.source_context_lines / 2);
  const startLine = Math.max(1, frame.line - halfCtx);
  const endLine = frame.line + halfCtx;
  const source = session.getSource(frame.file, startLine, endLine);
  lines.push(`Source (${startLine}–${endLine}):`);
  for (const sl of source) {
    const marker = sl.line === frame.line ? '→' : ' ';
    lines.push(`${marker}${String(sl.line).padStart(4)}│ ${sl.text}`);
  }
  lines.push('');

  // Locals
  const locals = session.getLocals(0, config.locals_max_items);
  lines.push('Locals:');
  for (const v of locals) {
    const rendered = renderValue(v.value, config.locals_max_depth, config);
    lines.push(`  ${v.name.padEnd(12)} = ${rendered}`);
  }

  // Watch expressions
  if (session.watchExpressions.length > 0) {
    lines.push('');
    lines.push('Watch:');
    for (const expr of session.watchExpressions) {
      const val = session.evaluate(expr, 0);
      const rendered = renderValue(val, 1, config);
      lines.push(`  ${expr.padEnd(20)} = ${rendered}`);
    }
  }

  return lines.join('\n');
}
```

---

## Appendix B: Example Agent Debugging Session

A complete example showing how an agent might use these tools to diagnose the discount bug:

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

## Appendix C: Example CLI Debugging Session

The same discount bug from Appendix B, diagnosed via the CLI by an agent with bash access and the skill file loaded:

```bash
$ krometrail launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
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
$ krometrail stop
$ krometrail launch "python -m pytest tests/test_order.py::test_gold_discount -x" \
    --break order.py:143

── STOPPED at order.py:143 (process_order) ──
...

$ krometrail step into

── STOPPED at discount.py:15 (calculate_discount) ──
Locals:
  user      = <User: tier="gold">
  subtotal  = 149.97

$ krometrail break "discount.py:23 when tier == 'gold'"
Breakpoint set: discount.py:23 (conditional: tier == 'gold')

$ krometrail continue

── STOPPED at discount.py:23 (calculate_discount) ──
Reason: conditional breakpoint

Locals:
  tier              = "gold"
  base_rate         = 1.0
  discount_amount   = 149.97

$ krometrail eval "tier_multipliers"
{"bronze": 0.05, "silver": 0.1, "gold": 1.0, "platinum": 0.2}

$ krometrail stop
Session abc123 ended. Duration: 12s, Actions: 6
```

Agent now knows: `tier_multipliers["gold"]` is `1.0` (100%) instead of `0.1` (10%). Root cause identified in 6 commands.

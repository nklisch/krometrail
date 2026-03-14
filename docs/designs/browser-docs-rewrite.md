# Design: Browser Docs Rewrite — Human-First, Agent Content to Reference

## Problem

The browser docs are written for agents — every page leads with CLI commands and MCP JSON payloads. But the docs site is for humans. A human never runs `krometrail session search <id> --event-types network_response --status-codes 500`; their agent does. Meanwhile, the actual human-facing interface — the floating control panel with Mark/Snap buttons, keyboard shortcuts, visual feedback — is completely undocumented.

### Current State (what's wrong)

| Page | Issue |
|------|-------|
| `overview.md` | Workflow says "via your agent or CLI" — doesn't mention the in-browser UI at all |
| `recording-sessions.md` | 100% CLI + MCP code blocks. Marker placement shown only as terminal commands. No mention of the ◎ Mark button or Ctrl+Shift+M |
| `markers-screenshots.md` | Same — markers shown as CLI/MCP only. Screenshots described as "automatic" with no mention of the 📷 Snap button or Ctrl+Shift+S |
| `investigation-tools/*` | Titled with tool names (`session_search`, `session_inspect`). Parameter tables, CLI examples. A human reader doesn't need to memorize these — the agent reads tool descriptions |

### What Humans Actually Experience

When a recording session is active, Chrome has a **floating control panel** injected in the bottom-right corner:

```
┌─────────────────────────┐
│ ● krometrail            │  ← green dot = recording active
├─────────────────────────┤
│  ◎ Mark    📷 Snap      │  ← two action buttons
├─────────────────────────┤
│  ⏱ auto: 5s             │  ← screenshot interval
└─────────────────────────┘
```

- **◎ Mark** (or Ctrl+Shift+M) — places a timeline marker. Button flashes green with "Marked!" for 1 second.
- **📷 Snap** (or Ctrl+Shift+S) — captures a screenshot. Button flashes blue with "Saved!" for 1 second.
- **Auto indicator** — shows the periodic screenshot interval, or "off" if disabled.

This is the human's primary interface during a recording. The docs never mention it.

---

## Design

### Principle: Audience Split

- **Human (docs site reader)**: Needs to know how to *use* browser observation — start a session, use the overlay, place markers, take screenshots, understand what their agent will see.
- **Agent (MCP/CLI consumer)**: Reads tool descriptions and `llms.txt` — not the docs site. Reference pages exist for completeness but are not the primary path.

### Sidebar Restructure

**Before:**
```
Browser Observation
  Overview
  Recording Sessions
  Investigation Tools
    Search
    Inspect
    Diff
    Replay Context
  Framework Observation
    React
    Vue
  Markers & Screenshots
```

**After:**
```
Browser Observation
  Overview
  Recording & Controls        ← renamed, leads with the in-browser UI
  Markers & Screenshots       ← promoted above investigation tools
  What Your Agent Sees        ← new section title, reframed
    Search
    Inspect
    Diff
    Replay Context
  Framework Observation
    React
    Vue
```

Key changes:
- "Markers & Screenshots" moves up — it's the human's primary interaction
- "Investigation Tools" becomes "What Your Agent Sees" — frames it as capabilities, not commands
- "Recording Sessions" becomes "Recording & Controls" — includes the overlay panel

---

## Implementation Units

### Unit 1: `overview.md` — Human-first framing

**Changes:**
- Remove "via your agent or CLI" framing
- Replace "How It Works" numbered list with a visual showing the human ↔ agent split
- Show the workflow as: you browse → you mark → agent investigates
- Add "manual snaps" to the Screenshots row in the capture table
- Remove MCP tool names from Next Steps links
- No CLI/MCP code blocks

### Unit 2: `recording-sessions.md` → "Recording & Controls"

**Changes:**
- Rename title to "Recording & Controls"
- Keep "Starting a Recording" section — this *is* something a human might run (or ask their agent to run). Keep a single CLI example, remove MCP JSON blocks.
- **New section: "The Control Panel"** — describe the floating overlay, its position, visual design, and behavior. This becomes the centerpiece of the page.
- **New section: "Keyboard Shortcuts"** — table of Ctrl+Shift+M and Ctrl+Shift+S
- Remove "Checking Recording Status" (agent operation)
- Remove "Placing Markers" CLI/MCP blocks — markers are covered by the control panel section and the markers-screenshots page
- Keep "Stopping a Recording" as a brief note (single CLI command)
- Remove "Listing Recorded Sessions" and "Session Overview" — these are agent operations
- Remove "Tips" section about MCP tool timing — replace with tips about the overlay

### Unit 3: `markers-screenshots.md` — Lead with the UI

**Changes:**
- Lead "Markers" section with the ◎ Mark button and Ctrl+Shift+M shortcut
- Describe the green flash feedback ("Marked!" for 1 second)
- Explain *why* markers matter — they anchor your agent's investigation. When you mark "before submit" and "after error", your agent can diff those two moments.
- Remove all CLI/MCP code blocks for placing markers
- Lead "Screenshots" section with the 📷 Snap button and Ctrl+Shift+S shortcut
- Describe the blue flash feedback ("Saved!" for 1 second)
- Explain auto-capture: periodic interval (shown in panel footer) + navigation triggers
- Remove parameter tables and CLI examples
- Keep "Tips for Effective Marking" but reword to reference buttons/shortcuts instead of CLI commands

### Unit 4: Investigation tool pages — Reframe as agent capabilities

**`search.md`** — Reframe as "Search":
- Remove the `session_search` title. Just "Search".
- Lead with what it does from the human's perspective: "Your agent can search everything recorded in a session — find failed API calls, console errors, framework bugs, or any text."
- List the *kinds* of searches possible (full-text, by event type, by status code, by framework pattern, by time range) as a capability list, not a parameter table
- Remove all CLI/MCP code blocks
- Remove the Parameters table (that's reference material)
- Keep the "Event Types" table — it helps humans understand what was captured
- Keep "Example Workflows" but describe them in prose, not CLI commands

**`inspect.md`** — Reframe as "Inspect":
- Remove `session_inspect` title. Just "Inspect".
- Lead with: "Your agent can deep-dive into any event — full request/response bodies, stack traces, component state, and a screenshot showing exactly what was on screen."
- Keep the "What You Get" section — rename to "What Your Agent Sees" and keep the per-event-type breakdowns
- Remove CLI/MCP code blocks and parameter table
- Keep "Screenshot Context" section — explain that every inspection includes the nearest screenshot

**`diff.md`** — Reframe as "Diff":
- Remove `session_diff` title. Just "Diff".
- Lead with: "Your agent can compare two moments in a session — what changed between 'working' and 'broken'."
- Explain how markers enable this: "When you place markers at key moments, your agent can diff the state between them."
- Keep "What Gets Compared" section
- Remove CLI/MCP code blocks and parameter table
- Rewrite the form submission example in prose instead of CLI commands

**`replay-context.md`** — Reframe as "Replay Context":
- Remove `session_replay_context` title. Just "Replay Context".
- Lead with: "Your agent can generate reproduction steps or test scaffolds from your recorded session."
- Keep the output format examples (steps, Playwright, Cypress) — these are useful for humans to understand what they'll get
- Remove CLI/MCP code blocks and parameter table

### Unit 5: VitePress sidebar config update

**File**: `docs/.vitepress/config.ts`

Update the `/browser/` sidebar to match the new structure:
- Rename "Recording Sessions" → "Recording & Controls"
- Move "Markers & Screenshots" above investigation tools
- Rename "Investigation Tools" → "What Your Agent Sees"

### Unit 6: `llms.txt` + `llms-full.txt`

**New files**: `docs/public/llms.txt` and build-time generation for `llms-full.txt`

**`llms.txt`** — Static index file:
```markdown
# Krometrail

> MCP server and CLI that gives AI coding agents runtime debugging via the Debug Adapter Protocol and browser observation via Chrome DevTools Protocol.

## Docs

- [Getting Started](/guide/getting-started): Install Krometrail and configure it with your AI coding agent.
- [MCP Configuration](/guide/mcp-configuration): Configure Krometrail as an MCP server for Claude, Cursor, or other agents.
- [CLI Installation](/guide/cli-installation): Install the standalone CLI binary.

## Browser Observation

- [Overview](/browser/overview): What browser observation captures and how it works.
- [Recording & Controls](/browser/recording-sessions): Start sessions and use the in-browser control panel.
- [Markers & Screenshots](/browser/markers-screenshots): Timeline markers and screenshot capture.
- [Search](/browser/investigation-tools/search): Full-text and structured event search across sessions.
- [Inspect](/browser/investigation-tools/inspect): Deep-dive into individual events with screenshots.
- [Diff](/browser/investigation-tools/diff): Compare two moments in a session.
- [Replay Context](/browser/investigation-tools/replay-context): Generate reproduction steps and test scaffolds.
- [React Observation](/browser/framework-observation/react): React component lifecycle and bug pattern detection.
- [Vue Observation](/browser/framework-observation/vue): Vue 2/3, Pinia, and Vuex state observation.

## Runtime Debugging

- [Overview](/debugging/overview): Runtime debugging concepts for AI agents.
- [Breakpoints & Stepping](/debugging/breakpoints-stepping): Set breakpoints, step through code, conditional breaks.
- [Variables & Evaluation](/debugging/variables-evaluation): Inspect variables and evaluate expressions at breakpoints.
- [Watch Expressions](/debugging/watch-expressions): Track variable changes across steps.
- [Context Compression](/debugging/context-compression): How viewport output is compressed for agent context windows.

## Language Support

- [Python](/languages/python): debugpy adapter setup.
- [Node.js / TypeScript](/languages/nodejs): js-debug adapter setup.
- [Go](/languages/go): Delve adapter setup.
- [Rust](/languages/rust): CodeLLDB adapter setup.
- [Java](/languages/java): java-debug adapter setup.
- [C / C++](/languages/cpp): cppdbg adapter setup.
- [Ruby](/languages/ruby): rdbg adapter setup.
- [C#](/languages/csharp): netcoredbg adapter setup.
- [Swift](/languages/swift): lldb-dap adapter setup.
- [Kotlin](/languages/kotlin): kotlin-debug-adapter setup.

## Reference

- [MCP Tools](/reference/mcp-tools): Complete MCP tool reference with parameters and examples.
- [CLI Commands](/reference/cli-commands): Full CLI command reference.
- [Configuration](/reference/configuration): All configuration options.

## Optional

- [Viewport Format](/reference/viewport-format): Internal viewport output format specification.
- [Adapter SDK](/reference/adapter-sdk): Build custom language adapters.
```

**`llms-full.txt`** — Generated at build time by concatenating all docs pages. Add a script to `scripts/generate-llms-full.ts` that:
1. Reads all `.md` files from `docs/` (excluding `.vitepress/`, `designs/`, `legacy/`, `.generated/`)
2. Strips frontmatter
3. Concatenates with `# {filename}` headers
4. Writes to `docs/public/llms-full.txt`

Hook into the existing `docs:build` script.

---

## Files Changed

| File | Action |
|------|--------|
| `docs/browser/overview.md` | Rewrite — human-first framing |
| `docs/browser/recording-sessions.md` | Rewrite — control panel focus, strip MCP |
| `docs/browser/markers-screenshots.md` | Rewrite — button/shortcut focus, strip CLI/MCP |
| `docs/browser/investigation-tools/search.md` | Rewrite — capability framing, strip CLI/MCP |
| `docs/browser/investigation-tools/inspect.md` | Rewrite — capability framing, strip CLI/MCP |
| `docs/browser/investigation-tools/diff.md` | Rewrite — capability framing, strip CLI/MCP |
| `docs/browser/investigation-tools/replay-context.md` | Rewrite — capability framing, strip CLI/MCP |
| `docs/.vitepress/config.ts` | Sidebar restructure |
| `docs/public/llms.txt` | New — LLM navigation index |
| `scripts/generate-llms-full.ts` | New — build-time llms-full.txt generator |

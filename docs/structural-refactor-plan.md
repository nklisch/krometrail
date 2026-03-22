# Structural Refactor Plan

Generated 2026-03-21. **All High Value items implemented.**

**Rules with zero violations:** Curated Barrels, Import Direction, Kebab-Case Naming, Test Mirror Tree, Centralized Types.

---

## High Value

Structural changes that significantly improve navigability with low risk.

---

#### 1. Split `src/core/session-manager.ts` (1,365 lines)

**Files:** `src/core/session-manager.ts` → 4 extracted modules + slimmed coordinator
**Rule:** File Size (~500 line soft limit)

SessionManager is a monolithic facade handling 6 distinct concerns: session lifecycle,
execution control, breakpoint management, state inspection, watch expressions, and output
capture. Each concern has clear boundaries and could be its own module.

**Current:**
```
src/core/
  session-manager.ts   (1,365 lines — everything)
```

**Target:**
```
src/core/
  session-manager.ts        (~400 lines — lifecycle, launch, attach, stop, coordination)
  breakpoint-manager.ts     (~100 lines — set/list breakpoints, exception breakpoints)
  execution-controller.ts   (~150 lines — continue, step, runTo, thread resolution)
  state-inspector.ts        (~200 lines — evaluate, variables, stack trace, source, frame resolution)
  session-output.ts         (~100 lines — output buffer, session log formatting)
```

**Implementation Notes:**
- SessionManager retains the public API surface — extracted modules are internal helpers
- Pass the DAPClient instance to extracted modules (dependency injection, not inheritance)
- Watch expression management can stay in session-manager (tightly coupled to stop handling)
- Viewport rendering stays in session-manager (orchestrates compression + state)
- All imports from other modules still go through `session-manager.ts` exports

**Acceptance Criteria:**
- [ ] Each extracted module is under 300 lines
- [ ] SessionManager is under 500 lines
- [ ] All 15+ public methods still accessible from session-manager.ts
- [ ] No circular imports between extracted modules
- [ ] `bun run test:unit` passes
- [ ] `bun run test:integration` passes
- [ ] `bun run lint` passes

---

#### 2. Split `src/daemon/server.ts` (709 lines)

**Files:** `src/daemon/server.ts` → handler modules + slimmed server
**Rule:** File Size (~500 line soft limit)

The daemon server mixes two concerns: daemon lifecycle management (socket, PID, idle timeout)
and RPC method dispatch (20+ handler methods). The handlers should be extracted.

**Current:**
```
src/daemon/
  server.ts    (709 lines — lifecycle + all RPC handlers)
  client.ts
  protocol.ts
```

**Target:**
```
src/daemon/
  server.ts                  (~200 lines — lifecycle, socket, PID, idle timeout, RPC routing)
  session-handlers.ts        (~200 lines — session.launch, session.attach, session.continue, etc.)
  browser-handlers.ts        (~250 lines — browser.start, browser.stop, browser.overview, etc.)
  client.ts
  protocol.ts
```

**Implementation Notes:**
- server.ts keeps the RPC router that dispatches to handler functions
- Handler modules export functions that take (params, sessionManager, browserState) args
- Browser handlers need access to BrowserRecorder, QueryEngine, SessionDiffer instances —
  pass via a context object rather than making them globals
- Error code mapping stays in server.ts (cross-cutting concern)

**Acceptance Criteria:**
- [ ] server.ts under 250 lines
- [ ] Each handler module under 300 lines
- [ ] All 20+ RPC methods still work
- [ ] `bun run test:unit` passes
- [ ] `bun run test:e2e` passes
- [ ] `bun run lint` passes

---

#### 3. Consolidate `docs/guide/` and `docs/guides/`

**Files:** Two directories with the same purpose
**Rule:** Docs Hierarchy (consistent structure)

**Current:**
```
docs/
  guide/           (5 files: cli-installation, faq, first-debug-session, getting-started, mcp-configuration)
  guides/          (4 files: claude-code, codex, cursor-windsurf, troubleshooting)
```

**Target:**
```
docs/
  guide/           (all 9 files consolidated)
```

**Implementation Notes:**
- Check VitePress sidebar config in `docs/.vitepress/` for references to both paths
- Update any cross-references between guide pages
- Check if the docs site has published URLs that would break (redirect if needed)
- Prefer singular `guide/` to match VitePress conventions

**Acceptance Criteria:**
- [ ] All guide content in one directory
- [ ] VitePress config updated
- [ ] No broken links in docs
- [ ] Docs site builds successfully

---

## Worth Considering

Valid reorganizations with moderate impact or moderate effort.

---

- **`src/browser/investigation/query-engine.ts` (556 lines)** — Already has extracted `SessionDiffer` and `ReplayContextGenerator` classes, but `QueryEngine` still implements overview generation, search, and body reading inline. Could delegate to these existing specialists and extract body-reading utilities to a `body-reader.ts`. Would drop to ~300 lines.

- **`src/browser/recorder/index.ts` (502 lines)** — Just over the soft limit. Chrome launch/attach logic could move to `chrome-launcher.ts` (which already exists but handles less). Borderline — the file is a coherent orchestrator.

- **`src/cli/commands/index.ts` (710 lines)** — The `getClient()`, `resolveSessionId()`, and `runCommand()` helpers (~50 lines) are shared CLI infrastructure mixed with command re-exports. Could extract to `cli/commands/shared.ts`. Minor improvement.

---

## Not Worth It

Code that technically violates a rule but should NOT be reorganized.

---

- **`src/cli/commands/debug.ts` (741 lines)** — File Size. This is a command registry containing 15 `defineCommand()` definitions. Each follows the same template. Splitting by command would create 15 tiny files with no shared logic — pure file sprawl. The file is scannable because every command follows the same pattern.

- **`src/cli/commands/browser.ts` (575 lines)** — File Size. Same rationale as debug.ts. 11 command definitions following the citty template.

- **`src/mcp/tools/index.ts` (514 lines)** — File Size. 18 MCP tool registrations (`server.tool()` calls). Sequential registration in one file is the documented pattern (see `mcp-tool-handler.md` pattern skill). Splitting would scatter related tool definitions.

- **`src/browser/types.ts` + `src/browser/executor/types.ts`** — Centralized Types. Two types files in the browser domain. However, the rule's exception explicitly allows splitting by sub-domain, and executor/ is a distinct sub-domain with its own types (StepRequest, StepResult, CaptureConfig). Consolidating would create one 290-line types file without improving discoverability.

- **`src/browser/SKILL.md`** — Docs Hierarchy. This is a workflow guide for the `krometrail-chrome` agent skill, not a documentation file. It lives next to the code it describes and is referenced by the skill system. Moving it to docs/ would break skill discovery.

- **`docs/stylistic-refactor-plan.md` at docs root** — Docs Hierarchy. This is a generated output from the stylistic-refactor skill, not a design doc. It belongs at the docs root as a working document, not in `designs/completed/` (which is for historical phase designs).

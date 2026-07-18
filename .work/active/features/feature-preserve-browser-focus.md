---
id: feature-preserve-browser-focus
kind: feature
stage: implementing
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Operate a visible managed browser without stealing desktop focus

Add an explicit per-session focus policy for managed browser launches. Preserve a normal visible
Chrome window so users can watch, while allowing an agent to navigate and interact with the current
visible tab without Krometrail activating Chrome or switching foreground tabs. Keep the existing
foregrounding behavior as the stable default.

## Strategic decisions

- **Visible browser**: retain a normal managed Chrome window rather than using headless mode — users
  should be able to watch and can bring the window forward themselves.
- **Compatibility default**: `foreground` remains the omitted/default 1.x policy.
- **Preserve semantics**: `preserve` forbids Krometrail from issuing target-activation or
  bring-to-front commands. Operations on the already visible tab continue normally; pointer work on
  a hidden tab fails explicitly as `target_hidden` instead of stealing focus.
- **Scope boundary**: apply the policy to managed launch sessions first. Attachment remains under the
  externally owned browser's existing behavior until a real adopter requires the same choice there.

## Architectural choice

Extend `LaunchBrowser` with a typed `BrowserFocusPolicy` (`foreground` or `preserve`) and copy that
immutable value into session supervision. Route every Krometrail-owned activation decision through
that policy: page creation/selection does not call `Target.activateTarget` in preserve mode, and
hidden pointer preparation returns its existing recoverable `target_hidden` error without calling
`Target.activateTarget` or `Page.bringToFront`. This is preferable to platform-specific window APIs,
which would not cover later tab activation and would weaken managed-process ownership.

## Implementation units

### Unit 1: Generated launch contract and session policy

**Files**: `crates/krometrail-core/src/ports/browser.rs`, exported core surfaces, and
`crates/krometrail-cdp/src/session/mod.rs`.

```rust
pub enum BrowserFocusPolicy { Foreground, Preserve }

pub struct LaunchBrowser {
    pub executable: Option<PathBuf>,
    pub profile: ManagedProfile,
    pub initial_url: Option<String>,
    pub every_nth_frame: EveryNthFrame,
    pub focus: BrowserFocusPolicy,
}
```

The wire schema defaults to `foreground`; typed serialization preserves explicit `preserve`.

**Acceptance criteria**:
- [ ] Omitted `focus` decodes to `foreground` and preserves existing launch behavior.
- [ ] Generated `start_browser` schema advertises exactly `foreground | preserve`.
- [ ] Session supervision retains the immutable launch policy without a second configuration source.

### Unit 2: Policy-complete target activation

**Files**: `crates/krometrail-cdp/src/session/operations.rs`,
`crates/krometrail-cdp/src/control/interaction.rs`, and focused lifecycle/control tests.

```rust
async fn prepare_pointer_target(
    &self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    focus: BrowserFocusPolicy,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()>;
```

Foreground mode keeps today's bounded activation-and-visibility behavior. Preserve mode never sends
activation commands. Create/select still update Krometrail's logical selected target, keeping logical
selection distinct from Chrome foreground state.

**Acceptance criteria**:
- [ ] Visible-target actions add no activation overhead in either policy.
- [ ] Hidden-target pointer actions retain current activation behavior under `foreground`.
- [ ] Hidden-target pointer actions under `preserve` return `target_hidden` and dispatch neither
  activation nor pointer events.
- [ ] Create/select under `preserve` update logical selection without `Target.activateTarget`.

### Unit 3: Contract and agent guidance

**Files**: `docs/SPEC.md`, `docs/ARCHITECTURE.md`, and `plugin/skills/krometrail/SKILL.md`.

Describe the policy as visible-window focus preservation, not guaranteed background rendering for
hidden tabs. Teach agents to request preserve mode for uninterrupted user work, stay on one visible
tab when possible, and use foreground mode when the user wants automatic tab switching.

**Acceptance criteria**:
- [ ] Foundation docs state the exact default, preserve-mode limit, and logical-selection behavior.
- [ ] The skill includes an exact focus-preserving `start_browser` request.

## Implementation order

1. Add the typed launch/session policy and generated-schema tests.
2. Apply it to create/select and hidden pointer preparation with focused regression tests.
3. Update foundation assertions and skill instructions.
4. Run focused tests, workspace checks, and a visible-browser manual qualification.

## Simplification

One immutable policy replaces scattered assumptions about whether foregrounding is permitted. The
design adds no headless launcher, platform window-control adapter, or second browser supervisor.

## Testing

- Core wire/schema tests protect the compatibility default and public MCP shape.
- Scripted CDP lifecycle tests assert exact presence/absence of activation calls.
- Pointer-control tests assert that preserve mode fails before activation or input dispatch.
- Manual macOS qualification keeps Chrome visible, places another app in front, and verifies that
  current-tab actions do not request foregrounding.

## Risks

Chrome may pause capture or reject geometry/input for hidden tabs. Preserve mode does not pretend
otherwise: hidden pointer work fails specifically, capture visibility remains reported, and the agent
can ask the user to foreground Chrome or start a foreground-policy session when required. The initial
Chrome process launch may still be surfaced by the operating system; the policy governs Krometrail's
subsequent activation commands rather than making a cross-platform promise about OS launch policy.

---
title: Use Krometrail with your agent
description: Ask a coding agent to reproduce, inspect, and verify transient browser behavior with Krometrail.
---

# Use Krometrail with your agent

Krometrail is most useful when the final screenshot is not enough.

Your agent can operate the browser normally and inspect the current page. Meanwhile, Krometrail records the controlled renderer. If the page flickers, shifts, reverses, or briefly shows the wrong state, you can ask the agent to look back at the interaction instead of trying to catch the moment manually.

## Start with the symptom

Describe what you saw in ordinary language. You do not need to name a Krometrail tool or know which artifact will help.

> Open `http://localhost:3000/settings` with Krometrail and reproduce the panel animation. The panel sometimes appears to move backward before it settles. Inspect what happened around the interaction and tell me which visible states appeared, in order.

Your agent may ask for permission before it launches or controls a browser. Krometrail intentionally follows your agent harness and operator approval policy.

## Useful requests

### A short-lived flash

> Use Krometrail to submit the form. The success state briefly flashes the wrong color. Inspect the temporal evidence around the submission, then show me the source frame where the wrong color appears.

### A layout jump

> Record the page while it loads. Check whether the main content shifts after the initial render, identify the affected region, and correlate it with any navigation or browser errors.

### Motion in one small area

> Reproduce the cart animation and inspect only the cart region as a filmstrip. I want to see whether the badge overshoots or reverses before reaching its final position.

### A canvas or game surface

> Use Krometrail while triggering the player movement. Inspect the visual timeline for a teleport or incorrect intermediate frame. Do not rely on DOM state—the problem is inside the canvas.

### Verify a fix

> Repeat the same interaction after my patch. Compare the visible sequence with the original symptom and verify that the transition is fixed, not just the final screenshot.

## What your agent does

A typical investigation is short:

1. **Work normally.** The agent starts a controlled browser, navigates, clicks, types, or scrolls. Recording begins with the browser session.
2. **Reproduce the symptom.** Tell the agent what looked wrong, especially when the final state appears correct.
3. **Inspect the interval.** The agent asks for evidence around the interaction or a selected time range.
4. **Zoom in when needed.** It requests a focused region or exact source frames rather than loading the entire recording.
5. **Fix and repeat.** The agent changes the application and verifies the same transition again.

These are choices, not mandatory stages. A current screenshot may be enough for a current-state question. Temporal evidence earns its keep when sequence and timing matter.

## Test responsive and high-DPI layouts

Ask the agent to use `set_viewport` when a reproduction depends on explicit CSS dimensions, device pixel ratio, mobile layout, or touch emulation. The override belongs to one browser target and survives navigation and managed reconnects. Ask it to clear the override when the test is finished; clearing restores the browser's native metrics.

Changing viewport metrics starts a new observation geometry. Snapshot references from the earlier geometry must be reacquired before pointer actions, and retained frames report the effective device scale that applied when each frame was captured. This keeps source-frame coordinates and derived visual evidence interpretable across responsive-layout transitions.

## What the evidence means

Krometrail reorganizes recorded frames into still images an agent can inspect:

- **Before / during / after** gives a quick orientation around an interaction.
- **Storyboard** shows representative frames in chronological order.
- **Difference map** shows where pixels changed and roughly when; it does not explain why.
- **Region filmstrip** keeps a small area readable across several moments.
- **Motion history** highlights repeated movement or broad oscillation without claiming direction.
- **Source frames** are the underlying visual record behind every derived artifact.
- **Browser events** add nearby console, exception, navigation, and request context.

Generated artifacts are compact, lossy views. When a detail matters, ask the agent to inspect the source frames behind it.

## Ask for careful conclusions

A useful prompt tells the agent how to handle uncertainty:

> Inspect the temporal bundle around the last interaction. Describe the observed sequence, cite the relevant source frames, and tell me whether capture gaps limit the conclusion. Do not infer a cause from a difference map alone.

Known capture gaps are shown with the evidence. A gap means unseen behavior may have occurred; it does not prove the missing interval was stable.

## When to keep evidence

Old unpinned recording segments can be evicted when the local disk budget is reached. For a longer investigation, ask your agent to pin the relevant interval and unpin it when the work is complete.

## Finish the session

When Krometrail launched the browser for the task, ask your agent to stop it when finished unless you want the controlled session left open.

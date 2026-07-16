---
layout: home
title: "Krometrail — Browser memory for coding agents"
titleTemplate: false
description: "Krometrail records a controlled browser session so coding agents can inspect flicker, layout jumps, reversed motion, and other transient visual bugs."

hero:
  name: Krometrail
  text: Browser memory for coding agents
  tagline: Catch the flicker, jump, and half-rendered state that disappears before the next screenshot.
  actions:
    - theme: brand
      text: Install Krometrail
      link: /guide/installation
    - theme: alt
      text: Use it with your agent
      link: /guide/using-krometrail

features:
  - title: See the moments screenshots miss
    details: Preserve short-lived states around an interaction—even when the final page looks correct.
  - title: Give your agent evidence, not video
    details: Turn an interval into storyboards, difference maps, filmstrips, and source frames an agent can inspect.
  - title: Keep browser evidence local
    details: Browser control, recording, and generated artifacts run on your machine.
---

A screenshot tells your coding agent what a page **is**. Krometrail preserves what the page **did**.

While your agent works in a controlled Chrome or Chromium browser, Krometrail records the visible timeline. If a component flashes, an animation moves backward, hydration briefly shows the wrong layout, or focus jumps and settles before the next screenshot, the evidence is already there.

## Install for your agent

The native plugin is the easiest path. It gives Claude Code or Codex the Krometrail skill, connects the local MCP server, and manages the matching binary for you.

### Claude Code

```bash
claude plugin marketplace add nklisch/krometrail --scope user
claude plugin install krometrail@krometrail --scope user
```

### Codex

```bash
codex plugin marketplace add nklisch/krometrail
codex plugin add krometrail@krometrail
```

Restart or reload your agent after installation. The first activation downloads and verifies the release paired with the plugin; later starts use the verified local copy.

Prefer a standalone command or manual MCP setup?

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
krometrail doctor
```

[Compare installation options →](/guide/installation)

## Ask your agent to look back

You do not need to predict the exact frame or script the capture. Describe the symptom and ask your agent to inspect what happened around the interaction.

> Use Krometrail to reproduce the settings-panel animation. It sometimes moves backward before it settles. Inspect the temporal evidence around that interaction and show me the relevant source frames.

Your agent can start with a compact temporal bundle, then zoom into a small region or retrieve exact frames when the first view is not enough.

[See practical prompts and workflows →](/guide/using-krometrail)

## What Krometrail helps investigate

- a component flickers before settling;
- an animation reverses direction or overshoots;
- a loading state appears and disappears too quickly;
- hydration briefly renders the wrong layout;
- scrolling or focus jumps unexpectedly;
- two updates compete before the page stabilizes;
- a canvas or game surface shows the wrong intermediate frame.

The final screenshot can look completely correct. The recorded interval still shows the sequence that led there.

## A normal debugging loop

1. Your agent opens and uses the page through Krometrail.
2. You reproduce the visual problem.
3. The agent inspects the interval around the action as compact still-image evidence.
4. It retrieves source frames or a focused filmstrip when more detail is needed.
5. It changes the application and repeats the interaction to verify the transition—not only the final state.

Krometrail preserves evidence. Your coding agent still interprets it and finds the cause.

## Local, bounded, and honest about gaps

Captured frames, browser events, and generated artifacts remain local unless your connected agent explicitly reads them through MCP. Recording uses a configurable disk budget and evicts old unpinned evidence when needed.

Krometrail does not promise to capture every browser-rendered frame or diagnose defects automatically. Chrome visibility, rendering activity, and local load can affect capture. Known gaps are reported with the evidence so your agent can avoid claiming more than the recording supports.

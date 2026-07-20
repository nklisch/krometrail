---
id: idea-eval-harness-browser-teardown
created: 2026-07-20
updated: 2026-07-20
tags: [testing, infra]
---

A headless Chrome launched by the `temporal-evaluation` harness was still running
five days after its run, discovered incidentally during the 2026-07-20 sixth
shakedown while inspecting the process table for the MCP server's binary path.

Observed state:

- Parent process started `Wed Jul 15 08:40:50`, still alive `Mon Jul 20`
- Command line pointed at a benchmark URL
  (`temporal-benchmark/index.html?case=movement-reversal/basic&duration_ms=16`)
- Still holding a profile directory under
  `target/temporal-evaluation/live/chrome/linux_stable_chrome_reference_host-*/profiles/tmp/`
- Zygote and GPU child processes also alive

This is the evaluation harness's own lifecycle gap, **not** the MCP server's
browser supervision — different owner, different code path. Worth keeping
separate from any product-side retention or cleanup work.

Two things to establish:

1. Why the harness does not tear down its browser process group on run
   completion, and whether that also leaks on abnormal exit (panic, timeout,
   CI cancellation).
2. Separately, confirm the MCP server *does* reliably reap its managed browsers
   on hard kill. This was not verified during the shakedown and is assumed rather
   than known. If it does not, that is a product bug and should be split out
   rather than fixed here.

Practical impact beyond the stale process: these hold disk (profile dirs under
`target/`) and may have contributed to the `/tmp` exhaustion that broke tooling
during the fifth shakedown session.

Origin: 2026-07-20 sixth shakedown against v1.2.6.

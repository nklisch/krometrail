---
id: mcp-discovery-probe-closes-legacy-stdio
created: 2026-09-06
updated: 2026-09-06
tags: [agent-ux, testing]
---

# Modern MCP discovery probe closes legacy stdio startup

## Finding

An independent Orogen MCP client cannot negotiate with Krometrail 1.6.3 using its default modern discovery-first path. Krometrail exits before sending a JSON-RPC response. A fresh process using legacy initialization first successfully negotiates and lists tools. This is a confirmed interoperability limitation of the selected older server/SDK, not a browser failure or evidence that a tool executed.

## Reproduction and observations

On Linux x86-64, launch the selected `krometrail mcp` binary with an empty inherited environment, disposable HOME/data/profile/temp roots, and separately supervised pipes. Keep stdin open. Send JSON-RPC request id `0`, method `server/discover`, with current protocol `2026-07-28` and empty client capabilities in request `_meta`.

Expected useful interoperability: bounded protocol rejection or negotiation that lets a client select the supported legacy exchange. Observed: no response, stdout closes, process exit `1` while its stdin is still open. Orogen rmcp 3.1.4 Auto reports `NegotiationFailed`; Orogen's owner reaps the child. No server JSON-RPC error code or tool-response correlation is available because no response was received.

An explicit separate diagnostic run sends `initialize` id `0` with requested `2025-11-25`. It receives `2025-06-18`, sends `notifications/initialized`, and receives 52 tools from `tools/list` id `1`. These are initialization/discovery operations, not tool calls. They authorize neither replay of a call nor silent process restarts.

## Version and source evidence

- Actual standalone binary: Krometrail 1.6.3, SHA-256 `a7b27d287d46beda5bf28376427d7f1784982bc8b8cd5128319d625e775565d3`.
- Public tag/source: `4d20efdc3487c7131496dba393e553e31bb51cfc`; `Cargo.toml` selects rmcp 0.11.0.
- Selected SDK source revision: `4c87f7f163000b95536308a8e594acd1b3f56b12`.
- [SDK server startup](https://github.com/modelcontextprotocol/rust-sdk/blob/4c87f7f163000b95536308a8e594acd1b3f56b12/crates/rmcp/src/service/server.rs#L185-L198) requires an InitializeRequest before returning a running server.
- In that same revision, `crates/rmcp/src/transport/async_rw.rs:119-127` converts a message decode error to end-of-stream; `:245-277` decodes the typed message and only tolerates selected unknown notifications, not arbitrary requests. Thus the earlier high-level initialize-first explanation does not prove that this unknown method reached the `ExpectedInitializeRequest(Some(...))` branch. A typed decode failure can terminate earlier. The exact internal error branch is source-supported but not independently instrumented in this release binary.

Private metadata-only Orogen receipts: `/storage/orogen-krometrail-wire-metadata.log` (original client wire), `/storage/orogen-fix-mcp-probe-2.log` and matching `.exit = 0` (independent probe confirms actual child exit). Orogen live failures remain in `/storage/orogen-krometrail-live-1.log` and `live-2.log`, `.exit = 101`. Probe success means the observation script completed, not server interoperability success. No browser was launched by these probes; no credential or page content was collected.

## Workaround and boundary

Orogen is implementing an explicit normalized `legacy_initialize` stdio selection using rmcp's existing Initialize lifecycle. Its default modern path remains available; no executable-name special case or automatic relaunch is proposed. Krometrail need not be patched or upgraded as a prerequisite for that bounded client path.

A future Krometrail change should deliberately select its supported protocol behavior and qualify discovery-first startup, rather than copying an SDK parser or claiming current-era support from legacy initialization alone. This capture authorizes no runtime change, dependency upgrade, public issue submission, or browser mutation.

## Separate client schema limitation

The advertised `oneOf` and `anyOf` constraints are legitimate inline JSON Schema, not a demonstrated Krometrail defect. Examples include `snapshot_page` at `$/properties/target` (`oneOf`), `create_page` at `$/properties/initial_url` (`anyOf`), and `fetch_source_frames` at the root (`oneOf`). Krometrail's schema projection resolves local references before publication. Orogen's former host subset rejected these keywords; that is being corrected in Orogen using its existing validator without removing constraints.

The complete public tool list is approximately 900,845 compact-JSON bytes (largest input schema 45,705 bytes), also exceeding Orogen's former 256 KiB aggregate discovery allowance. That limit and its misleading `InvalidToolCount` report are Orogen limitations, not an invalid Krometrail catalogue. No separate upstream schema bug is filed without evidence of an invalid advertised schema.

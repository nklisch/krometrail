// Installed adapter resolver regression: structured facts must survive alongside
// native MCP content. This is not full host/server/browser qualification.
// Usage: node agent-result-delivery.mjs /absolute/path/to/@nklisch/pi-mcp-adapter
// No model, browser, network, user config, or persistent state is opened.
// Samples are deliberately synthetic MCP envelopes, NOT captured Rust outputs.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
assert(root && isAbsolute(root), "provide an absolute installed adapter package directory");
const load = (file) => import(pathToFileURL(resolve(root, file)).href);
const require = createRequire(resolve(root, "package.json"));
const { CallToolResultSchema } = require("@modelcontextprotocol/core");
const { resolveMcpResultContent } = await load("dist/tool-registrar.js");
const { guardMcpOutput } = await load("dist/mcp-output-guard.js");
const { version } = JSON.parse(await readFile(resolve(root, "package.json"), "utf8"));

// Only these small semantic sentinels matter to the transport diagnosis. They
// are not a second domain model or assertions about complete Rust field shapes.
const cases = [
  ["list_pages", { target_id: "fixture-page-id" }],
  ["browser_status", { session_id: "fixture-session-id" }],
  ["inspect_page", { observation: "fixture-page-observation" }],
  ["resolve_temporal_range", { range_handle: "fixture-range-handle" }],
  ["take_screenshot", { correlation_id: "fixture-diagnostic-correlation" }, true],
  ["click", { interaction_id: "fixture-dispatched-interaction" }, false, "degraded"],
];
for (const [tool, sentinel, isError = false, status = isError ? "failed" : "succeeded"] of cases) {
  const wire = JSON.stringify({
    jsonrpc: "2.0", id: 1,
    result: {
      content: [{ type: "text", text: `${tool} ${status}` }],
      structuredContent: { tool, status, result: sentinel },
      isError,
    },
  });
  const decoded = CallToolResultSchema.parse(JSON.parse(wire).result);
  const projected = resolveMcpResultContent(decoded);
  const guarded = await guardMcpOutput(projected, {
    enabled: true, rawMcpResult: decoded,
    ...(isError ? { prefix: "Error: " } : {}),
  });
  assert.deepEqual(decoded.structuredContent.result, sentinel);
  assert.deepEqual(guarded.mcpResult, decoded);
  assert.equal(guarded.outputGuard, undefined);
  const visibleText = guarded.content.filter((block) => block.type === "text").map((block) => block.text).join("\n");
  assert(visibleText.includes(`${isError ? "Error: " : ""}${tool} ${status}`), "summary/error status must survive");
  for (const value of Object.values(sentinel)) {
    assert(visibleText.includes(value), `${tool}: structured fact missing from model-facing content`);
  }
  console.log(JSON.stringify({ adapterVersion: version, syntheticWire: JSON.parse(wire), decoded, modelFacingContent: guarded.content }));
}

const structured = { result: "fixture-essential-fact" };
assert.match(resolveMcpResultContent({ content: [], structuredContent: structured })[0].text, /fixture-essential-fact/);
const mixed = resolveMcpResultContent({
  content: [
    { type: "image", data: "AA==", mimeType: "image/png" },
    { type: "resource_link", name: "fixture", uri: "krometrail://fixture/artifact" },
  ],
  structuredContent: structured,
});
// AA== is a byte-preservation sentinel, not a screenshot or image-decoding test.
assert.deepEqual(mixed[0], { type: "image", data: "AA==", mimeType: "image/png" });
assert(mixed.some((block) => block.type === "text" && block.text === "[Resource Link: fixture]\nURI: krometrail://fixture/artifact"));
assert(mixed.some((block) => block.type === "text" && block.text.includes("fixture-essential-fact")));
const serialized = JSON.stringify(structured);
assert.deepEqual(resolveMcpResultContent({ content: [{ type: "text", text: serialized }], structuredContent: structured }), [{ type: "text", text: serialized }]);
console.log("Installed resolver regression passed: 6 structured-fact cases, empty-content fallback, native image/resource-link preservation, and whole-JSON deduplication. Synthetic evidence only; not full host/browser qualification.");

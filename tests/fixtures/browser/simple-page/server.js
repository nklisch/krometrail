/**
 * Simple Bun HTTP server for browser recording integration tests.
 * Serves the test page and provides API endpoints.
 *
 * Usage: bun tests/fixtures/browser/simple-page/server.js [port]
 */

import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const port = Number.parseInt(process.argv[2] ?? "8765", 10);

const server = Bun.serve({
	port,
	fetch(req) {
		const url = new URL(req.url);

		if (url.pathname === "/" || url.pathname === "/index.html") {
			const html = readFileSync(join(__dirname, "index.html"), "utf8");
			return new Response(html, { headers: { "Content-Type": "text/html" } });
		}

		if (url.pathname === "/app.js") {
			const js = readFileSync(join(__dirname, "app.js"), "utf8");
			return new Response(js, { headers: { "Content-Type": "application/javascript" } });
		}

		if (url.pathname === "/api/data" && req.method === "GET") {
			return Response.json({ items: [{ id: 1, name: "Item A" }, { id: 2, name: "Item B" }] });
		}

		if (url.pathname === "/api/submit" && req.method === "POST") {
			return req.json().then((body) => {
				const username = body?.username ?? "";
				if (!username) {
					return Response.json({ error: "Username is required" }, { status: 422 });
				}
				return Response.json({ success: true, user: username });
			});
		}

		return new Response("Not Found", { status: 404 });
	},
});

// Signal readiness on stdout
process.stdout.write(`READY:${port}\n`);

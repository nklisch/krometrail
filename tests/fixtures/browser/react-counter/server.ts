/**
 * Minimal React counter app for testing framework state observation.
 * Serves a single page with:
 * - A Counter component using useState
 * - A CountDisplay child component receiving count as props
 * - Buttons to increment/decrement
 * - A ResetButton component to test unmount/remount
 *
 * Usage: bun run tests/fixtures/browser/react-counter/server.ts <port>
 * Prints "READY:<port>" to stdout when listening.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const port = Number.parseInt(process.argv[2] ?? "0", 10);

const server = Bun.serve({
	port,
	async fetch(req) {
		const url = new URL(req.url);

		if (url.pathname === "/" || url.pathname === "/index.html") {
			const html = readFileSync(join(__dirname, "index.html"), "utf-8");
			return new Response(html, { headers: { "Content-Type": "text/html" } });
		}

		if (url.pathname === "/vendor/react.development.js") {
			const file = Bun.file(join(__dirname, "node_modules/react/umd/react.development.js"));
			return new Response(file, { headers: { "Content-Type": "application/javascript" } });
		}

		if (url.pathname === "/vendor/react-dom.development.js") {
			const file = Bun.file(join(__dirname, "node_modules/react-dom/umd/react-dom.development.js"));
			return new Response(file, { headers: { "Content-Type": "application/javascript" } });
		}

		return new Response("Not found", { status: 404 });
	},
});

process.stdout.write(`READY:${server.port}\n`);

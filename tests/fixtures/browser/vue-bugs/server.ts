/**
 * Vue 3 bug demo app for testing pattern detection.
 * Contains InfiniteWatcher (watcher_infinite_loop) and LostReactivity (lost_reactivity) components.
 *
 * Usage: bun run tests/fixtures/browser/vue-bugs/server.ts <port>
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

		return new Response("Not found", { status: 404 });
	},
});

process.stdout.write(`READY:${server.port}\n`);

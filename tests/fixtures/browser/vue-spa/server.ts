import { existsSync } from "node:fs";
import { dirname, join } from "node:path";

const __dirname = dirname(new URL(import.meta.url).pathname);
const port = Number.parseInt(process.argv[2] ?? "0", 10);

// Build on first run
if (!existsSync(join(__dirname, "dist"))) {
	const result = Bun.spawnSync(["bunx", "vite", "build"], { cwd: __dirname, stdout: "pipe", stderr: "pipe" });
	if (result.exitCode !== 0) {
		throw new Error(`Vite build failed: ${new TextDecoder().decode(result.stderr)}`);
	}
}

// Test control state
let failCreate = false;
let failSettings = false;
let apiDelayMs = 0;

let nextTaskId = 9;

const TASKS = [
	{ id: 1, title: "Set up CI pipeline", description: "Configure GitHub Actions for automated testing", status: "done", priority: "high", assignee: "alice", dueDate: "2026-03-01", comments: [] },
	{ id: 2, title: "Write unit tests", description: "Add coverage for core modules", status: "in-progress", priority: "high", assignee: "bob", dueDate: "2026-03-15", comments: [] },
	{ id: 3, title: "Update README", description: "Document new features and API changes", status: "todo", priority: "medium", assignee: "charlie", dueDate: "2026-03-20", comments: [] },
	{ id: 4, title: "Fix login bug", description: "Session not persisting after refresh", status: "todo", priority: "high", assignee: "alice", dueDate: "2026-03-10", comments: [] },
	{ id: 5, title: "Add dark mode", description: "Implement theme switching", status: "todo", priority: "low", assignee: "bob", dueDate: "2026-04-01", comments: [] },
	{ id: 6, title: "Optimize queries", description: "Slow dashboard queries need indexing", status: "in-progress", priority: "medium", assignee: "charlie", dueDate: "2026-03-18", comments: [] },
	{ id: 7, title: "Code review backlog", description: "Review pending PRs", status: "todo", priority: "medium", assignee: "alice", dueDate: "2026-03-12", comments: [] },
	{ id: 8, title: "Deploy to staging", description: "Push latest build to staging environment", status: "done", priority: "high", assignee: "bob", dueDate: "2026-03-05", comments: [] },
];

let taskData = TASKS.map((t) => ({ ...t, comments: [] as Array<{ id: number; text: string; author: string; createdAt: string }> }));

function jsonResponse(data: unknown, status = 200) {
	return new Response(JSON.stringify(data), {
		status,
		headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*" },
	});
}

async function maybeDelay() {
	if (apiDelayMs > 0) {
		await new Promise<void>((r) => setTimeout(r, apiDelayMs));
	}
}

const server = Bun.serve({
	port,
	async fetch(req) {
		const url = new URL(req.url);

		// Test control routes
		if (url.pathname === "/__test__/fail-create") {
			failCreate = true;
			return jsonResponse({ ok: true });
		}
		if (url.pathname === "/__test__/fail-settings") {
			failSettings = true;
			return jsonResponse({ ok: true });
		}
		if (url.pathname === "/__test__/slow-api") {
			apiDelayMs = Number.parseInt(url.searchParams.get("ms") ?? "2000", 10);
			return jsonResponse({ ok: true });
		}
		if (url.pathname === "/__test__/reset") {
			failCreate = false;
			failSettings = false;
			apiDelayMs = 0;
			taskData = TASKS.map((t) => ({ ...t, comments: [] }));
			nextTaskId = 9;
			return jsonResponse({ ok: true });
		}

		// CORS preflight
		if (req.method === "OPTIONS") {
			return new Response(null, { headers: { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Methods": "GET,POST,PATCH,DELETE,PUT,OPTIONS", "Access-Control-Allow-Headers": "Content-Type" } });
		}

		// API routes
		if (url.pathname === "/api/tasks" && req.method === "GET") {
			await maybeDelay();
			return jsonResponse(taskData);
		}

		const taskMatch = url.pathname.match(/^\/api\/tasks\/(\d+)$/);
		const commentMatch = url.pathname.match(/^\/api\/tasks\/(\d+)\/comments$/);

		if (taskMatch && req.method === "GET") {
			await maybeDelay();
			const task = taskData.find((t) => t.id === Number(taskMatch[1]));
			if (!task) return jsonResponse({ error: "Not found" }, 404);
			return jsonResponse(task);
		}

		if (url.pathname === "/api/tasks" && req.method === "POST") {
			await maybeDelay();
			if (failCreate) {
				failCreate = false;
				return jsonResponse({ message: "Validation failed", errors: { title: "Title is required" } }, 422);
			}
			const body = await req.json().catch(() => ({}));
			const task = { id: nextTaskId++, comments: [], ...body };
			taskData.push(task);
			return jsonResponse(task, 201);
		}

		if (taskMatch && req.method === "PATCH") {
			await maybeDelay();
			const id = Number(taskMatch[1]);
			const idx = taskData.findIndex((t) => t.id === id);
			if (idx < 0) return jsonResponse({ error: "Not found" }, 404);
			const body = await req.json().catch(() => ({}));
			taskData[idx] = { ...taskData[idx], ...body };
			return jsonResponse(taskData[idx]);
		}

		if (taskMatch && req.method === "DELETE") {
			const id = Number(taskMatch[1]);
			taskData = taskData.filter((t) => t.id !== id);
			return new Response(null, { status: 204 });
		}

		if (commentMatch && req.method === "POST") {
			await maybeDelay();
			const taskId = Number(commentMatch[1]);
			const task = taskData.find((t) => t.id === taskId);
			if (!task) return jsonResponse({ error: "Not found" }, 404);
			const body = await req.json().catch(() => ({}));
			const comment = { id: Date.now(), createdAt: new Date().toISOString(), ...body };
			task.comments.push(comment);
			return jsonResponse(comment, 201);
		}

		if (url.pathname === "/api/login" && req.method === "POST") {
			await maybeDelay();
			const body = await req.json().catch(() => ({}));
			if (body.username === "admin" && body.password === "secret") {
				return jsonResponse({ token: "tok_vue_123", user: { id: 1, name: "Admin User" } });
			}
			return jsonResponse({ error: "Invalid credentials" }, 401);
		}

		if (url.pathname === "/api/settings" && req.method === "PUT") {
			await maybeDelay();
			if (failSettings) {
				failSettings = false;
				return jsonResponse({ message: "Validation failed", errors: { email: "Invalid email" } }, 422);
			}
			const body = await req.json().catch(() => ({}));
			return jsonResponse({ ...body, savedAt: new Date().toISOString() });
		}

		// Static files
		const filePath = join(__dirname, "dist", url.pathname === "/" ? "index.html" : url.pathname);
		if (existsSync(filePath)) {
			return new Response(Bun.file(filePath));
		}

		// SPA fallback
		return new Response(Bun.file(join(__dirname, "dist/index.html")), {
			headers: { "Content-Type": "text/html" },
		});
	},
});

process.stdout.write(`READY:${server.port}\n`);

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
let failCheckout = false;
let errorCheckout = false;
let apiDelayMs = 0;

const PRODUCTS = [
	{ id: 1, name: "Widget Alpha", price: 9.99, image: "/img/1.jpg" },
	{ id: 2, name: "Gadget Beta", price: 24.99, image: "/img/2.jpg" },
	{ id: 3, name: "Gizmo Gamma", price: 14.99, image: "/img/3.jpg" },
	{ id: 4, name: "Device Delta", price: 39.99, image: "/img/4.jpg" },
	{ id: 5, name: "Tool Epsilon", price: 7.99, image: "/img/5.jpg" },
	{ id: 6, name: "Module Zeta", price: 19.99, image: "/img/6.jpg" },
];

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
		if (url.pathname === "/__test__/fail-checkout") {
			failCheckout = true;
			return jsonResponse({ ok: true });
		}
		if (url.pathname === "/__test__/error-checkout") {
			errorCheckout = true;
			return jsonResponse({ ok: true });
		}
		if (url.pathname === "/__test__/slow-api") {
			apiDelayMs = Number.parseInt(url.searchParams.get("ms") ?? "2000", 10);
			return jsonResponse({ ok: true });
		}
		if (url.pathname === "/__test__/reset") {
			failCheckout = false;
			errorCheckout = false;
			apiDelayMs = 0;
			return jsonResponse({ ok: true });
		}

		// API routes
		if (url.pathname === "/api/products" && req.method === "GET") {
			await maybeDelay();
			return jsonResponse(PRODUCTS);
		}

		if (url.pathname.startsWith("/api/products/") && req.method === "GET") {
			await maybeDelay();
			const id = Number.parseInt(url.pathname.split("/").pop() ?? "0", 10);
			const product = PRODUCTS.find((p) => p.id === id);
			if (!product) return jsonResponse({ error: "Not found" }, 404);
			return jsonResponse({
				...product,
				description: `A quality ${product.name} for all your needs.`,
				reviews: [
					{ id: 1, author: "Alice", rating: 5, text: "Great product!" },
					{ id: 2, author: "Bob", rating: 4, text: "Works well." },
				],
			});
		}

		if (url.pathname === "/api/login" && req.method === "POST") {
			await maybeDelay();
			const body = await req.json().catch(() => ({}));
			if (body.username === "admin" && body.password === "secret") {
				return jsonResponse({ token: "tok_admin_123", user: { id: 1, name: "Admin User" } });
			}
			return jsonResponse({ error: "Invalid credentials" }, 401);
		}

		if (url.pathname === "/api/checkout" && req.method === "POST") {
			await maybeDelay();
			if (errorCheckout) {
				errorCheckout = false;
				return jsonResponse({ error: "Internal server error" }, 500);
			}
			if (failCheckout) {
				failCheckout = false;
				return jsonResponse({ errors: { address: "Address is required", zip: "Invalid ZIP" } }, 422);
			}
			return jsonResponse({ orderId: `ORD-${Date.now()}` });
		}

		// CORS preflight
		if (req.method === "OPTIONS") {
			return new Response(null, { headers: { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Methods": "GET,POST,OPTIONS", "Access-Control-Allow-Headers": "Content-Type" } });
		}

		// Static file serving from dist/
		const filePath = join(__dirname, "dist", url.pathname === "/" ? "index.html" : url.pathname);
		if (existsSync(filePath)) {
			return new Response(Bun.file(filePath));
		}

		// SPA fallback for client-side routes
		return new Response(Bun.file(join(__dirname, "dist/index.html")), {
			headers: { "Content-Type": "text/html" },
		});
	},
});

process.stdout.write(`READY:${server.port}\n`);

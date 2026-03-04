import { describe, expect, it } from "vitest";
import { renderViewport } from "../../../src/core/viewport.js";
import type { ViewportConfig, ViewportSnapshot } from "../../../src/core/types.js";

const defaultConfig: ViewportConfig = {
	sourceContextLines: 15,
	stackDepth: 5,
	localsMaxDepth: 1,
	localsMaxItems: 20,
	stringTruncateLength: 120,
	collectionPreviewItems: 5,
};

describe("renderViewport", () => {
	it("renders a basic viewport snapshot", () => {
		const snapshot: ViewportSnapshot = {
			file: "order.py",
			line: 147,
			function: "process_order",
			reason: "breakpoint",
			totalFrames: 3,
			stack: [
				{
					file: "app/services/order.py",
					shortFile: "order.py",
					line: 147,
					function: "process_order",
					arguments: "cart=<Cart>, user=<User:482>",
				},
				{
					file: "app/router.py",
					shortFile: "router.py",
					line: 83,
					function: "handle_request",
					arguments: "req=<Request>",
				},
			],
			source: [
				{ line: 146, text: "  total = subtotal - discount + tax" },
				{ line: 147, text: "  charge_result = payment.charge(user.card, total)" },
				{ line: 148, text: "" },
			],
			locals: [
				{ name: "subtotal", value: "149.97" },
				{ name: "discount", value: "-149.97" },
				{ name: "total", value: "314.937" },
			],
		};

		const output = renderViewport(snapshot, defaultConfig);

		expect(output).toContain("STOPPED at order.py:147 (process_order)");
		expect(output).toContain("Reason: breakpoint");
		expect(output).toContain("Call Stack (2 of 3 frames)");
		expect(output).toContain("subtotal");
		expect(output).toContain("-149.97");
		expect(output).toMatch(/→\s*147│/);
	});

	it("renders watch expressions when present", () => {
		const snapshot: ViewportSnapshot = {
			file: "test.py",
			line: 10,
			function: "main",
			reason: "step",
			totalFrames: 1,
			stack: [
				{ file: "test.py", shortFile: "test.py", line: 10, function: "main", arguments: "" },
			],
			source: [{ line: 10, text: "  x = 42" }],
			locals: [{ name: "x", value: "42" }],
			watches: [
				{ name: "x > 0", value: "True" },
				{ name: "x * 2", value: "84" },
			],
		};

		const output = renderViewport(snapshot, defaultConfig);

		expect(output).toContain("Watch:");
		expect(output).toContain("x > 0");
		expect(output).toContain("True");
	});
});

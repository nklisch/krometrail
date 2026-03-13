import { describe, expect, it } from "vitest";
import type { ViewportConfig, ViewportSnapshot } from "../../../src/core/types.js";
import { computeViewportDiff, isDiffEligible, renderViewport, renderViewportDiff } from "../../../src/core/viewport.js";

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
			stack: [{ file: "test.py", shortFile: "test.py", line: 10, function: "main", arguments: "" }],
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

	it("renders thread indicator when thread field is present", () => {
		const snapshot: ViewportSnapshot = {
			file: "app.py",
			line: 10,
			function: "worker",
			reason: "breakpoint",
			totalFrames: 1,
			stack: [{ file: "app.py", shortFile: "app.py", line: 10, function: "worker", arguments: "" }],
			source: [{ line: 10, text: "  x = 1" }],
			locals: [],
			thread: { id: 2, name: "worker-1", totalThreads: 3 },
		};
		const output = renderViewport(snapshot, defaultConfig);
		expect(output).toContain("[worker-1 (2/3)]");
	});

	it("renders clean header (no thread indicator) for single-threaded snapshot", () => {
		const snapshot: ViewportSnapshot = {
			file: "app.py",
			line: 10,
			function: "main",
			reason: "breakpoint",
			totalFrames: 1,
			stack: [{ file: "app.py", shortFile: "app.py", line: 10, function: "main", arguments: "" }],
			source: [{ line: 10, text: "  x = 1" }],
			locals: [],
		};
		const output = renderViewport(snapshot, defaultConfig);
		expect(output).not.toContain("[");
		expect(output).toContain("── STOPPED at app.py:10 (main) ──");
	});

	it("renders exception info in viewport when exception field is present", () => {
		const snapshot: ViewportSnapshot = {
			file: "app.py",
			line: 42,
			function: "process",
			reason: "exception",
			totalFrames: 1,
			stack: [{ file: "app.py", shortFile: "app.py", line: 42, function: "process", arguments: "" }],
			source: [{ line: 42, text: "  raise ValueError('bad input')" }],
			locals: [],
			exception: { type: "ValueError", message: "bad input" },
		};
		const output = renderViewport(snapshot, defaultConfig);
		expect(output).toContain("Exception: ValueError: bad input");
	});

	it("appends compression note at end of viewport", () => {
		const snapshot: ViewportSnapshot = {
			file: "test.py",
			line: 10,
			function: "main",
			reason: "step",
			totalFrames: 1,
			stack: [{ file: "test.py", shortFile: "test.py", line: 10, function: "main", arguments: "" }],
			source: [{ line: 10, text: "  x = 42" }],
			locals: [{ name: "x", value: "42" }],
			compressionNote: "(compressed: action 25/200, use debug_variables for full locals)",
		};

		const output = renderViewport(snapshot, defaultConfig);

		expect(output).toContain("(compressed: action 25/200");
	});
});

describe("isDiffEligible", () => {
	const makeSnapshot = (file: string, fn: string, stackLen: number): ViewportSnapshot => ({
		file,
		line: 10,
		function: fn,
		reason: "step",
		totalFrames: stackLen,
		stack: Array.from({ length: stackLen }, (_, i) => ({ file, shortFile: file, line: 10 + i, function: fn, arguments: "" })),
		source: [{ line: 10, text: "x = 1" }],
		locals: [],
	});

	it("returns true for same file, function, and stack depth", () => {
		const a = makeSnapshot("test.py", "main", 2);
		const b = makeSnapshot("test.py", "main", 2);
		expect(isDiffEligible(a, b)).toBe(true);
	});

	it("returns false when file differs", () => {
		const a = makeSnapshot("test.py", "main", 2);
		const b = makeSnapshot("other.py", "main", 2);
		expect(isDiffEligible(a, b)).toBe(false);
	});

	it("returns false when function differs", () => {
		const a = makeSnapshot("test.py", "main", 2);
		const b = makeSnapshot("test.py", "other", 2);
		expect(isDiffEligible(a, b)).toBe(false);
	});

	it("returns false when stack depth differs", () => {
		const a = makeSnapshot("test.py", "main", 2);
		const b = makeSnapshot("test.py", "main", 3);
		expect(isDiffEligible(a, b)).toBe(false);
	});
});

describe("computeViewportDiff", () => {
	const makeSnapshot = (line: number, locals: Array<{ name: string; value: string }>, source?: Array<{ line: number; text: string }>): ViewportSnapshot => ({
		file: "order.py",
		line,
		function: "process_order",
		reason: "step",
		totalFrames: 1,
		stack: [{ file: "order.py", shortFile: "order.py", line, function: "process_order", arguments: "" }],
		source: source ?? [{ line, text: `  line ${line}` }],
		locals,
	});

	it("identifies changed variables", () => {
		const prev = makeSnapshot(10, [{ name: "x", value: "1" }]);
		const curr = makeSnapshot(11, [{ name: "x", value: "2" }]);
		const diff = computeViewportDiff(curr, prev);
		expect(diff.changedVariables).toHaveLength(1);
		expect(diff.changedVariables[0]).toMatchObject({ name: "x", oldValue: "1", newValue: "2" });
	});

	it("identifies added variables as changes", () => {
		const prev = makeSnapshot(10, []);
		const curr = makeSnapshot(11, [{ name: "y", value: "5" }]);
		const diff = computeViewportDiff(curr, prev);
		expect(diff.changedVariables).toHaveLength(1);
		expect(diff.changedVariables[0].name).toBe("y");
	});

	it("counts unchanged variables", () => {
		const prev = makeSnapshot(10, [
			{ name: "x", value: "1" },
			{ name: "y", value: "2" },
		]);
		const curr = makeSnapshot(11, [
			{ name: "x", value: "1" },
			{ name: "y", value: "9" },
		]);
		const diff = computeViewportDiff(curr, prev);
		expect(diff.unchangedCount).toBe(1);
		expect(diff.changedVariables).toHaveLength(1);
	});

	it("omits source when line is within previous source window", () => {
		const prevSource = [
			{ line: 8, text: "a" },
			{ line: 9, text: "b" },
			{ line: 10, text: "c" },
			{ line: 11, text: "d" },
			{ line: 12, text: "e" },
		];
		const prev = makeSnapshot(10, [], prevSource);
		const curr = makeSnapshot(11, [], [{ line: 11, text: "d" }]);
		const diff = computeViewportDiff(curr, prev);
		expect(diff.source).toBeUndefined();
	});

	it("includes source when line moved outside previous window", () => {
		const prevSource = [
			{ line: 5, text: "a" },
			{ line: 6, text: "b" },
		];
		const newSource = [{ line: 50, text: "x" }];
		const prev = makeSnapshot(5, [], prevSource);
		const curr = makeSnapshot(50, [], newSource);
		const diff = computeViewportDiff(curr, prev);
		expect(diff.source).toBeDefined();
	});

	it("includes watches in full", () => {
		const prev = makeSnapshot(10, []);
		const curr = { ...makeSnapshot(11, []), watches: [{ name: "x > 0", value: "True" }] };
		const diff = computeViewportDiff(curr, prev);
		expect(diff.watches).toBeDefined();
		expect(diff.watches?.[0].name).toBe("x > 0");
	});

	it("includes compression note when provided", () => {
		const prev = makeSnapshot(10, []);
		const curr = makeSnapshot(11, []);
		const diff = computeViewportDiff(curr, prev, "(compressed)");
		expect(diff.compressionNote).toBe("(compressed)");
	});
});

describe("renderViewport — locals truncation", () => {
	it("shows '(N more...)' when locals exceed localsMaxItems", () => {
		const locals = Array.from({ length: 25 }, (_, i) => ({ name: `var${i}`, value: `${i}` }));
		const snapshot: ViewportSnapshot = {
			file: "test.py",
			line: 10,
			function: "main",
			reason: "step",
			totalFrames: 1,
			stack: [{ file: "test.py", shortFile: "test.py", line: 10, function: "main", arguments: "" }],
			source: [{ line: 10, text: "  x = 1" }],
			locals,
		};
		const config = { ...defaultConfig, localsMaxItems: 5 };
		const output = renderViewport(snapshot, config);
		expect(output).toContain("var0");
		expect(output).toContain("var4");
		expect(output).not.toContain("var5");
		expect(output).toContain("(20 more...)");
	});

	it("shows no truncation message when locals fit within limit", () => {
		const snapshot: ViewportSnapshot = {
			file: "test.py",
			line: 10,
			function: "main",
			reason: "step",
			totalFrames: 1,
			stack: [{ file: "test.py", shortFile: "test.py", line: 10, function: "main", arguments: "" }],
			source: [{ line: 10, text: "  x = 1" }],
			locals: [{ name: "x", value: "42" }],
		};
		const output = renderViewport(snapshot, defaultConfig);
		expect(output).not.toContain("more...");
	});
});

describe("renderViewport — stack depth display", () => {
	it("shows 'N of M frames' when stack exceeds stackDepth", () => {
		const stack = Array.from({ length: 3 }, (_, i) => ({
			file: `file${i}.py`,
			shortFile: `file${i}.py`,
			line: 10 + i,
			function: `func${i}`,
			arguments: "",
		}));
		const snapshot: ViewportSnapshot = {
			file: "file0.py",
			line: 10,
			function: "func0",
			reason: "breakpoint",
			totalFrames: 10,
			stack,
			source: [{ line: 10, text: "  x = 1" }],
			locals: [],
		};
		const output = renderViewport(snapshot, defaultConfig);
		expect(output).toContain("Call Stack (3 of 10 frames)");
	});
});

describe("renderViewport — stop reasons", () => {
	const reasons: Array<"breakpoint" | "step" | "exception" | "pause" | "entry"> = ["breakpoint", "step", "exception", "pause", "entry"];
	for (const reason of reasons) {
		it(`renders reason '${reason}'`, () => {
			const snapshot: ViewportSnapshot = {
				file: "test.py",
				line: 1,
				function: "main",
				reason,
				totalFrames: 1,
				stack: [{ file: "test.py", shortFile: "test.py", line: 1, function: "main", arguments: "" }],
				source: [{ line: 1, text: "  pass" }],
				locals: [],
			};
			const output = renderViewport(snapshot, defaultConfig);
			expect(output).toContain(`Reason: ${reason}`);
		});
	}
});

describe("renderViewport — exception with exceptionId", () => {
	it("renders exception info including type and message", () => {
		const snapshot: ViewportSnapshot = {
			file: "app.py",
			line: 42,
			function: "process",
			reason: "exception",
			totalFrames: 1,
			stack: [{ file: "app.py", shortFile: "app.py", line: 42, function: "process", arguments: "" }],
			source: [{ line: 42, text: "  raise TypeError('expected int')" }],
			locals: [],
			exception: { type: "TypeError", message: "expected int", exceptionId: "exc-123" },
		};
		const output = renderViewport(snapshot, defaultConfig);
		expect(output).toContain("Exception: TypeError: expected int");
	});
});

describe("computeViewportDiff — removed variables", () => {
	it("does not track removed variables (only current locals are diffed)", () => {
		const prev: ViewportSnapshot = {
			file: "test.py",
			line: 10,
			function: "main",
			reason: "step",
			totalFrames: 1,
			stack: [{ file: "test.py", shortFile: "test.py", line: 10, function: "main", arguments: "" }],
			source: [{ line: 10, text: "  x = 1" }],
			locals: [{ name: "x", value: "1" }, { name: "temp", value: "99" }],
		};
		const curr: ViewportSnapshot = {
			file: "test.py",
			line: 11,
			function: "main",
			reason: "step",
			totalFrames: 1,
			stack: [{ file: "test.py", shortFile: "test.py", line: 11, function: "main", arguments: "" }],
			source: [{ line: 11, text: "  y = 2" }],
			locals: [{ name: "x", value: "1" }],
		};
		const diff = computeViewportDiff(curr, prev);
		// "temp" was removed — it should not appear in changedVariables
		expect(diff.changedVariables.map((v) => v.name)).not.toContain("temp");
		// "x" is unchanged
		expect(diff.unchangedCount).toBe(1);
	});
});

describe("renderViewportDiff", () => {
	it("shows (same frame) in header", () => {
		const diff = {
			isDiff: true as const,
			file: "order.py",
			line: 11,
			function: "process_order",
			reason: "step" as const,
			changedVariables: [{ name: "x", oldValue: "1", newValue: "2" }],
			unchangedCount: 3,
		};
		const output = renderViewportDiff(diff, defaultConfig);
		expect(output).toContain("same frame");
	});

	it("shows unchanged count", () => {
		const diff = {
			isDiff: true as const,
			file: "order.py",
			line: 11,
			function: "process_order",
			reason: "step" as const,
			changedVariables: [],
			unchangedCount: 5,
		};
		const output = renderViewportDiff(diff, defaultConfig);
		expect(output).toContain("5 locals unchanged");
	});

	it("renders watch expressions", () => {
		const diff = {
			isDiff: true as const,
			file: "order.py",
			line: 11,
			function: "process_order",
			reason: "step" as const,
			changedVariables: [],
			unchangedCount: 0,
			watches: [{ name: "total > 0", value: "True" }],
		};
		const output = renderViewportDiff(diff, defaultConfig);
		expect(output).toContain("Watch:");
		expect(output).toContain("total > 0");
	});

	it("shows compression note", () => {
		const diff = {
			isDiff: true as const,
			file: "order.py",
			line: 11,
			function: "process_order",
			reason: "step" as const,
			changedVariables: [],
			unchangedCount: 0,
			compressionNote: "(compressed: action 55/200)",
		};
		const output = renderViewportDiff(diff, defaultConfig);
		expect(output).toContain("(compressed: action 55/200)");
	});

	it("produces fewer tokens than full viewport for same-frame step", () => {
		const snapshot: ViewportSnapshot = {
			file: "order.py",
			line: 11,
			function: "process_order",
			reason: "step",
			totalFrames: 2,
			stack: [
				{ file: "order.py", shortFile: "order.py", line: 11, function: "process_order", arguments: "" },
				{ file: "router.py", shortFile: "router.py", line: 83, function: "handle_request", arguments: "" },
			],
			source: [
				{ line: 9, text: "  a = 1" },
				{ line: 10, text: "  b = 2" },
				{ line: 11, text: "  c = 3" },
			],
			locals: [
				{ name: "x", value: "42" },
				{ name: "y", value: "100" },
				{ name: "z", value: "200" },
			],
		};
		const prev = {
			...snapshot,
			line: 10,
			locals: [
				{ name: "x", value: "41" },
				{ name: "y", value: "100" },
				{ name: "z", value: "200" },
			],
		};
		const diff = computeViewportDiff(snapshot, prev);
		const fullOutput = renderViewport(snapshot, defaultConfig);
		const diffOutput = renderViewportDiff(diff, defaultConfig);
		expect(diffOutput.length).toBeLessThan(fullOutput.length);
	});
});

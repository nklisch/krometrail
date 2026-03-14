import { describe, expect, it } from "vitest";
import { EXIT_ERROR, EXIT_NOT_FOUND, EXIT_STATE, EXIT_TIMEOUT, exitCodeFromError } from "../../../src/cli/exit-codes.js";
import {
	AdapterNotFoundError,
	BrowserRecorderStateError,
	DAPTimeoutError,
	KrometrailError,
	SessionLimitError,
	SessionNotFoundError,
	SessionStateError,
	TabNotFoundError,
} from "../../../src/core/errors.js";

describe("exitCodeFromError", () => {
	it("returns EXIT_NOT_FOUND for SessionNotFoundError", () => {
		expect(exitCodeFromError(new SessionNotFoundError("sess-1"))).toBe(EXIT_NOT_FOUND);
		expect(exitCodeFromError(new SessionNotFoundError("sess-1"))).toBe(3);
	});

	it("returns EXIT_NOT_FOUND for AdapterNotFoundError", () => {
		expect(exitCodeFromError(new AdapterNotFoundError("python"))).toBe(EXIT_NOT_FOUND);
	});

	it("returns EXIT_NOT_FOUND for TabNotFoundError", () => {
		expect(exitCodeFromError(new TabNotFoundError("tab-1"))).toBe(EXIT_NOT_FOUND);
	});

	it("returns EXIT_TIMEOUT for DAPTimeoutError", () => {
		expect(exitCodeFromError(new DAPTimeoutError("continue", 30_000))).toBe(EXIT_TIMEOUT);
		expect(exitCodeFromError(new DAPTimeoutError("continue", 30_000))).toBe(4);
	});

	it("returns EXIT_STATE for SessionStateError", () => {
		expect(exitCodeFromError(new SessionStateError("sess-1", "running", ["stopped"]))).toBe(EXIT_STATE);
		expect(exitCodeFromError(new SessionStateError("sess-1", "running", ["stopped"]))).toBe(5);
	});

	it("returns EXIT_STATE for SessionLimitError", () => {
		expect(exitCodeFromError(new SessionLimitError("max_sessions", 5, 5))).toBe(EXIT_STATE);
	});

	it("returns EXIT_STATE for BrowserRecorderStateError", () => {
		expect(exitCodeFromError(new BrowserRecorderStateError("not recording"))).toBe(EXIT_STATE);
	});

	it("returns EXIT_ERROR for generic Error", () => {
		expect(exitCodeFromError(new Error("generic"))).toBe(EXIT_ERROR);
		expect(exitCodeFromError(new Error("generic"))).toBe(1);
	});

	it("returns EXIT_NOT_FOUND for KrometrailError with RPC code -32000 (SESSION_NOT_FOUND)", () => {
		const rpcWrapped = new KrometrailError("Session not found", "-32000");
		expect(exitCodeFromError(rpcWrapped)).toBe(EXIT_NOT_FOUND);
	});

	it("returns EXIT_STATE for KrometrailError with RPC code -32001 (SESSION_STATE_ERROR)", () => {
		const rpcWrapped = new KrometrailError("Session state error", "-32001");
		expect(exitCodeFromError(rpcWrapped)).toBe(EXIT_STATE);
	});

	it("returns EXIT_ERROR for unknown KrometrailError codes", () => {
		const err = new KrometrailError("Adapter failed", "-32003");
		expect(exitCodeFromError(err)).toBe(EXIT_ERROR);
	});

	it("returns EXIT_ERROR for plain objects without known codes", () => {
		expect(exitCodeFromError({ message: "something" })).toBe(EXIT_ERROR);
	});

	it("returns EXIT_ERROR for null/undefined", () => {
		expect(exitCodeFromError(null)).toBe(EXIT_ERROR);
		expect(exitCodeFromError(undefined)).toBe(EXIT_ERROR);
	});
});

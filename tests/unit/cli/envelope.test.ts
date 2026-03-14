import { describe, expect, it } from "vitest";
import { errorEnvelope, RETRYABLE_CODES, successEnvelope } from "../../../src/cli/envelope.js";
import { DAPTimeoutError, KrometrailError, SessionNotFoundError } from "../../../src/core/errors.js";

describe("successEnvelope", () => {
	it("wraps data in { ok: true, data }", () => {
		const out = successEnvelope({ sessionId: "abc" });
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.sessionId).toBe("abc");
	});

	it("serializes nested objects", () => {
		const data = { a: { b: { c: 42 } }, arr: [1, 2, 3] };
		const out = successEnvelope(data);
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.a.b.c).toBe(42);
		expect(parsed.data.arr).toHaveLength(3);
	});

	it("handles primitive values", () => {
		const out = successEnvelope(42);
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data).toBe(42);
	});

	it("handles null", () => {
		const out = successEnvelope(null);
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data).toBeNull();
	});
});

describe("errorEnvelope", () => {
	it("wraps KrometrailError with code and retryable=false for non-retryable errors", () => {
		const err = new SessionNotFoundError("sess-1");
		const out = errorEnvelope(err);
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("SESSION_NOT_FOUND");
		expect(parsed.error.retryable).toBe(false);
	});

	it("marks DAP_TIMEOUT as retryable", () => {
		const err = new DAPTimeoutError("continue", 30_000);
		const out = errorEnvelope(err);
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("DAP_TIMEOUT");
		expect(parsed.error.retryable).toBe(true);
	});

	it("marks SESSION_NOT_FOUND as non-retryable", () => {
		const err = new SessionNotFoundError("sess-1");
		const out = errorEnvelope(err);
		const parsed = JSON.parse(out);
		expect(parsed.error.retryable).toBe(false);
	});

	it("handles generic Error with UNKNOWN_ERROR code", () => {
		const err = new Error("something went wrong");
		const out = errorEnvelope(err);
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("UNKNOWN_ERROR");
		expect(parsed.error.message).toBe("something went wrong");
		expect(parsed.error.retryable).toBe(false);
	});

	it("includes error message in envelope", () => {
		const err = new KrometrailError("specific error message", "SOME_CODE");
		const out = errorEnvelope(err);
		const parsed = JSON.parse(out);
		expect(parsed.error.message).toBe("specific error message");
	});

	it("handles non-Error values", () => {
		const out = errorEnvelope("string error");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("UNKNOWN_ERROR");
	});
});

describe("RETRYABLE_CODES", () => {
	it("includes DAP_TIMEOUT", () => {
		expect(RETRYABLE_CODES.has("DAP_TIMEOUT")).toBe(true);
	});

	it("includes DAP_CONNECTION_FAILED", () => {
		expect(RETRYABLE_CODES.has("DAP_CONNECTION_FAILED")).toBe(true);
	});

	it("includes CDP_CONNECTION_FAILED", () => {
		expect(RETRYABLE_CODES.has("CDP_CONNECTION_FAILED")).toBe(true);
	});

	it("does not include SESSION_NOT_FOUND", () => {
		expect(RETRYABLE_CODES.has("SESSION_NOT_FOUND")).toBe(false);
	});

	it("does not include ADAPTER_PREREQUISITES", () => {
		expect(RETRYABLE_CODES.has("ADAPTER_PREREQUISITES")).toBe(false);
	});
});

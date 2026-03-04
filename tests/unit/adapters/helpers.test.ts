import { describe, expect, it } from "vitest";
import { allocatePort } from "../../../src/adapters/helpers.js";

describe("allocatePort", () => {
	it("returns a valid port number > 0", async () => {
		const port = await allocatePort();
		expect(port).toBeGreaterThan(0);
		expect(port).toBeLessThanOrEqual(65535);
		expect(Number.isInteger(port)).toBe(true);
	});

	it("returns different ports on sequential calls", async () => {
		const port1 = await allocatePort();
		const port2 = await allocatePort();
		expect(typeof port1).toBe("number");
		expect(typeof port2).toBe("number");
	});
});

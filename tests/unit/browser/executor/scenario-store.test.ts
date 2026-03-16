import { beforeEach, describe, expect, it } from "vitest";
import { ScenarioStore } from "../../../../src/browser/executor/scenario-store.js";
import type { Step } from "../../../../src/browser/executor/types.js";

function makeStep(action = "navigate", url = "/login"): Step {
	return { action: "navigate", url } as Step;
}

describe("ScenarioStore", () => {
	let store: ScenarioStore;

	beforeEach(() => {
		store = new ScenarioStore();
	});

	it("save + get round-trips correctly", () => {
		const steps: Step[] = [makeStep(), { action: "click", selector: "#btn" }];
		store.save("login", steps);
		const scenario = store.get("login");
		expect(scenario).toBeDefined();
		expect(scenario?.name).toBe("login");
		expect(scenario?.steps).toEqual(steps);
		expect(scenario?.savedAt).toBeTypeOf("number");
	});

	it("list returns all scenarios", () => {
		store.save("flow-a", [makeStep()]);
		store.save("flow-b", [makeStep("reload")]);
		const list = store.list();
		expect(list).toHaveLength(2);
		expect(list.map((s) => s.name)).toContain("flow-a");
		expect(list.map((s) => s.name)).toContain("flow-b");
	});

	it("delete removes a named scenario", () => {
		store.save("to-delete", [makeStep()]);
		const deleted = store.delete("to-delete");
		expect(deleted).toBe(true);
		expect(store.get("to-delete")).toBeUndefined();
	});

	it("delete returns false for non-existent scenario", () => {
		expect(store.delete("not-there")).toBe(false);
	});

	it("clear empties the store", () => {
		store.save("a", [makeStep()]);
		store.save("b", [makeStep()]);
		store.clear();
		expect(store.list()).toHaveLength(0);
	});

	it("overwrite existing scenario", () => {
		const original: Step[] = [makeStep()];
		const updated: Step[] = [makeStep(), { action: "click", selector: "#x" }];
		store.save("flow", original);
		store.save("flow", updated);
		const scenario = store.get("flow");
		expect(scenario?.steps).toEqual(updated);
	});

	it("returns undefined for missing scenario", () => {
		expect(store.get("missing")).toBeUndefined();
	});
});

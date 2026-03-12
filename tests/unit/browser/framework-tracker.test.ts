import { describe, expect, it } from "vitest";
import { FrameworkTracker } from "../../../src/browser/recorder/framework/index.js";

describe("FrameworkTracker", () => {
	describe("constructor normalization", () => {
		it("undefined → disabled", () => {
			const tracker = new FrameworkTracker(undefined);
			expect(tracker.isEnabled()).toBe(false);
		});

		it("false → disabled", () => {
			const tracker = new FrameworkTracker(false);
			expect(tracker.isEnabled()).toBe(false);
		});

		it("true → all frameworks enabled", () => {
			const tracker = new FrameworkTracker(true);
			expect(tracker.isEnabled()).toBe(true);
		});

		it('["react"] → only react enabled', () => {
			const tracker = new FrameworkTracker(["react"]);
			expect(tracker.isEnabled()).toBe(true);
		});

		it('["react", "vue"] → both enabled', () => {
			const tracker = new FrameworkTracker(["react", "vue"]);
			expect(tracker.isEnabled()).toBe(true);
		});
	});

	describe("isEnabled", () => {
		it("returns false when disabled", () => {
			expect(new FrameworkTracker(undefined).isEnabled()).toBe(false);
			expect(new FrameworkTracker(false).isEnabled()).toBe(false);
		});

		it("returns true when enabled", () => {
			expect(new FrameworkTracker(true).isEnabled()).toBe(true);
			expect(new FrameworkTracker(["react"]).isEnabled()).toBe(true);
		});
	});

	describe("getInjectionScripts", () => {
		it("returns empty array when disabled", () => {
			expect(new FrameworkTracker(undefined).getInjectionScripts()).toEqual([]);
			expect(new FrameworkTracker(false).getInjectionScripts()).toEqual([]);
		});

		it("returns 2 scripts when react is enabled (detection + observer)", () => {
			const scripts = new FrameworkTracker(["react"]).getInjectionScripts();
			expect(scripts).toHaveLength(2);
			expect(typeof scripts[0]).toBe("string");
			expect(typeof scripts[1]).toBe("string");
		});

		it("first script is detection, second is react observer", () => {
			const scripts = new FrameworkTracker(["react"]).getInjectionScripts();
			expect(scripts[0]).toContain("framework_detect");
			expect(scripts[1]).toContain("onCommitFiberRoot");
		});

		it("returns 1 script when only vue is enabled (no observer yet)", () => {
			const scripts = new FrameworkTracker(["vue"]).getInjectionScripts();
			expect(scripts).toHaveLength(1);
		});

		it("returns 1 script when only solid is enabled (no observer yet)", () => {
			const scripts = new FrameworkTracker(["solid"]).getInjectionScripts();
			expect(scripts).toHaveLength(1);
		});

		it("returns 2 scripts when true (all frameworks — react observer included)", () => {
			const scripts = new FrameworkTracker(true).getInjectionScripts();
			expect(scripts).toHaveLength(2);
			expect(scripts[0]).toContain("framework_detect");
			expect(scripts[1]).toContain("onCommitFiberRoot");
		});

		it("detection script is always at index 0", () => {
			const scripts = new FrameworkTracker(["react"]).getInjectionScripts();
			expect(scripts[0]).toContain("__REACT_DEVTOOLS_GLOBAL_HOOK__");
		});
	});

	describe("processFrameworkEvent", () => {
		const tracker = new FrameworkTracker(true);
		const tabId = "tab-123";

		it("parses valid framework_detect JSON", () => {
			const json = JSON.stringify({
				type: "framework_detect",
				ts: 1000,
				data: { framework: "react", version: "18.2.0", rootCount: 1, componentCount: 0 },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event).not.toBeNull();
			expect(event?.type).toBe("framework_detect");
			expect(event?.timestamp).toBe(1000);
			expect(event?.tabId).toBe(tabId);
			expect(event?.data.framework).toBe("react");
		});

		it("parses valid framework_state JSON", () => {
			const json = JSON.stringify({
				type: "framework_state",
				ts: 2000,
				data: { framework: "react", componentName: "Counter", changeType: "update", renderCount: 3 },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event).not.toBeNull();
			expect(event?.type).toBe("framework_state");
			expect(event?.data.componentName).toBe("Counter");
		});

		it("parses valid framework_error JSON", () => {
			const json = JSON.stringify({
				type: "framework_error",
				ts: 3000,
				data: { framework: "react", pattern: "infinite_rerender", componentName: "Counter", severity: "high", detail: "too many", evidence: {} },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event).not.toBeNull();
			expect(event?.type).toBe("framework_error");
			expect(event?.data.severity).toBe("high");
		});

		it("returns null for invalid JSON", () => {
			expect(tracker.processFrameworkEvent("not-json", tabId)).toBeNull();
			expect(tracker.processFrameworkEvent("{bad", tabId)).toBeNull();
		});

		it("returns null for non-framework type", () => {
			const json = JSON.stringify({ type: "user_input", ts: 1000, data: {} });
			expect(tracker.processFrameworkEvent(json, tabId)).toBeNull();
		});

		it("returns null for missing ts field", () => {
			const json = JSON.stringify({ type: "framework_detect", data: { framework: "react" } });
			expect(tracker.processFrameworkEvent(json, tabId)).toBeNull();
		});

		it("returns null for missing data field", () => {
			const json = JSON.stringify({ type: "framework_detect", ts: 1000 });
			expect(tracker.processFrameworkEvent(json, tabId)).toBeNull();
		});

		it("generates correct summary for detect event", () => {
			const json = JSON.stringify({
				type: "framework_detect",
				ts: 1000,
				data: { framework: "react", version: "18.2.0", rootCount: 1, componentCount: 0 },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event?.summary).toBe("[react] React 18.2.0 detected (1 root)");
		});

		it("generates correct summary for state event", () => {
			const json = JSON.stringify({
				type: "framework_state",
				ts: 2000,
				data: { framework: "react", componentName: "UserProfile", changeType: "update", renderCount: 3 },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event?.summary).toBe("[react] UserProfile: update (render #3)");
		});

		it("generates correct summary for error event with severity", () => {
			const json = JSON.stringify({
				type: "framework_error",
				ts: 3000,
				data: { framework: "react", pattern: "infinite_rerender", componentName: "Counter", severity: "high", detail: "too many renders", evidence: {} },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event?.summary).toBe("[react:high] infinite_rerender in Counter");
		});

		it("uses crypto.randomUUID() for event id", () => {
			const json = JSON.stringify({
				type: "framework_detect",
				ts: 1000,
				data: { framework: "react", version: "18.2.0", rootCount: 1, componentCount: 0 },
			});
			const event1 = tracker.processFrameworkEvent(json, tabId);
			const event2 = tracker.processFrameworkEvent(json, tabId);
			expect(event1?.id).toBeTruthy();
			expect(event2?.id).toBeTruthy();
			expect(event1?.id).not.toBe(event2?.id);
		});

		it("preserves timestamp from parsed data", () => {
			const ts = 9999999;
			const json = JSON.stringify({
				type: "framework_detect",
				ts,
				data: { framework: "vue", version: "3.0.0", rootCount: 0, componentCount: 0 },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event?.timestamp).toBe(ts);
		});

		it("generates correct summary for detect event with plural roots", () => {
			const json = JSON.stringify({
				type: "framework_detect",
				ts: 1000,
				data: { framework: "react", version: "18.2.0", rootCount: 3, componentCount: 0 },
			});
			const event = tracker.processFrameworkEvent(json, tabId);
			expect(event?.summary).toBe("[react] React 18.2.0 detected (3 roots)");
		});
	});
});

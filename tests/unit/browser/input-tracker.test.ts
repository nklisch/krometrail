import { beforeEach, describe, expect, it } from "vitest";
import { InputTracker } from "../../../src/browser/recorder/input-tracker.js";

describe("InputTracker", () => {
	let tracker: InputTracker;

	beforeEach(() => {
		tracker = new InputTracker();
	});

	describe("getInjectionScript", () => {
		it("returns a non-empty string", () => {
			const script = tracker.getInjectionScript();
			expect(typeof script).toBe("string");
			expect(script.length).toBeGreaterThan(0);
		});

		it("contains __BL__ marker", () => {
			expect(tracker.getInjectionScript()).toContain("__BL__");
		});

		it("contains password masking logic", () => {
			expect(tracker.getInjectionScript()).toContain("[MASKED]");
		});

		it("contains keyboard hotkey listener", () => {
			expect(tracker.getInjectionScript()).toContain("ctrlKey");
			expect(tracker.getInjectionScript()).toContain("shiftKey");
		});
	});

	describe("processInputEvent", () => {
		it("processes click event", () => {
			const data = JSON.stringify({ type: "click", ts: Date.now(), selector: "#submit-btn", text: "Submit", tag: "button" });
			const event = tracker.processInputEvent(data, "tab1");

			expect(event).not.toBeNull();
			expect(event?.type).toBe("user_input");
			expect(event?.summary).toContain("Click #submit-btn");
			expect(event?.summary).toContain("Submit");
			expect(event?.data.action).toBe("click");
			expect(event?.data.selector).toBe("#submit-btn");
		});

		it("processes submit event with field values", () => {
			const data = JSON.stringify({
				type: "submit",
				ts: Date.now(),
				selector: "form",
				action: "/api/login",
				fields: { username: "alice", password: "[MASKED]" },
			});
			const event = tracker.processInputEvent(data, "tab1");

			expect(event?.type).toBe("user_input");
			expect(event?.summary).toContain("Form submit");
			expect(event?.data.action).toBe("submit");
			expect((event?.data.fields as Record<string, string>).username).toBe("alice");
			expect((event?.data.fields as Record<string, string>).password).toBe("[MASKED]");
		});

		it("processes change event", () => {
			const data = JSON.stringify({
				type: "change",
				ts: Date.now(),
				selector: 'input[name="email"]',
				value: "test@example.com",
				tag: "input",
			});
			const event = tracker.processInputEvent(data, "tab1");

			expect(event?.type).toBe("user_input");
			expect(event?.summary).toContain("Change");
			expect(event?.summary).toContain("test@example.com");
			expect(event?.data.action).toBe("change");
		});

		it("masks password values in change events", () => {
			const data = JSON.stringify({
				type: "change",
				ts: Date.now(),
				selector: 'input[name="password"]',
				value: "[MASKED]",
				tag: "input",
			});
			const event = tracker.processInputEvent(data, "tab1");

			expect(event?.summary).toContain("[MASKED]");
			expect(event?.data.value).toBe("[MASKED]");
		});

		it("returns marker event type for keyboard marker", () => {
			const data = JSON.stringify({ type: "marker", ts: Date.now(), label: "Bug found here" });
			const event = tracker.processInputEvent(data, "tab1");

			expect(event?.type).toBe("marker");
			expect(event?.data.source).toBe("keyboard");
			expect(event?.data.label).toBe("Bug found here");
		});

		it("returns null for invalid JSON", () => {
			expect(tracker.processInputEvent("not json", "tab1")).toBeNull();
			expect(tracker.processInputEvent("{}", "tab1")).toBeNull(); // missing type/ts
		});

		it("includes tabId in event", () => {
			const data = JSON.stringify({ type: "click", ts: Date.now(), selector: "#btn", text: "", tag: "button" });
			const event = tracker.processInputEvent(data, "myTab");

			expect(event?.tabId).toBe("myTab");
		});

		it("generates unique event IDs", () => {
			const data = JSON.stringify({ type: "click", ts: Date.now(), selector: "#btn", text: "", tag: "button" });
			const e1 = tracker.processInputEvent(data, "tab1");
			const e2 = tracker.processInputEvent(data, "tab1");

			expect(e1?.id).not.toBe(e2?.id);
		});

		it("shows field count in form submit summary", () => {
			const data = JSON.stringify({
				type: "submit",
				ts: Date.now(),
				selector: "form",
				action: "/submit",
				fields: { a: "1", b: "2", c: "3" },
			});
			const event = tracker.processInputEvent(data, "tab1");

			expect(event?.summary).toContain("3 fields");
		});
	});
});

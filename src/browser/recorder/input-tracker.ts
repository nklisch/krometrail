import type { RecordedEvent } from "../types.js";

interface InputEventData {
	type: "click" | "submit" | "change" | "marker";
	ts: number;
	selector?: string;
	text?: string;
	tag?: string;
	action?: string;
	fields?: Record<string, string>;
	value?: string;
	label?: string;
}

/**
 * Captures user interactions (clicks, form submissions, field changes) via a
 * minimal script injected into each page via Page.addScriptToEvaluateOnNewDocument.
 *
 * Events are reported back through console.debug('__BL__', ...) and intercepted
 * from the Runtime.consoleAPICalled stream.
 */
export class InputTracker {
	/** Get the injection script source to be injected into each page. */
	getInjectionScript(): string {
		return `(function() {
  function sel(el) {
    if (el.id) return '#' + el.id;
    if (el.getAttribute('data-testid')) return '[data-testid="' + el.getAttribute('data-testid') + '"]';
    if (el.name) return el.tagName.toLowerCase() + '[name="' + el.name + '"]';
    return el.tagName.toLowerCase();
  }

  function report(type, detail) {
    try {
      console.debug('__BL__', JSON.stringify(Object.assign({ type: type, ts: Date.now() }, detail)));
    } catch (e) {}
  }

  document.addEventListener('click', function(e) {
    var t = e.target.closest('[id],[name],[data-testid],[role="button"],a,button,input,select,label');
    if (!t) return;
    report('click', { selector: sel(t), text: (t.textContent || '').trim().slice(0, 80), tag: t.tagName.toLowerCase() });
  }, true);

  document.addEventListener('submit', function(e) {
    var form = e.target;
    var fields = {};
    var inputs = form.querySelectorAll('input,select,textarea');
    for (var i = 0; i < inputs.length; i++) {
      var inp = inputs[i];
      var name = inp.name || inp.id || sel(inp);
      fields[name] = inp.type === 'password' ? '[MASKED]' : (inp.value || '').slice(0, 200);
    }
    report('submit', { selector: sel(form), action: form.action, fields: fields });
  }, true);

  document.addEventListener('change', function(e) {
    var t = e.target;
    report('change', {
      selector: sel(t),
      value: t.type === 'password' ? '[MASKED]' : (t.value || '').slice(0, 200),
      tag: t.tagName.toLowerCase()
    });
  }, true);

  document.addEventListener('keydown', function(e) {
    if (e.ctrlKey && e.shiftKey && (e.key === 'M' || e.key === 'm')) {
      e.preventDefault();
      report('marker', { label: 'Keyboard marker' });
    }
  }, true);
})();`;
	}

	/**
	 * Process a __BL__ prefixed console message into a RecordedEvent.
	 * Returns null if the data is invalid or represents an internal marker event
	 * (markers are handled separately by the orchestrator).
	 */
	processInputEvent(data: string, tabId: string): RecordedEvent | null {
		let parsed: InputEventData;
		try {
			parsed = JSON.parse(data) as InputEventData;
		} catch {
			return null;
		}

		if (!parsed.type || !parsed.ts) return null;

		// Keyboard marker events are surfaced as marker placement requests by the orchestrator
		if (parsed.type === "marker") {
			return {
				id: crypto.randomUUID(),
				timestamp: parsed.ts,
				type: "marker",
				tabId,
				summary: `Keyboard marker: ${parsed.label ?? "unnamed"}`,
				data: { label: parsed.label, source: "keyboard" },
			};
		}

		return this.buildUserInputEvent(parsed, tabId);
	}

	private buildUserInputEvent(parsed: InputEventData, tabId: string): RecordedEvent | null {
		const selector = parsed.selector ?? "unknown";

		switch (parsed.type) {
			case "click": {
				const text = parsed.text ? ` "${parsed.text}"` : "";
				return {
					id: crypto.randomUUID(),
					timestamp: parsed.ts,
					type: "user_input",
					tabId,
					summary: `Click ${selector}${text}`,
					data: { action: "click", selector, text: parsed.text, tag: parsed.tag },
				};
			}

			case "submit": {
				const fieldCount = parsed.fields ? Object.keys(parsed.fields).length : 0;
				return {
					id: crypto.randomUUID(),
					timestamp: parsed.ts,
					type: "user_input",
					tabId,
					summary: `Form submit ${selector} (${fieldCount} fields)`,
					data: { action: "submit", selector, formAction: parsed.action, fields: parsed.fields },
				};
			}

			case "change": {
				const displayValue = parsed.value === "[MASKED]" ? "[MASKED]" : `"${parsed.value ?? ""}"`;
				return {
					id: crypto.randomUUID(),
					timestamp: parsed.ts,
					type: "user_input",
					tabId,
					summary: `Change ${selector} → ${displayValue}`,
					data: { action: "change", selector, value: parsed.value, tag: parsed.tag },
				};
			}

			default:
				return null;
		}
	}
}

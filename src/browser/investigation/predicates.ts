import type { EventRow } from "../storage/database.js";

export function isErrorEvent(e: EventRow): boolean {
	if (e.type === "page_error") return true;
	if (e.type === "console" && e.summary.startsWith("[error]")) return true;
	if (e.type === "network_response") {
		const status = Number.parseInt(e.summary, 10);
		return status >= 400;
	}
	return false;
}

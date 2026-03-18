import type { QueryEngine } from "./query-engine.js";

/**
 * Resolve a timestamp reference to epoch ms.
 *
 * Accepts:
 * - Pure numeric string: treated as epoch ms
 * - ISO timestamp: "2024-01-01T12:00:00Z" → epoch ms
 * - Wall-clock time: "HH:mm:ss" or "HH:mm:ss.SSS" → resolved relative to session start date
 * - Event ID (UUID): looks up the event's timestamp via queryEngine
 *
 * @throws Error if the reference cannot be resolved
 */
export function resolveTimestamp(queryEngine: QueryEngine, sessionId: string, ref: string): number {
	// Pure numeric string → epoch ms
	if (/^\d+$/.test(ref)) return Number(ref);

	// ISO timestamp (YYYY-MM-DD prefix or contains T+zone offset)
	if (/^\d{4}-\d{2}-\d{2}/.test(ref) || (ref.includes("T") && ref.includes("-"))) {
		return new Date(ref).getTime();
	}

	// Wall-clock time: HH:mm:ss or HH:mm:ss.SSS
	const wallClockMatch = ref.match(/^(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,3}))?$/);
	if (wallClockMatch) {
		const session = queryEngine.getSession(sessionId);
		const sessionStartDate = new Date(session.started_at);
		// Use the session start date as the calendar date context
		const resolved = new Date(sessionStartDate);
		resolved.setUTCHours(
			Number.parseInt(wallClockMatch[1], 10),
			Number.parseInt(wallClockMatch[2], 10),
			Number.parseInt(wallClockMatch[3], 10),
			wallClockMatch[4] ? Number.parseInt(wallClockMatch[4].padEnd(3, "0"), 10) : 0,
		);
		// Handle day rollover: if resolved time is before session start, add a day
		if (resolved.getTime() < session.started_at) {
			resolved.setUTCDate(resolved.getUTCDate() + 1);
		}
		return resolved.getTime();
	}

	// Event ID — look up by event_id
	const event = queryEngine.getFullEvent(sessionId, ref);
	if (event) return event.timestamp;

	throw new Error(`Cannot resolve "${ref}" to a timestamp or event. ` + "Accepted formats: ISO timestamp (2024-01-01T12:00:00Z), wall-clock time (01:50:39.742), epoch ms, or event ID.");
}

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { BrowserDatabase, EventRow, MarkerRow, NetworkBodyRow, SessionRow } from "../storage/database.js";
import { EventWriter } from "../storage/event-writer.js";
import type { RecordedEvent } from "../types.js";

export type { SessionRow };

export class QueryEngine {
	constructor(
		private db: BrowserDatabase,
		_dataDir: string,
	) {}

	// --- Session queries ---

	listSessions(filter?: SessionListFilter): SessionSummary[] {
		const rows = this.db.listSessions(filter);
		return rows.map((row) => ({
			id: row.id,
			startedAt: row.started_at,
			duration: (row.ended_at ?? Date.now()) - row.started_at,
			url: row.tab_url,
			title: row.tab_title,
			eventCount: row.event_count,
			markerCount: row.marker_count,
			errorCount: row.error_count,
		}));
	}

	// --- Overview queries ---

	getOverview(sessionId: string, options?: OverviewOptions): SessionOverview {
		const session = this.db.getSession(sessionId);
		const markers = this.db.queryMarkers(sessionId);

		const result: SessionOverview = {
			session: { id: session.id, startedAt: session.started_at, url: session.tab_url, title: session.tab_title },
			markers,
			timeline: [],
			networkSummary: null,
			errorSummary: null,
		};

		// Navigation timeline
		if (!options?.include || options.include.includes("timeline")) {
			result.timeline = this.db.queryEvents(sessionId, {
				types: ["navigation", "marker"],
				limit: 50,
			});
		}

		// Network summary
		if (!options?.include || options.include.includes("network_summary")) {
			const allNetwork = this.db.queryEvents(sessionId, {
				types: ["network_response"],
			});
			result.networkSummary = this.summarizeNetwork(allNetwork);
		}

		// Error summary
		if (!options?.include || options.include.includes("errors")) {
			const candidates = this.db.queryEvents(sessionId, {
				types: ["network_response", "page_error", "console"],
			});
			result.errorSummary = candidates.filter((e) => this.isErrorEvent(e));
		}

		// Time range focus around marker
		if (options?.aroundMarker) {
			const marker = markers.find((m) => m.id === options.aroundMarker);
			if (marker) {
				const padding = 60_000; // ±60 seconds for overview
				result.timeline = result.timeline.filter((e) => Math.abs(e.timestamp - marker.timestamp) <= padding);
			}
		}

		// Time range focus
		if (options?.timeRange) {
			const { start, end } = options.timeRange;
			result.timeline = result.timeline.filter((e) => e.timestamp >= start && e.timestamp <= end);
		}

		return result;
	}

	// --- Search queries ---

	search(sessionId: string, params: SearchParams): EventRow[] {
		// Resolve aroundMarker into a timeRange (only if no explicit timeRange provided)
		if (params.filters?.aroundMarker && !params.filters.timeRange) {
			const markers = this.db.queryMarkers(sessionId);
			const marker = markers.find((m) => m.id === params.filters?.aroundMarker);
			if (!marker) throw new Error(`Marker not found: ${params.filters.aroundMarker}`);
			params = {
				...params,
				filters: {
					...params.filters,
					timeRange: { start: marker.timestamp - 120_000, end: marker.timestamp + 30_000 },
				},
			};
		}

		if (params.query) {
			return this.db.searchFTS(sessionId, params.query, params.maxResults ?? 10);
		}

		let results = this.db.queryEvents(sessionId, {
			types: params.filters?.eventTypes,
			timeRange: params.filters?.timeRange,
			limit: params.maxResults ?? 10,
		});

		// Post-filter by status codes (parsed from summary string)
		if (params.filters?.statusCodes && params.filters.statusCodes.length > 0) {
			const codes = params.filters.statusCodes;
			results = results.filter((e) => {
				const status = Number.parseInt(e.summary, 10);
				return codes.includes(status);
			});
		}

		// Post-filter by URL pattern (glob-style match on summary)
		if (params.filters?.urlPattern) {
			const pattern = params.filters.urlPattern;
			const regex = new RegExp(pattern.replace(/\*\*/g, ".*").replace(/\*/g, "[^/]*"), "i");
			results = results.filter((e) => regex.test(e.summary));
		}

		// Post-filter by console levels (parsed from summary "[level] message")
		if (params.filters?.consoleLevels && params.filters.consoleLevels.length > 0) {
			const levels = params.filters.consoleLevels;
			results = results.filter((e) => {
				if (e.type !== "console") return false;
				const match = e.summary.match(/^\[(\w+)\]/);
				return match ? levels.includes(match[1]) : false;
			});
		}

		// Post-filter by text content in summary (case-insensitive substring)
		if (params.filters?.containsText) {
			const text = params.filters.containsText.toLowerCase();
			results = results.filter((e) => e.summary.toLowerCase().includes(text));
		}

		return results;
	}

	// --- Inspect queries ---

	inspect(sessionId: string, params: InspectParams): InspectResult {
		const session = this.db.getSession(sessionId);
		const recordingDir = session.recording_dir;

		// Resolve the target event
		let targetEvent: EventRow;
		if (params.eventId) {
			targetEvent = this.db.getEventById(sessionId, params.eventId);
		} else if (params.markerId) {
			const marker = this.db.getMarkerById(params.markerId);
			const events = this.db.queryEvents(sessionId, {
				timeRange: { start: marker.timestamp - 1000, end: marker.timestamp + 1000 },
				limit: 1,
			});
			if (!events[0]) throw new Error(`No events found near marker ${params.markerId}`);
			targetEvent = events[0];
		} else if (params.timestamp !== undefined) {
			const events = this.db.queryEvents(sessionId, {
				timeRange: { start: params.timestamp - 500, end: params.timestamp + 500 },
				limit: 1,
			});
			if (!events[0]) throw new Error(`No events found near timestamp ${params.timestamp}`);
			targetEvent = events[0];
		} else {
			throw new Error("Must provide eventId, markerId, or timestamp");
		}

		// Read full event detail from JSONL
		const fullEvent = EventWriter.readAt(resolve(recordingDir, "events.jsonl"), targetEvent.detail_offset, targetEvent.detail_length);

		const result: InspectResult = {
			event: fullEvent,
			surroundingEvents: [],
			networkBody: null,
			screenshot: null,
		};

		// Surrounding context
		if (!params.include || params.include.includes("surrounding_events")) {
			const windowMs = (params.contextWindow ?? 5) * 1000;
			result.surroundingEvents = this.db.queryEvents(sessionId, {
				timeRange: {
					start: fullEvent.timestamp - windowMs,
					end: fullEvent.timestamp + windowMs,
				},
				limit: 20,
			});
		}

		// Network body
		if (!params.include || params.include.includes("network_body")) {
			const bodyRef = this.db.getNetworkBody(targetEvent.event_id);
			if (bodyRef) {
				if (bodyRef.response_body_path) {
					const bodyPath = resolve(recordingDir, "network", bodyRef.response_body_path);
					if (existsSync(bodyPath)) {
						result.networkBody = {
							response: readFileSync(bodyPath, "utf-8"),
							contentType: bodyRef.content_type ?? undefined,
							size: bodyRef.response_size ?? undefined,
						};
					}
				}
				if (bodyRef.request_body_path) {
					const bodyPath = resolve(recordingDir, "network", bodyRef.request_body_path);
					if (existsSync(bodyPath)) {
						result.networkBody = result.networkBody ?? {};
						result.networkBody.request = readFileSync(bodyPath, "utf-8");
					}
				}
			}
		}

		// Nearest screenshot
		if (!params.include || params.include.includes("screenshot")) {
			const screenshotDir = resolve(recordingDir, "screenshots");
			if (existsSync(screenshotDir)) {
				const files = readdirSync(screenshotDir)
					.filter((f) => f.endsWith(".jpg") || f.endsWith(".png"))
					.sort();
				if (files.length > 0) {
					const targetTs = fullEvent.timestamp;
					let nearest = files[0];
					let nearestDist = Number.POSITIVE_INFINITY;
					for (const f of files) {
						const ts = Number.parseInt(f.replace(/\.(jpg|png)$/, ""), 10);
						const dist = Math.abs(ts - targetTs);
						if (dist < nearestDist) {
							nearest = f;
							nearestDist = dist;
						}
					}
					result.screenshot = resolve(screenshotDir, nearest);
				}
			}
		}

		return result;
	}

	// --- Convenience helpers for Phase 12 ---

	getSession(sessionId: string): SessionRow {
		return this.db.getSession(sessionId);
	}

	getMarkers(sessionId: string): MarkerRow[] {
		return this.db.queryMarkers(sessionId);
	}

	getFullEvent(sessionId: string, eventId: string): RecordedEvent | null {
		try {
			const eventRow = this.db.getEventById(sessionId, eventId);
			const session = this.db.getSession(sessionId);
			return EventWriter.readAt(resolve(session.recording_dir, "events.jsonl"), eventRow.detail_offset, eventRow.detail_length);
		} catch {
			return null;
		}
	}

	getNetworkBody(eventId: string): NetworkBodyRow | undefined {
		return this.db.getNetworkBody(eventId);
	}

	readNetworkBody(sessionId: string, relPath: string): string | undefined {
		try {
			const session = this.db.getSession(sessionId);
			const fullPath = resolve(session.recording_dir, "network", relPath);
			if (!existsSync(fullPath)) return undefined;
			return readFileSync(fullPath, "utf-8");
		} catch {
			return undefined;
		}
	}

	private summarizeNetwork(events: EventRow[]): NetworkSummary {
		let total = 0;
		let succeeded = 0;
		let failed = 0;
		const notable: string[] = [];

		for (const e of events) {
			total++;
			const status = Number.parseInt(e.summary, 10);
			if (status >= 400) {
				failed++;
				notable.push(e.summary);
			} else {
				succeeded++;
			}
		}

		return { total, succeeded, failed, notable };
	}

	private isErrorEvent(e: EventRow): boolean {
		if (e.type === "page_error") return true;
		if (e.type === "console" && e.summary.startsWith("[error]")) return true;
		if (e.type === "network_response") {
			const status = Number.parseInt(e.summary, 10);
			return status >= 400;
		}
		return false;
	}
}

// --- Query types ---

export interface SessionListFilter {
	after?: number;
	before?: number;
	urlContains?: string;
	hasMarkers?: boolean;
	hasErrors?: boolean;
	limit?: number;
}

export interface OverviewOptions {
	include?: ("timeline" | "markers" | "errors" | "network_summary")[];
	aroundMarker?: string;
	timeRange?: { start: number; end: number };
}

export interface SearchParams {
	query?: string;
	filters?: {
		eventTypes?: string[];
		statusCodes?: number[];
		urlPattern?: string;
		consoleLevels?: string[];
		timeRange?: { start: number; end: number };
		containsText?: string;
		aroundMarker?: string; // marker ID, implies ±120s/+30s time range
	};
	maxResults?: number;
}

export interface InspectParams {
	eventId?: string;
	markerId?: string;
	timestamp?: number;
	include?: ("surrounding_events" | "network_body" | "screenshot" | "form_state" | "console_context")[];
	contextWindow?: number; // seconds
}

// --- Result types ---

export interface SessionSummary {
	id: string;
	startedAt: number;
	duration: number;
	url: string;
	title: string;
	eventCount: number;
	markerCount: number;
	errorCount: number;
}

export interface SessionOverview {
	session: { id: string; startedAt: number; url: string; title: string };
	markers: MarkerRow[];
	timeline: EventRow[];
	networkSummary: NetworkSummary | null;
	errorSummary: EventRow[] | null;
}

export interface NetworkSummary {
	total: number;
	succeeded: number;
	failed: number;
	notable: string[];
}

export interface InspectResult {
	event: RecordedEvent;
	surroundingEvents: EventRow[];
	networkBody: { request?: string; response?: string; contentType?: string; size?: number } | null;
	screenshot: string | null;
}

// Re-export row types for consumers
export type { EventRow, MarkerRow, NetworkBodyRow, SessionRow };

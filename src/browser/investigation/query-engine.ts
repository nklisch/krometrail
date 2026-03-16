import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { InspectInclude, OverviewInclude } from "../../core/enums.js";
import type { BrowserDatabase, EventRow, MarkerRow, NetworkBodyRow, SessionRow } from "../storage/database.js";
import { EventWriter } from "../storage/event-writer.js";
import type { RecordedEvent } from "../types.js";
import { MARKER_LOOKAHEAD_MS, MARKER_LOOKBACK_MS } from "./format-helpers.js";
import { isErrorEvent } from "./predicates.js";
import { resolveTimestamp } from "./resolve-timestamp.js";

export class QueryEngine {
	constructor(
		private db: BrowserDatabase,
		_dataDir: string,
	) {}

	/** Resolve "latest" to the most recent session ID, or return the ID as-is. */
	private resolveSessionId(sessionId: string): string {
		if (sessionId === "latest") {
			const sessions = this.db.listSessions({ limit: 1 });
			if (sessions.length === 0) throw new Error("No sessions found");
			return sessions[0].id;
		}
		return sessionId;
	}

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
		sessionId = this.resolveSessionId(sessionId);
		const session = this.db.getSession(sessionId);
		const markers = this.db.queryMarkers(sessionId);

		const result: SessionOverview = {
			session: { id: session.id, startedAt: session.started_at, url: session.tab_url, title: session.tab_title },
			markers,
			timeline: [],
			networkSummary: null,
			errorSummary: null,
			frameworkSummary: null,
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
			result.errorSummary = candidates.filter((e) => isErrorEvent(e));
		}

		// Framework summary
		if (!options?.include || options.include.includes("framework")) {
			result.frameworkSummary = this.summarizeFramework(sessionId);
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
		sessionId = this.resolveSessionId(sessionId);
		// Resolve aroundMarker into a timeRange (only if no explicit timeRange provided)
		if (params.filters?.aroundMarker && !params.filters.timeRange) {
			const markers = this.db.queryMarkers(sessionId);
			const ref = params.filters?.aroundMarker;
			const marker = markers.find((m) => m.id === ref || m.label === ref);
			if (!marker) throw new Error(`Marker not found: ${params.filters.aroundMarker}`);
			params = {
				...params,
				filters: {
					...params.filters,
					timeRange: { start: marker.timestamp - MARKER_LOOKBACK_MS, end: marker.timestamp + MARKER_LOOKAHEAD_MS },
				},
			};
		}

		if (params.query) {
			return this.db.searchFTS(sessionId, params.query, params.maxResults ?? 10);
		}

		// When post-filtering by status codes, fetch all events first so the
		// limit is applied after filtering, not before.
		const needsPostFilter = !!(params.filters?.statusCodes?.length || params.filters?.urlPattern || params.filters?.framework || params.filters?.component || params.filters?.pattern);

		// Framework filter — auto-narrow to framework event types if not already specified
		if (params.filters?.framework && (!params.filters.eventTypes || params.filters.eventTypes.length === 0)) {
			params = {
				...params,
				filters: {
					...params.filters,
					eventTypes: ["framework_detect", "framework_state", "framework_error"],
				},
			};
		}

		let results = this.db.queryEvents(sessionId, {
			types: params.filters?.eventTypes,
			timeRange: params.filters?.timeRange,
			limit: needsPostFilter ? undefined : (params.maxResults ?? 10),
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

		// Post-filter by framework name — match [framework] or [framework:severity] prefix in summary
		if (params.filters?.framework) {
			const fw = params.filters.framework;
			results = results.filter((e) => e.summary.startsWith(`[${fw}]`) || e.summary.startsWith(`[${fw}:`));
		}

		// Post-filter by component name — substring match on component name in summary
		if (params.filters?.component) {
			const comp = params.filters.component;
			results = results.filter((e) => e.type.startsWith("framework_") && e.summary.includes(comp));
		}

		// Post-filter by bug pattern name — match pattern name in framework_error summaries
		if (params.filters?.pattern) {
			const pat = params.filters.pattern;
			results = results.filter((e) => e.type === "framework_error" && e.summary.includes(pat));
		}

		if (needsPostFilter) {
			results = results.slice(0, params.maxResults ?? 10);
		}

		return results;
	}

	// --- Inspect queries ---

	inspect(sessionId: string, params: InspectParams): InspectResult {
		sessionId = this.resolveSessionId(sessionId);
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
			const ts = resolveTimestamp(this, sessionId, params.timestamp);
			const events = this.db.queryEvents(sessionId, {
				timeRange: { start: ts - 500, end: ts + 500 },
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
		return this.db.getSession(this.resolveSessionId(sessionId));
	}

	getMarkers(sessionId: string): MarkerRow[] {
		return this.db.queryMarkers(this.resolveSessionId(sessionId));
	}

	getFullEvent(sessionId: string, eventId: string): RecordedEvent | null {
		try {
			sessionId = this.resolveSessionId(sessionId);
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
			const session = this.db.getSession(this.resolveSessionId(sessionId));
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

	private summarizeFramework(sessionId: string): FrameworkSummary | null {
		const detectEvents = this.db.queryEvents(sessionId, { types: ["framework_detect"] });
		if (detectEvents.length === 0) return null;

		const frameworks: FrameworkSummary["frameworks"] = [];
		for (const e of detectEvents) {
			const full = this.getFullEvent(sessionId, e.event_id);
			if (full) {
				frameworks.push({
					name: full.data.framework as string,
					version: (full.data.version as string) ?? "unknown",
					componentCount: (full.data.componentCount as number) ?? 0,
					storeDetected: full.data.storeDetected as string | undefined,
				});
			}
		}

		const stateEvents = this.db.queryEvents(sessionId, { types: ["framework_state"] });
		const errorEvents = this.db.queryEvents(sessionId, { types: ["framework_error"] });

		const errors = { high: 0, medium: 0, low: 0 };
		for (const e of errorEvents) {
			const severity = e.summary.match(/\[.*?:(high|medium|low)\]/)?.[1];
			if (severity && severity in errors) {
				errors[severity as keyof typeof errors]++;
			}
		}

		const componentCounts = new Map<string, number>();
		for (const e of stateEvents) {
			const match = e.summary.match(/\[.*?\] (.+?):/);
			if (match) {
				const name = match[1];
				componentCounts.set(name, (componentCounts.get(name) ?? 0) + 1);
			}
		}
		const topComponents = [...componentCounts.entries()]
			.sort((a, b) => b[1] - a[1])
			.slice(0, 10)
			.map(([name, updateCount]) => ({ name, updateCount }));

		return { frameworks, stateEventCount: stateEvents.length, errors, topComponents };
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
	include?: OverviewInclude[];
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
		/** Filter by framework name. Implies eventTypes narrowed to framework_* types. */
		framework?: string;
		/** Filter by component name (substring match on summary). */
		component?: string;
		/** Filter by bug pattern name (exact match on framework_error events). */
		pattern?: string;
	};
	maxResults?: number;
}

export interface InspectParams {
	eventId?: string;
	markerId?: string;
	timestamp?: string; // ISO timestamp, event_id, or epoch ms as string
	include?: InspectInclude[];
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

export interface FrameworkSummary {
	/** Detected frameworks with version info. */
	frameworks: Array<{
		name: string;
		version: string;
		componentCount: number;
		storeDetected?: string;
	}>;
	/** Total framework_state events in the session. */
	stateEventCount: number;
	/** Total framework_error events, grouped by severity. */
	errors: { high: number; medium: number; low: number };
	/** Top components by update frequency (most active first). */
	topComponents: Array<{ name: string; updateCount: number }>;
}

export interface SessionOverview {
	session: { id: string; startedAt: number; url: string; title: string };
	markers: MarkerRow[];
	timeline: EventRow[];
	networkSummary: NetworkSummary | null;
	errorSummary: EventRow[] | null;
	frameworkSummary: FrameworkSummary | null;
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

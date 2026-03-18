import Database from "bun:sqlite";
import { resolve } from "node:path";
import { z } from "zod";
import { EventNotFoundError, MarkerNotFoundError, SessionNotFoundError } from "../../core/errors.js";
import { EventWriter } from "./event-writer.js";

export interface SessionRow {
	id: string;
	started_at: number;
	ended_at: number | null;
	tab_url: string;
	tab_title: string;
	event_count: number;
	marker_count: number;
	error_count: number;
	recording_dir: string;
}

export interface EventRow {
	rowid: number;
	session_id: string;
	event_id: string;
	timestamp: number;
	type: string;
	summary: string;
	detail_offset: number;
	detail_length: number;
}

export interface MarkerRow {
	id: string;
	session_id: string;
	timestamp: number;
	label: string | null;
	auto_detected: number;
	severity: string | null;
}

export interface NetworkBodyRow {
	event_id: string;
	session_id: string;
	request_body_path: string | null;
	response_body_path: string | null;
	response_size: number | null;
	content_type: string | null;
	request_content_type: string | null;
}

export const SessionFilterSchema = z.object({
	after: z.number().optional(),
	before: z.number().optional(),
	urlContains: z.string().optional(),
	hasMarkers: z.boolean().optional(),
	hasErrors: z.boolean().optional(),
	limit: z.number().int().positive().optional(),
});

export type SessionFilter = z.infer<typeof SessionFilterSchema>;

export const EventQueryFilterSchema = z.object({
	types: z.array(z.string()).optional(),
	timeRange: z.object({ start: z.number(), end: z.number() }).optional(),
	statusCodes: z.array(z.number().int()).optional(),
	limit: z.number().int().positive().optional(),
	offset: z.number().int().nonnegative().optional(),
});

export type EventQueryFilter = z.infer<typeof EventQueryFilterSchema>;

export class BrowserDatabase {
	private db: Database;

	constructor(dbPath: string) {
		this.db = new Database(dbPath);
		this.db.exec("PRAGMA journal_mode = WAL");
		this.db.exec("PRAGMA synchronous = NORMAL");
		this.migrate();
	}

	private migrate(): void {
		// Incremental migrations for existing databases
		try {
			this.db.exec("ALTER TABLE network_bodies ADD COLUMN request_content_type TEXT");
		} catch {
			// Column already exists — ignore
		}

		this.db.exec(`
			CREATE TABLE IF NOT EXISTS sessions (
				id TEXT PRIMARY KEY,
				started_at INTEGER NOT NULL,
				ended_at INTEGER,
				tab_url TEXT,
				tab_title TEXT,
				event_count INTEGER DEFAULT 0,
				marker_count INTEGER DEFAULT 0,
				error_count INTEGER DEFAULT 0,
				recording_dir TEXT NOT NULL
			);

			CREATE TABLE IF NOT EXISTS events (
				rowid INTEGER PRIMARY KEY,
				session_id TEXT NOT NULL REFERENCES sessions(id),
				event_id TEXT NOT NULL,
				timestamp INTEGER NOT NULL,
				type TEXT NOT NULL,
				summary TEXT NOT NULL,
				detail_offset INTEGER NOT NULL,
				detail_length INTEGER NOT NULL,
				UNIQUE(session_id, event_id)
			);

			CREATE TABLE IF NOT EXISTS markers (
				id TEXT PRIMARY KEY,
				session_id TEXT NOT NULL REFERENCES sessions(id),
				timestamp INTEGER NOT NULL,
				label TEXT,
				auto_detected INTEGER NOT NULL DEFAULT 0,
				severity TEXT
			);

			CREATE TABLE IF NOT EXISTS network_bodies (
				event_id TEXT PRIMARY KEY,
				session_id TEXT NOT NULL REFERENCES sessions(id),
				request_body_path TEXT,
				response_body_path TEXT,
				response_size INTEGER,
				content_type TEXT,
				request_content_type TEXT
			);

			CREATE INDEX IF NOT EXISTS idx_events_session_time
				ON events(session_id, timestamp);
			CREATE INDEX IF NOT EXISTS idx_events_type
				ON events(session_id, type);
			CREATE INDEX IF NOT EXISTS idx_markers_session
				ON markers(session_id, timestamp);

			CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(
				summary,
				content=events,
				content_rowid=rowid
			);

			CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
				INSERT INTO events_fts(rowid, summary) VALUES (new.rowid, new.summary);
			END;
		`);
	}

	// --- Session CRUD ---

	createSession(session: { id: string; startedAt: number; tabUrl: string; tabTitle: string; recordingDir: string }): void {
		this.db
			.prepare("INSERT INTO sessions (id, started_at, tab_url, tab_title, recording_dir) VALUES (?, ?, ?, ?, ?)")
			.run(session.id, session.startedAt, session.tabUrl, session.tabTitle, session.recordingDir);
	}

	updateSessionCounts(sessionId: string): void {
		this.db
			.prepare(
				`UPDATE sessions SET
					event_count = (SELECT COUNT(*) FROM events WHERE session_id = ?),
					marker_count = (SELECT COUNT(*) FROM markers WHERE session_id = ?),
					error_count = (SELECT COUNT(*) FROM events WHERE session_id = ? AND type = 'page_error')
				WHERE id = ?`,
			)
			.run(sessionId, sessionId, sessionId, sessionId);
	}

	endSession(sessionId: string, endedAt: number): void {
		this.db.prepare("UPDATE sessions SET ended_at = ? WHERE id = ?").run(endedAt, sessionId);
	}

	listSessions(filter?: SessionFilter): SessionRow[] {
		const conditions: string[] = [];
		const params: unknown[] = [];

		if (filter?.after !== undefined) {
			conditions.push("started_at > ?");
			params.push(filter.after);
		}
		if (filter?.before !== undefined) {
			conditions.push("started_at < ?");
			params.push(filter.before);
		}
		if (filter?.urlContains) {
			conditions.push("tab_url LIKE ?");
			params.push(`%${filter.urlContains}%`);
		}
		if (filter?.hasMarkers) {
			conditions.push("marker_count > 0");
		}
		if (filter?.hasErrors) {
			conditions.push("error_count > 0");
		}

		const where = conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";
		const limit = filter?.limit ? `LIMIT ${filter.limit}` : "";

		return this.db.prepare(`SELECT * FROM sessions ${where} ORDER BY started_at DESC ${limit}`).all(...params) as SessionRow[];
	}

	deleteSession(sessionId: string): void {
		// Delete in dependency order
		this.db.prepare("DELETE FROM events_fts WHERE rowid IN (SELECT rowid FROM events WHERE session_id = ?)").run(sessionId);
		this.db.prepare("DELETE FROM network_bodies WHERE session_id = ?").run(sessionId);
		this.db.prepare("DELETE FROM markers WHERE session_id = ?").run(sessionId);
		this.db.prepare("DELETE FROM events WHERE session_id = ?").run(sessionId);
		this.db.prepare("DELETE FROM sessions WHERE id = ?").run(sessionId);
	}

	// --- Event insertion ---

	insertEvent(event: { sessionId: string; eventId: string; timestamp: number; type: string; summary: string; detailOffset: number; detailLength: number }): void {
		this.db
			.prepare("INSERT OR IGNORE INTO events (session_id, event_id, timestamp, type, summary, detail_offset, detail_length) VALUES (?, ?, ?, ?, ?, ?, ?)")
			.run(event.sessionId, event.eventId, event.timestamp, event.type, event.summary, event.detailOffset, event.detailLength);
	}

	insertEventBatch(events: Array<{ sessionId: string; eventId: string; timestamp: number; type: string; summary: string; detailOffset: number; detailLength: number }>): void {
		const stmt = this.db.prepare("INSERT OR IGNORE INTO events (session_id, event_id, timestamp, type, summary, detail_offset, detail_length) VALUES (?, ?, ?, ?, ?, ?, ?)");
		const insertMany = this.db.transaction((rows: typeof events) => {
			for (const e of rows) {
				stmt.run(e.sessionId, e.eventId, e.timestamp, e.type, e.summary, e.detailOffset, e.detailLength);
			}
		});
		insertMany(events);
	}

	// --- Marker insertion ---

	insertMarker(marker: { id: string; sessionId: string; timestamp: number; label?: string; autoDetected: boolean; severity?: string }): void {
		this.db
			.prepare("INSERT OR IGNORE INTO markers (id, session_id, timestamp, label, auto_detected, severity) VALUES (?, ?, ?, ?, ?, ?)")
			.run(marker.id, marker.sessionId, marker.timestamp, marker.label ?? null, marker.autoDetected ? 1 : 0, marker.severity ?? null);
	}

	// --- Network body references ---

	insertNetworkBody(ref: { eventId: string; sessionId: string; requestBodyPath?: string; responseBodyPath?: string; responseSize?: number; contentType?: string; requestContentType?: string }): void {
		this.db
			.prepare(
				`INSERT OR REPLACE INTO network_bodies (event_id, session_id, request_body_path, response_body_path, response_size, content_type, request_content_type)
				VALUES (?, ?, ?, ?, ?, ?, ?)`,
			)
			.run(ref.eventId, ref.sessionId, ref.requestBodyPath ?? null, ref.responseBodyPath ?? null, ref.responseSize ?? null, ref.contentType ?? null, ref.requestContentType ?? null);
	}

	// --- Point lookups ---

	getSession(sessionId: string): SessionRow {
		const row = this.db.prepare("SELECT * FROM sessions WHERE id = ?").get(sessionId) as SessionRow | undefined;
		if (!row) throw new SessionNotFoundError(sessionId);
		return row;
	}

	getEventById(sessionId: string, eventId: string): EventRow {
		const row = this.db.prepare("SELECT * FROM events WHERE session_id = ? AND event_id = ?").get(sessionId, eventId) as EventRow | undefined;
		if (!row) throw new EventNotFoundError(eventId);
		return row;
	}

	getMarkerById(markerId: string): MarkerRow {
		const row = this.db.prepare("SELECT * FROM markers WHERE id = ?").get(markerId) as MarkerRow | undefined;
		if (!row) throw new MarkerNotFoundError(markerId);
		return row;
	}

	getNetworkBody(eventId: string): NetworkBodyRow | undefined {
		return this.db.prepare("SELECT * FROM network_bodies WHERE event_id = ?").get(eventId) as NetworkBodyRow | undefined;
	}

	// --- Queries ---

	queryEvents(sessionId: string, filter: EventQueryFilter): EventRow[] {
		const conditions: string[] = ["session_id = ?"];
		const params: unknown[] = [sessionId];

		if (filter.types && filter.types.length > 0) {
			conditions.push(`type IN (${filter.types.map(() => "?").join(", ")})`);
			params.push(...filter.types);
		}
		if (filter.timeRange) {
			conditions.push("timestamp >= ? AND timestamp <= ?");
			params.push(filter.timeRange.start, filter.timeRange.end);
		}

		const where = `WHERE ${conditions.join(" AND ")}`;
		const limit = filter.limit ? `LIMIT ${filter.limit}` : "";
		const offset = filter.offset ? `OFFSET ${filter.offset}` : "";

		return this.db.prepare(`SELECT * FROM events ${where} ORDER BY timestamp ASC ${limit} ${offset}`).all(...params) as EventRow[];
	}

	queryMarkers(sessionId: string): MarkerRow[] {
		return this.db.prepare("SELECT * FROM markers WHERE session_id = ? ORDER BY timestamp ASC").all(sessionId) as MarkerRow[];
	}

	searchFTS(sessionId: string, query: string, limit = 50): EventRow[] {
		return this.db
			.prepare(
				`SELECT e.* FROM events e
				JOIN events_fts fts ON e.rowid = fts.rowid
				WHERE e.session_id = ? AND events_fts MATCH ?
				ORDER BY e.timestamp ASC
				LIMIT ?`,
			)
			.all(sessionId, query, limit) as EventRow[];
	}

	/**
	 * Read the full event JSON from the JSONL file using stored byte offsets.
	 */
	getEventByOffset(sessionId: string, offset: number, length: number): string {
		const session = this.db.prepare("SELECT recording_dir FROM sessions WHERE id = ?").get(sessionId) as { recording_dir: string } | undefined;
		if (!session) throw new SessionNotFoundError(sessionId);
		const jsonlPath = resolve(session.recording_dir, "events.jsonl");
		const event = EventWriter.readAt(jsonlPath, offset, length);
		return JSON.stringify(event);
	}

	close(): void {
		this.db.close();
	}
}

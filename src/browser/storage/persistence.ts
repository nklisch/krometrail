import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { z } from "zod";
import { getKrometrailSubdir } from "../../core/paths.js";
import type { CDPClient } from "../recorder/cdp-client.js";
import type { RollingBuffer } from "../recorder/rolling-buffer.js";
import type { BrowserSessionInfo, Marker, RecordedEvent } from "../types.js";
import { BrowserDatabase } from "./database.js";
import { EventWriter } from "./event-writer.js";
import { NetworkExtractor } from "./network-extractor.js";
import { type RetentionConfig, RetentionConfigSchema, RetentionManager } from "./retention.js";
import type { ScreenshotCapture } from "./screenshot.js";

export const PersistenceConfigSchema = z.object({
	/** Root data directory. Default: ~/.krometrail/browser */
	dataDir: z.string().default(resolve(getKrometrailSubdir("browser"))),
	/** Ms of context around each marker to persist. Should match buffer.markerPaddingMs. */
	markerPaddingMs: z.number().default(120_000),
});

export type PersistenceConfig = z.infer<typeof PersistenceConfigSchema>;

interface ActiveSession {
	writer: EventWriter;
	dir: string;
	persistedEventIds: Set<string>;
	openMarkerWindows: Array<{ markerId: string; start: number; end: number }>;
}

function formatTimestamp(ts: number): string {
	return new Date(ts).toISOString().replace("T", "_").replace(/:/g, "-").slice(0, 19);
}

function slugify(url: string): string {
	return url
		.replace(/^https?:\/\//, "")
		.replace(/[^a-z0-9]+/gi, "-")
		.toLowerCase()
		.slice(0, 50)
		.replace(/-+$/, "");
}

export class PersistencePipeline {
	private config: PersistenceConfig;
	private db: BrowserDatabase;
	private activeSessions = new Map<string, ActiveSession>();
	private networkExtractor = new NetworkExtractor();

	constructor(
		config: Partial<PersistenceConfig>,
		private screenshotCapture?: ScreenshotCapture,
	) {
		this.config = PersistenceConfigSchema.parse(config);
		const dbPath = resolve(this.config.dataDir, "index.db");
		mkdirSync(this.config.dataDir, { recursive: true });
		this.db = new BrowserDatabase(dbPath);
	}

	/**
	 * Called when a marker is placed. Persists the buffer window around the marker.
	 */
	async onMarkerPlaced(marker: Marker, buffer: RollingBuffer, sessionInfo: BrowserSessionInfo, cdpClient: CDPClient, tabSessionId: string): Promise<void> {
		const session = this.ensureSession(sessionInfo);

		this.db.insertMarker({
			id: marker.id,
			sessionId: sessionInfo.id,
			timestamp: marker.timestamp,
			label: marker.label,
			autoDetected: marker.autoDetected,
			severity: marker.severity,
		});

		const windowStart = marker.timestamp - this.config.markerPaddingMs;
		const windowEnd = marker.timestamp + this.config.markerPaddingMs;
		const events = buffer.getEvents(windowStart, marker.timestamp);

		const newEvents = events.filter((e) => !session.persistedEventIds.has(e.id));

		if (newEvents.length > 0) {
			const offsets = session.writer.writeBatch(newEvents);
			const batch = newEvents.map((e, i) => ({
				sessionId: sessionInfo.id,
				eventId: e.id,
				timestamp: e.timestamp,
				type: e.type,
				summary: e.summary,
				detailOffset: offsets[i].offset,
				detailLength: offsets[i].length,
			}));
			this.db.insertEventBatch(batch);

			for (const e of newEvents) {
				session.persistedEventIds.add(e.id);
			}
		}

		await this.networkExtractor.extractBodies(newEvents, cdpClient, tabSessionId, resolve(session.dir, "network"), this.db, sessionInfo.id);

		if (this.screenshotCapture) {
			try {
				await this.screenshotCapture.capture(cdpClient, tabSessionId, resolve(session.dir, "screenshots"), marker.timestamp);
			} catch {
				// Screenshot capture may fail if tab is navigating or closed
			}
		}

		session.openMarkerWindows.push({
			markerId: marker.id,
			start: marker.timestamp,
			end: windowEnd,
		});

		this.db.updateSessionCounts(sessionInfo.id);
	}

	/**
	 * Called for every new event. Persists events that fall within an open marker window.
	 */
	onNewEvent(event: RecordedEvent, sessionInfo: BrowserSessionInfo): void {
		const session = this.activeSessions.get(sessionInfo.id);
		if (!session) return;

		const now = Date.now();
		session.openMarkerWindows = session.openMarkerWindows.filter((w) => w.end > now);

		if (session.openMarkerWindows.length === 0) return;

		const inWindow = session.openMarkerWindows.some((w) => event.timestamp >= w.start && event.timestamp <= w.end);

		if (inWindow && !session.persistedEventIds.has(event.id)) {
			const { offset, length } = session.writer.write(event);
			this.db.insertEvent({
				sessionId: sessionInfo.id,
				eventId: event.id,
				timestamp: event.timestamp,
				type: event.type,
				summary: event.summary,
				detailOffset: offset,
				detailLength: length,
			});
			session.persistedEventIds.add(event.id);
		}
	}

	/**
	 * Get the recording directory for a session, if it exists.
	 */
	getSessionDir(sessionId: string): string | null {
		const session = this.activeSessions.get(sessionId);
		return session?.dir ?? null;
	}

	/**
	 * Get or create the recording directory for a session.
	 * Unlike getSessionDir, this initialises the session if it doesn't exist yet.
	 */
	getOrCreateSessionDir(sessionInfo: BrowserSessionInfo): string {
		return this.ensureSession(sessionInfo).dir;
	}

	/**
	 * Run retention cleanup to remove recordings older than the retention period.
	 */
	async runRetentionCleanup(config?: Partial<RetentionConfig>): Promise<{ deleted: number }> {
		const manager = new RetentionManager(RetentionConfigSchema.parse(config ?? {}));
		return manager.cleanup(this.db);
	}

	/**
	 * End a session, flushing remaining data.
	 */
	endSession(sessionId: string): void {
		const session = this.activeSessions.get(sessionId);
		if (!session) return;
		session.writer.close();
		this.db.endSession(sessionId, Date.now());
		this.activeSessions.delete(sessionId);
	}

	close(): void {
		for (const [sessionId, session] of this.activeSessions) {
			session.writer.close();
			this.db.endSession(sessionId, Date.now());
		}
		this.activeSessions.clear();
		this.db.close();
	}

	private ensureSession(info: BrowserSessionInfo): ActiveSession {
		const existing = this.activeSessions.get(info.id);
		if (existing) return existing;

		const url = info.tabs[0]?.url ?? "unknown";
		const dirName = `${formatTimestamp(info.startedAt)}_${slugify(url)}`;
		const dir = resolve(this.config.dataDir, "recordings", dirName);
		mkdirSync(resolve(dir, "network"), { recursive: true });
		mkdirSync(resolve(dir, "screenshots"), { recursive: true });

		const writer = new EventWriter(resolve(dir, "events.jsonl"));

		this.db.createSession({
			id: info.id,
			startedAt: info.startedAt,
			tabUrl: url,
			tabTitle: info.tabs[0]?.title ?? "",
			recordingDir: dir,
		});

		const session: ActiveSession = { writer, dir, persistedEventIds: new Set(), openMarkerWindows: [] };
		this.activeSessions.set(info.id, session);
		return session;
	}
}

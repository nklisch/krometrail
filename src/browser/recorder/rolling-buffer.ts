import { z } from "zod";
import type { Marker, RecordedEvent } from "../types.js";

export const BufferConfigSchema = z.object({
	/** Max age of events in buffer, in ms. Default: 30 minutes. */
	maxAgeMs: z.number().default(30 * 60 * 1000),
	/** Seconds of context to preserve around markers. Default: 120s. */
	markerPaddingMs: z.number().default(120 * 1000),
	/** Max events in buffer (memory safety). Default: 100_000. */
	maxEvents: z.number().default(100_000),
});

export type BufferConfig = z.infer<typeof BufferConfigSchema>;

export class RollingBuffer {
	private events: RecordedEvent[] = [];
	private markers: Marker[] = [];

	constructor(private config: BufferConfig) {}

	/** Add an event to the buffer. */
	push(event: RecordedEvent): void {
		this.events.push(event);
		this.evict();
	}

	/** Place a marker at the current time. */
	placeMarker(label?: string, autoDetected = false, severity?: "low" | "medium" | "high"): Marker {
		const marker: Marker = {
			id: crypto.randomUUID(),
			timestamp: Date.now(),
			label,
			autoDetected,
			severity,
		};
		this.markers.push(marker);
		return marker;
	}

	/** Get all events within a time range. */
	getEvents(start: number, end: number): RecordedEvent[] {
		return this.events.filter((e) => e.timestamp >= start && e.timestamp <= end);
	}

	/** Get all events within the padding window of a marker. */
	getEventsAroundMarker(markerId: string): RecordedEvent[] {
		const marker = this.markers.find((m) => m.id === markerId);
		if (!marker) return [];
		return this.getEvents(marker.timestamp - this.config.markerPaddingMs, marker.timestamp + this.config.markerPaddingMs);
	}

	/** Get all markers. */
	getMarkers(): Marker[] {
		return [...this.markers];
	}

	/** Get buffer stats. */
	getStats(): { eventCount: number; markerCount: number; oldestTimestamp: number; newestTimestamp: number } {
		const oldest = this.events[0]?.timestamp ?? 0;
		const newest = this.events[this.events.length - 1]?.timestamp ?? 0;
		return {
			eventCount: this.events.length,
			markerCount: this.markers.length,
			oldestTimestamp: oldest,
			newestTimestamp: newest,
		};
	}

	/** Evict old events that aren't near any marker. */
	private evict(): void {
		const cutoff = Date.now() - this.config.maxAgeMs;

		this.events = this.events.filter((e) => {
			// Keep if within max age
			if (e.timestamp >= cutoff) return true;
			// Keep if within padding of any marker
			return this.markers.some((m) => Math.abs(e.timestamp - m.timestamp) <= this.config.markerPaddingMs);
		});

		// Also enforce max events (drop oldest first)
		while (this.events.length > this.config.maxEvents) {
			this.events.shift();
		}
	}
}

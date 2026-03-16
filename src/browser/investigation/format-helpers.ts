export function formatTime(ts: number): string {
	return new Date(ts).toISOString().slice(11, 23); // HH:mm:ss.SSS
}

/** Time window constants for marker-relative queries. */
export const MARKER_LOOKBACK_MS = 120_000;
export const MARKER_LOOKAHEAD_MS = 30_000;

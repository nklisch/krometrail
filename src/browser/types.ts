export type EventType =
	| "navigation"
	| "network_request"
	| "network_response"
	| "console"
	| "page_error"
	| "user_input"
	| "dom_mutation"
	| "form_state"
	| "screenshot"
	| "performance"
	| "websocket"
	| "storage_change"
	| "marker";

export interface RecordedEvent {
	id: string;
	timestamp: number;
	type: EventType;
	tabId: string;
	summary: string;
	data: Record<string, unknown>;
}

export interface Marker {
	id: string;
	timestamp: number;
	label?: string;
	autoDetected: boolean;
	severity?: "low" | "medium" | "high";
}

export interface BrowserSessionInfo {
	id: string;
	startedAt: number;
	tabs: Array<{ targetId: string; url: string; title: string }>;
	eventCount: number;
	markerCount: number;
	bufferAgeMs: number;
}

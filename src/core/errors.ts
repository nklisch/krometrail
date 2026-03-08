/**
 * Base error for all Agent Lens errors.
 */
export class AgentLensError extends Error {
	constructor(
		message: string,
		public readonly code: string,
	) {
		super(message);
		this.name = "AgentLensError";
	}
}

/**
 * DAP request timed out.
 */
export class DAPTimeoutError extends AgentLensError {
	constructor(
		public readonly command: string,
		public readonly timeoutMs: number,
	) {
		super(`DAP request '${command}' timed out after ${timeoutMs}ms`, "DAP_TIMEOUT");
		this.name = "DAPTimeoutError";
	}
}

/**
 * DAP client has been disposed.
 */
export class DAPClientDisposedError extends AgentLensError {
	constructor() {
		super("DAP client has been disposed", "DAP_DISPOSED");
		this.name = "DAPClientDisposedError";
	}
}

/**
 * DAP connection failed.
 */
export class DAPConnectionError extends AgentLensError {
	constructor(
		public readonly host: string,
		public readonly port: number,
		public readonly cause?: Error,
	) {
		super(`Failed to connect to DAP server at ${host}:${port}: ${cause?.message ?? "unknown error"}`, "DAP_CONNECTION_FAILED");
		this.name = "DAPConnectionError";
	}
}

/**
 * Session not found.
 */
export class SessionNotFoundError extends AgentLensError {
	constructor(public readonly sessionId: string) {
		super(`No debug session with id: ${sessionId}`, "SESSION_NOT_FOUND");
		this.name = "SessionNotFoundError";
	}
}

/**
 * Session is in an invalid state for the requested operation.
 */
export class SessionStateError extends AgentLensError {
	constructor(
		public readonly sessionId: string,
		public readonly currentState: string,
		public readonly expectedStates: string[],
	) {
		super(`Session ${sessionId} is '${currentState}', expected one of: ${expectedStates.join(", ")}`, "SESSION_INVALID_STATE");
		this.name = "SessionStateError";
	}
}

/**
 * Session resource limit exceeded.
 */
export class SessionLimitError extends AgentLensError {
	constructor(
		public readonly limitName: string,
		public readonly currentValue: number,
		public readonly maxValue: number,
		public readonly suggestion?: string,
	) {
		super(`Session limit '${limitName}' exceeded: ${currentValue}/${maxValue}. ${suggestion ?? ""}`, "SESSION_LIMIT_EXCEEDED");
		this.name = "SessionLimitError";
	}
}

/**
 * Adapter prerequisites not met.
 */
export class AdapterPrerequisiteError extends AgentLensError {
	constructor(
		public readonly adapterId: string,
		public readonly missing: string[],
		public readonly installHint?: string,
	) {
		super(`Adapter '${adapterId}' prerequisites not met: ${missing.join(", ")}. ${installHint ? `Install: ${installHint}` : ""}`, "ADAPTER_PREREQUISITES");
		this.name = "AdapterPrerequisiteError";
	}
}

/**
 * No adapter found for the given language or file extension.
 */
export class AdapterNotFoundError extends AgentLensError {
	constructor(public readonly languageOrExt: string) {
		super(`No debug adapter found for '${languageOrExt}'. Available adapters can be checked with debug_status.`, "ADAPTER_NOT_FOUND");
		this.name = "AdapterNotFoundError";
	}
}

/**
 * Debugee process launch failed.
 */
export class LaunchError extends AgentLensError {
	constructor(
		message: string,
		public readonly stderr?: string,
	) {
		super(message, "LAUNCH_FAILED");
		this.name = "LaunchError";
	}
}

/**
 * Chrome executable not found.
 */
export class ChromeNotFoundError extends AgentLensError {
	constructor() {
		super("Chrome not found. Install Chrome or use --attach to connect to an existing instance.", "CHROME_NOT_FOUND");
		this.name = "ChromeNotFoundError";
	}
}

/**
 * Chrome CDP connection failed (WebSocket or HTTP endpoint unavailable).
 */
export class CDPConnectionError extends AgentLensError {
	constructor(
		message: string,
		public readonly cause?: Error,
	) {
		super(message, "CDP_CONNECTION_FAILED");
		this.name = "CDPConnectionError";
	}
}

/**
 * Browser tab not found by targetId.
 */
export class TabNotFoundError extends AgentLensError {
	constructor(public readonly targetId: string) {
		super(`Tab not found: ${targetId}`, "TAB_NOT_FOUND");
		this.name = "TabNotFoundError";
	}
}

/**
 * Browser recorder is in an invalid state for the requested operation.
 */
export class BrowserRecorderStateError extends AgentLensError {
	constructor(message: string) {
		super(message, "BROWSER_RECORDER_STATE");
		this.name = "BrowserRecorderStateError";
	}
}

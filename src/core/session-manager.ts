import type {
	Breakpoint,
	ResourceLimits,
	SessionInfo,
	ViewportConfig,
	ViewportSnapshot,
} from "./types.js";

export interface LaunchOptions {
	command: string;
	language?: string;
	breakpoints?: Breakpoint[];
	cwd?: string;
	env?: Record<string, string>;
	viewportConfig?: Partial<ViewportConfig>;
	stopOnEntry?: boolean;
}

/**
 * Manages debug sessions: lifecycle, DAP communication, viewport rendering,
 * and resource limit enforcement.
 */
export class SessionManager {
	private sessions = new Map<string, DebugSession>();

	constructor(private limits: ResourceLimits) {}

	async launch(
		_options: LaunchOptions,
	): Promise<{ sessionId: string; viewport?: ViewportSnapshot }> {
		// TODO: select adapter, launch debugee, set breakpoints, return session
		throw new Error("Not implemented");
	}

	async stop(sessionId: string): Promise<{ duration: number; actionCount: number }> {
		const session = this.getSession(sessionId);
		// TODO: terminate debugee, clean up
		void session;
		throw new Error("Not implemented");
	}

	getInfo(sessionId: string): SessionInfo {
		const session = this.getSession(sessionId);
		return session.info;
	}

	private getSession(sessionId: string): DebugSession {
		const session = this.sessions.get(sessionId);
		if (!session) {
			throw new Error(`No session with id: ${sessionId}`);
		}
		return session;
	}
}

interface DebugSession {
	info: SessionInfo;
	// TODO: DAP client, adapter, viewport config, action log
}

import type { ChildProcess } from "node:child_process";
import type { Readable, Writable } from "node:stream";

export interface PrerequisiteResult {
	satisfied: boolean;
	missing?: string[];
	installHint?: string;
}

export interface DAPConnection {
	reader: Readable;
	writer: Writable;
	process?: ChildProcess;
}

export interface LaunchConfig {
	command: string;
	cwd?: string;
	env?: Record<string, string>;
	args?: string[];
	port?: number;
}

export interface AttachConfig {
	pid?: number;
	port?: number;
	host?: string;
}

export interface DebugAdapter {
	/** Unique identifier, e.g., "python", "node", "go" */
	id: string;

	/** File extensions this adapter handles */
	fileExtensions: string[];

	/** Human-readable name for error messages */
	displayName: string;

	/** Check if the adapter's debugger is available on this system */
	checkPrerequisites(): Promise<PrerequisiteResult>;

	/** Launch the debugee and return a DAP connection */
	launch(config: LaunchConfig): Promise<DAPConnection>;

	/** Attach to an already-running process */
	attach(config: AttachConfig): Promise<DAPConnection>;

	/** Clean up adapter-specific resources */
	dispose(): Promise<void>;
}

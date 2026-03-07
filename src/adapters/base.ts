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
	/**
	 * Adapter-specific arguments to pass in the DAP launch request.
	 * Merged with the session manager's default launch args.
	 * Used by adapters like Go/Delve that need mode/program/args in the DAP launch.
	 */
	launchArgs?: Record<string, unknown>;
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
	env?: Record<string, string>;
}

export interface DebugAdapter {
	/** Unique identifier, e.g., "python", "node", "go" */
	id: string;

	/** File extensions this adapter handles */
	fileExtensions: string[];

	/** Alternative language names that map to this adapter, e.g., ["javascript", "typescript", "ts", "js"] */
	aliases?: string[];

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

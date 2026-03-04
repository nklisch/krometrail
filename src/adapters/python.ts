import type {
	AttachConfig,
	DAPConnection,
	DebugAdapter,
	LaunchConfig,
	PrerequisiteResult,
} from "./base.js";

export class PythonAdapter implements DebugAdapter {
	id = "python";
	fileExtensions = [".py"];
	displayName = "Python (debugpy)";

	async checkPrerequisites(): Promise<PrerequisiteResult> {
		// TODO: check for python3 and debugpy
		throw new Error("Not implemented");
	}

	async launch(_config: LaunchConfig): Promise<DAPConnection> {
		// TODO: launch python -m debugpy --listen 0:PORT --wait-for-client <script>
		throw new Error("Not implemented");
	}

	async attach(_config: AttachConfig): Promise<DAPConnection> {
		// TODO: connect to running debugpy instance
		throw new Error("Not implemented");
	}

	async dispose(): Promise<void> {
		// TODO: clean up child processes
	}
}

import type { DebugAdapter } from "./base.js";

const adapters = new Map<string, DebugAdapter>();

export function registerAdapter(adapter: DebugAdapter): void {
	adapters.set(adapter.id, adapter);
	for (const ext of adapter.fileExtensions) {
		adapters.set(ext, adapter);
	}
}

export function getAdapter(idOrExtension: string): DebugAdapter | undefined {
	return adapters.get(idOrExtension);
}

export function getAdapterForFile(filePath: string): DebugAdapter | undefined {
	const ext = `.${filePath.split(".").pop()}`;
	return adapters.get(ext);
}

export function listAdapters(): DebugAdapter[] {
	const seen = new Set<string>();
	const result: DebugAdapter[] = [];
	for (const adapter of adapters.values()) {
		if (!seen.has(adapter.id)) {
			seen.add(adapter.id);
			result.push(adapter);
		}
	}
	return result;
}

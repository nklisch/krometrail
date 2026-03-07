import type { DebugAdapter } from "./base.js";
import { CppAdapter } from "./cpp.js";
import { CSharpAdapter } from "./csharp.js";
import { GoAdapter } from "./go.js";
import { JavaAdapter } from "./java.js";
import { KotlinAdapter } from "./kotlin.js";
import { NodeAdapter } from "./node.js";
import { PythonAdapter } from "./python.js";
import { RubyAdapter } from "./ruby.js";
import { RustAdapter } from "./rust.js";
import { SwiftAdapter } from "./swift.js";

const adapters = new Map<string, DebugAdapter>();

export function registerAdapter(adapter: DebugAdapter): void {
	adapters.set(adapter.id, adapter);
	for (const ext of adapter.fileExtensions) {
		adapters.set(ext, adapter);
	}
	for (const alias of adapter.aliases ?? []) {
		adapters.set(alias, adapter);
	}
}

export function getAdapter(idOrExtension: string): DebugAdapter | undefined {
	return adapters.get(idOrExtension);
}

export function getAdapterForFile(filePath: string): DebugAdapter | undefined {
	const ext = `.${filePath.split(".").pop()}`;
	return adapters.get(ext);
}

/**
 * Register the default set of language adapters (Python, Node.js, Go).
 * Call this once at startup in each entry point.
 */
export function registerAllAdapters(): void {
	registerAdapter(new PythonAdapter());
	registerAdapter(new NodeAdapter());
	registerAdapter(new GoAdapter());
	registerAdapter(new RustAdapter());
	registerAdapter(new JavaAdapter());
	registerAdapter(new CppAdapter());
	registerAdapter(new RubyAdapter());
	registerAdapter(new CSharpAdapter());
	registerAdapter(new SwiftAdapter());
	registerAdapter(new KotlinAdapter());
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

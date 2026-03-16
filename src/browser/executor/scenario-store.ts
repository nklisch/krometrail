import type { Step } from "./types.js";

export interface SavedScenario {
	name: string;
	steps: Step[];
	savedAt: number;
}

export class ScenarioStore {
	private scenarios = new Map<string, SavedScenario>();

	save(name: string, steps: Step[]): void {
		this.scenarios.set(name, { name, steps, savedAt: Date.now() });
	}

	get(name: string): SavedScenario | undefined {
		return this.scenarios.get(name);
	}

	list(): SavedScenario[] {
		return [...this.scenarios.values()];
	}

	delete(name: string): boolean {
		return this.scenarios.delete(name);
	}

	clear(): void {
		this.scenarios.clear();
	}
}

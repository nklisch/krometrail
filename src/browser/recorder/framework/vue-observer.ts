import { buildVueInjectionScript } from "./vue-injection.js";

export interface VueObserverConfig {
	/** Max framework events per second reported via __BL__. Default: 10. */
	maxEventsPerSecond?: number;
	/** Max depth for state/props serialization. Default: 3. */
	maxSerializationDepth?: number;
	/** Component updates in 2s window before infinite loop warning. Default: 30. */
	infiniteLoopThreshold?: number;
	/** Max components visited per event batch (safety cap). Default: 5000. */
	maxComponentsPerBatch?: number;
	/** Max queued events before overflow (oldest dropped). Default: 1000. */
	maxQueueSize?: number;
	/** Enable Pinia/Vuex store observation. Default: true. */
	storeObservation?: boolean;
	/** Interval in ms for lazy store discovery polling. Default: 5000. */
	storeDiscoveryIntervalMs?: number;
}

/**
 * Manages the Vue 3 state observation injection script.
 * Instantiated by FrameworkTracker when "vue" is in the enabled frameworks.
 */
export class VueObserver {
	private config: Required<VueObserverConfig>;

	constructor(config: VueObserverConfig = {}) {
		this.config = {
			maxEventsPerSecond: config.maxEventsPerSecond ?? 10,
			maxSerializationDepth: config.maxSerializationDepth ?? 3,
			infiniteLoopThreshold: config.infiniteLoopThreshold ?? 30,
			maxComponentsPerBatch: config.maxComponentsPerBatch ?? 5000,
			maxQueueSize: config.maxQueueSize ?? 1000,
			storeObservation: config.storeObservation ?? true,
			storeDiscoveryIntervalMs: config.storeDiscoveryIntervalMs ?? 5000,
		};
	}

	/**
	 * Returns the injection script IIFE string.
	 * This script hooks into __VUE_DEVTOOLS_GLOBAL_HOOK__ (installed by detector.ts)
	 * to observe component lifecycle events and report state changes via __BL__.
	 */
	getInjectionScript(): string {
		return buildVueInjectionScript(this.config);
	}
}

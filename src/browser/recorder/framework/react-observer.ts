import { buildReactInjectionScript } from "./react-injection.js";

export interface ReactObserverConfig {
	/** Max framework events per second reported via __BL__. Default: 10. */
	maxEventsPerSecond?: number;
	/** Max depth for state/props serialization. Default: 3. */
	maxSerializationDepth?: number;
	/** Renders with unchanged deps before stale closure warning. Default: 5. */
	staleClosureThreshold?: number;
	/** Renders in 1s window before infinite loop warning. Default: 15. */
	infiniteRerenderThreshold?: number;
	/** Context consumers before excessive re-render warning. Default: 20. */
	contextRerenderThreshold?: number;
	/** Max fibers visited per commit (safety cap). Default: 5000. */
	maxFibersPerCommit?: number;
	/** Max queued events before overflow (oldest dropped). Default: 1000. */
	maxQueueSize?: number;
}

/**
 * Manages the React state observation injection script.
 * Instantiated by FrameworkTracker when "react" is in the enabled frameworks.
 */
export class ReactObserver {
	private config: Required<ReactObserverConfig>;

	constructor(config: ReactObserverConfig = {}) {
		this.config = {
			maxEventsPerSecond: config.maxEventsPerSecond ?? 10,
			maxSerializationDepth: config.maxSerializationDepth ?? 3,
			staleClosureThreshold: config.staleClosureThreshold ?? 5,
			infiniteRerenderThreshold: config.infiniteRerenderThreshold ?? 15,
			contextRerenderThreshold: config.contextRerenderThreshold ?? 20,
			maxFibersPerCommit: config.maxFibersPerCommit ?? 5000,
			maxQueueSize: config.maxQueueSize ?? 1000,
		};
	}

	/**
	 * Returns the injection script IIFE string.
	 * This script patches __REACT_DEVTOOLS_GLOBAL_HOOK__ (installed by detector.ts)
	 * to observe fiber commits and report state changes via __BL__.
	 */
	getInjectionScript(): string {
		return buildReactInjectionScript(this.config);
	}
}

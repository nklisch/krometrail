import React, { useEffect, useState } from "react";

declare global {
	interface Window {
		__TEST_CONTROLS__: Record<string, () => void>;
	}
}

export function InfiniteUpdater() {
	const [count, setCount] = useState(0);
	const [active, setActive] = useState(false);

	// Expose test control
	useEffect(() => {
		window.__TEST_CONTROLS__ = window.__TEST_CONTROLS__ || {};
		window.__TEST_CONTROLS__.activateInfiniteUpdate = () => setActive(true);
	}, []);

	// Intentional bug: count is in deps and is mutated — infinite loop
	useEffect(() => {
		if (!active) return;
		setCount(count + 1);
	}, [count, active]);

	return (
		<div data-testid="infinite-updater">
			<p>Count: {count}</p>
			<p>Active: {String(active)}</p>
		</div>
	);
}

import React, { useEffect, useState } from "react";

export function LeakyInterval() {
	const [tick, setTick] = useState(0);
	const [active, setActive] = useState(false);
	const [mountCount, setMountCount] = useState(0);

	// Expose test control
	useEffect(() => {
		window.__TEST_CONTROLS__ = window.__TEST_CONTROLS__ || {};
		window.__TEST_CONTROLS__.activateLeakyInterval = () => {
			setActive(true);
			// Force re-mount by toggling mount count
			setMountCount((n) => n + 1);
		};
	}, []);

	return (
		<div data-testid="leaky-interval">
			<p>Tick: {tick}</p>
			<p>Active: {String(active)}</p>
			{active && <LeakyIntervalInner key={mountCount} onTick={() => setTick((n) => n + 1)} />}
		</div>
	);
}

function LeakyIntervalInner({ onTick }: { onTick: () => void }) {
	// Intentional bug: setInterval without cleanup
	useEffect(() => {
		setInterval(onTick, 500); // No return cleanup!
	}, [onTick]);

	return <span data-testid="leaky-inner">interval running</span>;
}

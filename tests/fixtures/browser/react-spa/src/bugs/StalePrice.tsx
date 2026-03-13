import React, { useCallback, useEffect, useState } from "react";
import { useStore } from "../store.js";

export function StalePrice() {
	const items = useStore((s) => s.items);
	const addToCart = useStore((s) => s.addToCart);
	const [showStale, setShowStale] = useState(false);

	// Expose test control
	useEffect(() => {
		window.__TEST_CONTROLS__ = window.__TEST_CONTROLS__ || {};
		window.__TEST_CONTROLS__.showStalePrice = () => setShowStale(true);
	}, []);

	// Intentional bug: stale closure — captures total at creation time with empty deps
	// biome-ignore lint/correctness/useExhaustiveDependencies: intentional stale closure bug
	const getStaleTotal = useCallback(() => {
		return items.reduce((sum, i) => sum + i.price * i.quantity, 0);
	}, []); // empty deps = stale closure over initial items

	const realTotal = items.reduce((sum, i) => sum + i.price * i.quantity, 0);

	return (
		<div data-testid="stale-price">
			<p>Real total: ${realTotal.toFixed(2)}</p>
			{showStale && <p data-testid="stale-total">Stale total: ${getStaleTotal().toFixed(2)}</p>}
			<button
				type="button"
				data-testid="add-product"
				onClick={() => addToCart({ id: 99, name: "Test Product", price: 10.0 })}
			>
				Add $10 Item
			</button>
		</div>
	);
}

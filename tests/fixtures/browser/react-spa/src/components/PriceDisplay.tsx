import React from "react";

interface Props {
	price: number;
}

// Intentional: this component is simple and correct — stale closure bugs
// are demonstrated in bugs/StalePrice.tsx instead.
export function PriceDisplay({ price }: Props) {
	return <span data-testid="price-display">${price.toFixed(2)}</span>;
}

import React from "react";
import type { CartItem as CartItemType } from "../store.js";
import { useStore } from "../store.js";

interface Props {
	item: CartItemType;
}

export function CartItem({ item }: Props) {
	const updateQuantity = useStore((s) => s.updateQuantity);
	const removeFromCart = useStore((s) => s.removeFromCart);

	return (
		<div data-testid={`cart-item-${item.productId}`} style={{ display: "flex", gap: "1rem", alignItems: "center", padding: "0.5rem", borderBottom: "1px solid #eee" }}>
			<span data-testid={`cart-item-name-${item.productId}`}>{item.name}</span>
			<span data-testid={`cart-item-price-${item.productId}`}>${item.price.toFixed(2)}</span>
			<button type="button" data-testid={`quantity-decrease-${item.productId}`} onClick={() => updateQuantity(item.productId, item.quantity - 1)}>
				-
			</button>
			<span data-testid={`cart-item-qty-${item.productId}`}>{item.quantity}</span>
			<button type="button" data-testid={`quantity-increase-${item.productId}`} onClick={() => updateQuantity(item.productId, item.quantity + 1)}>
				+
			</button>
			<button type="button" data-testid={`remove-item-${item.productId}`} onClick={() => removeFromCart(item.productId)}>
				Remove
			</button>
		</div>
	);
}

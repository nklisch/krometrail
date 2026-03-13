import React from "react";
import { Link } from "react-router-dom";
import { CartItem } from "../components/CartItem.js";
import { useStore } from "../store.js";

export function Cart() {
	const items = useStore((s) => s.items);
	const total = items.reduce((sum, i) => sum + i.price * i.quantity, 0);

	if (items.length === 0) {
		return (
			<div data-testid="cart-page">
				<h1>Cart</h1>
				<p data-testid="cart-empty">Your cart is empty.</p>
				<Link to="/" data-testid="continue-shopping">
					Continue Shopping
				</Link>
			</div>
		);
	}

	return (
		<div data-testid="cart-page">
			<h1>Cart</h1>
			<div data-testid="cart-items">
				{items.map((item) => (
					<CartItem key={item.productId} item={item} />
				))}
			</div>
			<div data-testid="cart-total" style={{ fontWeight: "bold", marginTop: "1rem" }}>
				Total: ${total.toFixed(2)}
			</div>
			<Link to="/checkout" data-testid="checkout-button">
				<button type="button">Checkout</button>
			</Link>
		</div>
	);
}

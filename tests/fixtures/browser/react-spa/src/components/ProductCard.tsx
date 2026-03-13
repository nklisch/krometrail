import React from "react";
import { Link } from "react-router-dom";
import type { Product } from "../api.js";
import { useStore } from "../store.js";

interface Props {
	product: Product;
}

export function ProductCard({ product }: Props) {
	const addToCart = useStore((s) => s.addToCart);
	return (
		<div data-testid={`product-card-${product.id}`} style={{ border: "1px solid #ccc", padding: "1rem", borderRadius: "4px" }}>
			<h3>{product.name}</h3>
			<p data-testid={`product-price-${product.id}`}>${product.price.toFixed(2)}</p>
			<Link to={`/product/${product.id}`} data-testid={`product-link-${product.id}`}>
				View
			</Link>
			<button
				type="button"
				data-testid="add-to-cart"
				onClick={() => addToCart(product)}
				style={{ marginLeft: "0.5rem" }}
			>
				Add to Cart
			</button>
		</div>
	);
}

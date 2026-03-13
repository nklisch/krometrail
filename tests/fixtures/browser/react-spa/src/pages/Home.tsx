import React, { useEffect, useState } from "react";
import { ProductCard } from "../components/ProductCard.js";
import type { Product } from "../api.js";
import { fetchProducts } from "../api.js";

export function Home() {
	const [products, setProducts] = useState<Product[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		fetchProducts()
			.then((p) => {
				setProducts(p);
				setLoading(false);
			})
			.catch((e) => {
				setError(e.message);
				setLoading(false);
			});
	}, []);

	if (loading) return <div data-testid="loading">Loading products...</div>;
	if (error) return <div data-testid="error">Error: {error}</div>;

	return (
		<div data-testid="home-page">
			<h1>Products</h1>
			<div data-testid="product-grid" style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "1rem" }}>
				{products.map((p) => (
					<ProductCard key={p.id} product={p} />
				))}
			</div>
		</div>
	);
}

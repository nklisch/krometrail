import React, { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import type { ProductDetail as ProductDetailType } from "../api.js";
import { fetchProduct } from "../api.js";
import { useStore } from "../store.js";

export function ProductDetail() {
	const { id } = useParams<{ id: string }>();
	const [product, setProduct] = useState<ProductDetailType | null>(null);
	const [loading, setLoading] = useState(true);
	const addToCart = useStore((s) => s.addToCart);

	useEffect(() => {
		if (!id) return;
		fetchProduct(Number(id))
			.then((p) => {
				setProduct(p);
				setLoading(false);
			})
			.catch(() => setLoading(false));
	}, [id]);

	if (loading) return <div data-testid="loading">Loading...</div>;
	if (!product) return <div data-testid="not-found">Product not found</div>;

	return (
		<div data-testid="product-detail-page">
			<h1 data-testid="product-name">{product.name}</h1>
			<p data-testid="product-price">${product.price.toFixed(2)}</p>
			<p data-testid="product-description">{product.description}</p>
			<button type="button" data-testid="add-to-cart" onClick={() => addToCart(product)}>
				Add to Cart
			</button>
			<section data-testid="reviews">
				<h2>Reviews</h2>
				{product.reviews.map((r) => (
					<div key={r.id} data-testid={`review-${r.id}`}>
						<strong>{r.author}</strong> — {r.rating}/5
						<p>{r.text}</p>
					</div>
				))}
			</section>
		</div>
	);
}

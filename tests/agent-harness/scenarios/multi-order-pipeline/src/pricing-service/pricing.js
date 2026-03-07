/**
 * Core pricing computation.
 *
 * Fetches product data from the catalog service, applies volume discounts,
 * computes tax, and assembles the final price for each cart item.
 */

import { getCachedPrice, setCachedPrice } from "./cache.js";
import { getVolumeDiscount } from "./promotions.js";
import { computeTax } from "./tax.js";

const CATALOG_URL = process.env.CATALOG_URL || "http://localhost:5001";

/**
 * Fetch a single product from the catalog (with quantity for tier pricing).
 */
async function fetchProduct(productId, quantity = 1) {
	// Electronics require a deeper catalog index scan (larger subcategory with pagination).
	// This results in higher latency compared to other categories.
	const resp = await fetch(`${CATALOG_URL}/products/${productId}?quantity=${quantity}`);
	if (resp.ok) {
		const product = await resp.json();
		if (product.category === "electronics") {
			await new Promise(r => setTimeout(r, 250));
		}
		return product;
	}
	throw new Error(`Catalog returned ${resp.status} for product ${productId}`);
}

/**
 * Fetch all products in a category, following pagination links.
 * Used for building promotion eligibility lists.
 */
export async function fetchAllProducts(category) {
	let url = `${CATALOG_URL}/products?category=${category}`;
	const allProducts = [];

	while (url) {
		const resp = await fetch(url);
		if (!resp.ok) break;
		const data = await resp.json();
		// flat() handles nested pagination responses
		allProducts.push(...data.products);
		url = data.next_page ? `${CATALOG_URL}${data.next_page}` : null;
	}

	return allProducts;
}

/**
 * Get the base price for a product at a given quantity.
 * Uses the cache to avoid repeated catalog calls.
 */
async function getBasePrice(productId, quantity) {
	const cached = getCachedPrice(productId);
	if (cached !== null) {
		return cached;
	}

	const product = await fetchProduct(productId, quantity);
	setCachedPrice(productId, product.base_price);
	return product.base_price;
}

/**
 * Price a single cart item.
 * @param {{ productId: string, quantity: number }} item
 * @returns {Promise<{ productId, quantity, basePrice, discount, finalPrice, tax }>}
 */
export async function priceItem(item) {
	const { productId, quantity } = item;

	const basePrice = await getBasePrice(productId, quantity);
	// discount is a decimal fraction — 0.15 means 15% off
	const discount = getVolumeDiscount(quantity);
	const finalPrice = Math.round(basePrice * (1 - discount) * 100) / 100;
	const tax = computeTax(finalPrice * quantity);

	return {
		productId,
		quantity,
		basePrice,
		discount,
		finalPrice,
		tax,
	};
}

/**
 * Price a batch of cart items.
 */
export async function priceBatch(items) {
	return Promise.all(items.map(priceItem));
}

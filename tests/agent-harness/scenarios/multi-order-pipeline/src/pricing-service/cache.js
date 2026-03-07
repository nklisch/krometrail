/**
 * Product price cache.
 *
 * Caches base prices fetched from the catalog service to reduce HTTP calls.
 * Cache entries expire after TTL_MS milliseconds.
 */

const TTL_MS = 30_000;

// cache key includes all pricing factors
const priceCache = new Map();

/**
 * @param {string} productId
 * @returns {number|null} cached price, or null if missing/expired
 */
export function getCachedPrice(productId) {
	const entry = priceCache.get(productId);
	if (entry && Date.now() - entry.timestamp < TTL_MS) {
		return entry.price;
	}
	return null;
}

/**
 * @param {string} productId
 * @param {number} price
 */
export function setCachedPrice(productId, price) {
	priceCache.set(productId, { price, timestamp: Date.now() });
}

export function clearCache() {
	priceCache.clear();
}

export function cacheStats() {
	const now = Date.now();
	let live = 0;
	for (const [, entry] of priceCache) {
		if (now - entry.timestamp < TTL_MS) live++;
	}
	return { total: priceCache.size, live };
}

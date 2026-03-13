export interface Product {
	id: number;
	name: string;
	price: number;
	image: string;
}

export interface ProductDetail extends Product {
	description: string;
	reviews: Array<{ id: number; author: string; rating: number; text: string }>;
}

export async function fetchProducts(): Promise<Product[]> {
	const res = await fetch("/api/products");
	if (!res.ok) throw new Error("Failed to fetch products");
	return res.json();
}

export async function fetchProduct(id: number): Promise<ProductDetail> {
	const res = await fetch(`/api/products/${id}`);
	if (!res.ok) throw new Error(`Failed to fetch product ${id}`);
	return res.json();
}

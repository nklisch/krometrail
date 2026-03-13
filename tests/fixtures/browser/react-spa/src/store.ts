import { create } from "zustand";

export interface CartItem {
	productId: number;
	name: string;
	price: number;
	quantity: number;
}

export interface AppState {
	// Auth
	token: string | null;
	user: { id: number; name: string } | null;
	login: (username: string, password: string) => Promise<void>;
	logout: () => void;

	// Cart
	items: CartItem[];
	addToCart: (product: { id: number; name: string; price: number }) => void;
	updateQuantity: (productId: number, quantity: number) => void;
	removeFromCart: (productId: number) => void;
	clearCart: () => void;

	// Checkout
	shippingAddress: Record<string, string> | null;
	setShippingAddress: (address: Record<string, string>) => void;
	submitOrder: () => Promise<{ orderId: string }>;
}

export const useStore = create<AppState>((set, get) => ({
	// Auth
	token: null,
	user: null,

	login: async (username: string, password: string) => {
		const res = await fetch("/api/login", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ username, password }),
		});
		if (!res.ok) throw new Error("Login failed");
		const data = await res.json();
		set({ token: data.token, user: data.user });
	},

	logout: () => set({ token: null, user: null }),

	// Cart
	items: [],

	addToCart: (product) => {
		const items = get().items;
		const existing = items.find((i) => i.productId === product.id);
		if (existing) {
			set({ items: items.map((i) => (i.productId === product.id ? { ...i, quantity: i.quantity + 1 } : i)) });
		} else {
			set({ items: [...items, { productId: product.id, name: product.name, price: product.price, quantity: 1 }] });
		}
	},

	updateQuantity: (productId, quantity) => {
		if (quantity <= 0) {
			get().removeFromCart(productId);
			return;
		}
		set({ items: get().items.map((i) => (i.productId === productId ? { ...i, quantity } : i)) });
	},

	removeFromCart: (productId) => {
		set({ items: get().items.filter((i) => i.productId !== productId) });
	},

	clearCart: () => set({ items: [] }),

	// Checkout
	shippingAddress: null,

	setShippingAddress: (address) => set({ shippingAddress: address }),

	submitOrder: async () => {
		const { items, shippingAddress } = get();
		const res = await fetch("/api/checkout", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ items, shippingAddress }),
		});
		if (!res.ok) {
			const err = await res.json().catch(() => ({ message: "Checkout failed" }));
			throw new Error(err.message || `Checkout failed: ${res.status}`);
		}
		const data = await res.json();
		get().clearCart();
		return data;
	},
}));

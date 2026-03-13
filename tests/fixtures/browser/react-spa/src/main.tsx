import React from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Navbar } from "./components/Navbar.js";
import { InfiniteUpdater } from "./bugs/InfiniteUpdater.js";
import { StalePrice } from "./bugs/StalePrice.js";
import { LeakyInterval } from "./bugs/LeakyInterval.js";
import { ContextFlood } from "./bugs/ContextFlood.js";
import { Cart } from "./pages/Cart.js";
import { Checkout } from "./pages/Checkout.js";
import { Home } from "./pages/Home.js";
import { Login } from "./pages/Login.js";
import { ProductDetail } from "./pages/ProductDetail.js";

const BUG_COMPONENTS: Record<string, React.FC> = {
	"infinite-updater": InfiniteUpdater,
	"stale-price": StalePrice,
	"leaky-interval": LeakyInterval,
	"context-flood": ContextFlood,
};

function BugRoute() {
	const name = window.location.pathname.split("/bugs/")[1] ?? "";
	const Component = BUG_COMPONENTS[name];
	if (!Component) return <div>Unknown bug: {name}</div>;
	return <Component />;
}

function App() {
	return (
		<BrowserRouter>
			<Navbar />
			<main style={{ padding: "1rem" }}>
				<Routes>
					<Route path="/" element={<Home />} />
					<Route path="/product/:id" element={<ProductDetail />} />
					<Route path="/cart" element={<Cart />} />
					<Route path="/checkout" element={<Checkout />} />
					<Route path="/login" element={<Login />} />
					<Route path="/bugs/:name" element={<BugRoute />} />
				</Routes>
			</main>
		</BrowserRouter>
	);
}

const root = createRoot(document.getElementById("root")!);
root.render(<App />);

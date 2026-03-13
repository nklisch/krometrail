import React from "react";
import { Link } from "react-router-dom";
import { useStore } from "../store.js";

export function Navbar() {
	const items = useStore((s) => s.items);
	const user = useStore((s) => s.user);
	const logout = useStore((s) => s.logout);
	const itemCount = items.reduce((sum, i) => sum + i.quantity, 0);

	return (
		<nav data-testid="navbar" style={{ display: "flex", gap: "1rem", padding: "0.5rem 1rem", background: "#333", color: "#fff" }}>
			<Link to="/" style={{ color: "#fff" }} data-testid="nav-home">
				Shop
			</Link>
			<Link to="/cart" style={{ color: "#fff" }} data-testid="nav-cart">
				Cart <span data-testid="cart-badge">({itemCount})</span>
			</Link>
			{user ? (
				<>
					<span data-testid="nav-user">{user.name}</span>
					<button type="button" onClick={logout} data-testid="nav-logout" style={{ color: "#fff", background: "none", border: "none", cursor: "pointer" }}>
						Logout
					</button>
				</>
			) : (
				<Link to="/login" style={{ color: "#fff" }} data-testid="nav-login">
					Login
				</Link>
			)}
		</nav>
	);
}

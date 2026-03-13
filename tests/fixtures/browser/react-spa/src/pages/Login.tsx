import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useStore } from "../store.js";

export function Login() {
	const [username, setUsername] = useState("");
	const [password, setPassword] = useState("");
	const [error, setError] = useState<string | null>(null);
	const login = useStore((s) => s.login);
	const navigate = useNavigate();

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		try {
			await login(username, password);
			navigate("/");
		} catch {
			setError("Invalid credentials");
		}
	};

	return (
		<div data-testid="login-page">
			<h1>Login</h1>
			{error && <div data-testid="login-error" style={{ color: "red" }}>{error}</div>}
			<form data-testid="login-form" onSubmit={handleSubmit}>
				<input data-testid="username" placeholder="Username" value={username} onChange={(e) => setUsername(e.target.value)} />
				<input data-testid="password" type="password" placeholder="Password" value={password} onChange={(e) => setPassword(e.target.value)} />
				<button type="submit" data-testid="login-submit">
					Login
				</button>
			</form>
		</div>
	);
}

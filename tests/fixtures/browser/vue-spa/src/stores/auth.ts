import { defineStore } from "pinia";
import { ref, computed } from "vue";

export const useAuthStore = defineStore("auth", () => {
	const token = ref<string | null>(null);
	const user = ref<{ id: number; name: string } | null>(null);

	const isAuthenticated = computed(() => token.value !== null);

	async function login(username: string, password: string) {
		const res = await fetch("/api/login", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ username, password }),
		});
		if (!res.ok) throw new Error("Login failed");
		const data = await res.json();
		token.value = data.token;
		user.value = data.user;
	}

	function logout() {
		token.value = null;
		user.value = null;
	}

	return { token, user, isAuthenticated, login, logout };
});

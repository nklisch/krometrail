import { ref } from "vue";

export function useApi<T>(fn: (...args: unknown[]) => Promise<T>) {
	const loading = ref(false);
	const error = ref<string | null>(null);
	const data = ref<T | null>(null);

	async function execute(...args: unknown[]) {
		loading.value = true;
		error.value = null;
		try {
			data.value = await fn(...args);
		} catch (e) {
			error.value = e instanceof Error ? e.message : "Unknown error";
		} finally {
			loading.value = false;
		}
	}

	return { loading, error, data, execute };
}

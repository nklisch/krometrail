import { defineStore } from "pinia";
import { ref, computed } from "vue";

export interface Task {
	id: number;
	title: string;
	description: string;
	status: "todo" | "in-progress" | "done";
	priority: "low" | "medium" | "high";
	assignee: string;
	dueDate: string;
	comments: Array<{ id: number; text: string; author: string; createdAt: string }>;
}

export const useTaskStore = defineStore("tasks", () => {
	const tasks = ref<Task[]>([]);
	const filter = ref<{ status: string | null; priority: string | null; search: string }>({
		status: null,
		priority: null,
		search: "",
	});

	const filteredTasks = computed(() => {
		return tasks.value.filter((t) => {
			if (filter.value.status && t.status !== filter.value.status) return false;
			if (filter.value.priority && t.priority !== filter.value.priority) return false;
			if (filter.value.search && !t.title.toLowerCase().includes(filter.value.search.toLowerCase())) return false;
			return true;
		});
	});

	const stats = computed(() => {
		const now = new Date().toISOString().split("T")[0];
		return {
			total: tasks.value.length,
			completed: tasks.value.filter((t) => t.status === "done").length,
			overdue: tasks.value.filter((t) => t.dueDate < now && t.status !== "done").length,
			byPriority: {
				low: tasks.value.filter((t) => t.priority === "low").length,
				medium: tasks.value.filter((t) => t.priority === "medium").length,
				high: tasks.value.filter((t) => t.priority === "high").length,
			},
		};
	});

	async function fetchTasks() {
		const res = await fetch("/api/tasks");
		if (!res.ok) throw new Error("Failed to fetch tasks");
		tasks.value = await res.json();
	}

	async function createTask(data: Omit<Task, "id" | "comments">) {
		const res = await fetch("/api/tasks", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(data),
		});
		if (!res.ok) {
			const err = await res.json().catch(() => ({ message: "Create failed" }));
			throw new Error(err.message || `Create failed: ${res.status}`);
		}
		const task = await res.json();
		tasks.value.push(task);
		return task;
	}

	async function updateTask(id: number, patch: Partial<Task>) {
		const res = await fetch(`/api/tasks/${id}`, {
			method: "PATCH",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(patch),
		});
		if (!res.ok) throw new Error("Update failed");
		const updated = await res.json();
		const idx = tasks.value.findIndex((t) => t.id === id);
		if (idx >= 0) tasks.value[idx] = updated;
	}

	async function deleteTask(id: number) {
		await fetch(`/api/tasks/${id}`, { method: "DELETE" });
		tasks.value = tasks.value.filter((t) => t.id !== id);
	}

	async function addComment(taskId: number, text: string) {
		const res = await fetch(`/api/tasks/${taskId}/comments`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ text, author: "current-user" }),
		});
		if (!res.ok) throw new Error("Failed to add comment");
		const comment = await res.json();
		const task = tasks.value.find((t) => t.id === taskId);
		if (task) task.comments.push(comment);
	}

	function setFilter(f: Partial<typeof filter.value>) {
		filter.value = { ...filter.value, ...f };
	}

	return { tasks, filter, filteredTasks, stats, fetchTasks, createTask, updateTask, deleteTask, addComment, setFilter };
});

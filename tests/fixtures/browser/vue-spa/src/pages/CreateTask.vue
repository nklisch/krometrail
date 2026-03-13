<template>
  <div data-testid="create-task-page">
    <h1>New Task</h1>
    <div v-if="error" data-testid="create-error" style="color: red">{{ error }}</div>
    <form data-testid="create-task-form" @submit.prevent="handleSubmit">
      <input v-model="form.title" data-testid="task-title-input" placeholder="Title" required />
      <textarea v-model="form.description" data-testid="task-description-input" placeholder="Description" />
      <select v-model="form.priority" data-testid="task-priority-select">
        <option value="low">Low</option>
        <option value="medium">Medium</option>
        <option value="high">High</option>
      </select>
      <input v-model="form.dueDate" data-testid="task-due-date" type="date" />
      <input v-model="form.assignee" data-testid="task-assignee" placeholder="Assignee" />
      <button type="submit" data-testid="create-task-submit" :disabled="submitting">
        {{ submitting ? "Creating..." : "Create Task" }}
      </button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { useTaskStore } from "../stores/tasks.js";

const store = useTaskStore();
const router = useRouter();
const error = ref<string | null>(null);
const submitting = ref(false);

const form = reactive({
  title: "",
  description: "",
  priority: "medium" as "low" | "medium" | "high",
  dueDate: "",
  assignee: "",
  status: "todo" as const,
});

async function handleSubmit() {
  error.value = null;
  submitting.value = true;
  try {
    const task = await store.createTask({ ...form });
    router.push(`/tasks/${task.id}`);
  } catch (e) {
    error.value = e instanceof Error ? e.message : "Failed to create task";
  } finally {
    submitting.value = false;
  }
}
</script>

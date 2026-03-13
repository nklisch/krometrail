<template>
  <div data-testid="task-detail-page">
    <p v-if="loading" data-testid="loading">Loading...</p>
    <div v-else-if="task">
      <h1 data-testid="task-title">{{ task.title }}</h1>
      <p data-testid="task-description">{{ task.description }}</p>
      <div style="display: flex; gap: 0.5rem; align-items: center">
        <span data-testid="task-status">Status: {{ task.status }}</span>
        <select data-testid="status-select" :value="task.status" @change="handleStatusChange">
          <option value="todo">To Do</option>
          <option value="in-progress">In Progress</option>
          <option value="done">Done</option>
        </select>
      </div>
      <CommentThread :comments="task.comments" :task-id="task.id" @add-comment="handleAddComment" />
    </div>
    <p v-else data-testid="not-found">Task not found.</p>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import CommentThread from "../components/CommentThread.vue";
import { useTaskStore } from "../stores/tasks.js";

const route = useRoute();
const store = useTaskStore();
const loading = ref(true);

const taskId = computed(() => Number(route.params.id));
const task = computed(() => store.tasks.find((t) => t.id === taskId.value) ?? null);

onMounted(async () => {
  try {
    if (store.tasks.length === 0) await store.fetchTasks();
  } finally {
    loading.value = false;
  }
});

async function handleStatusChange(e: Event) {
  const status = (e.target as HTMLSelectElement).value as "todo" | "in-progress" | "done";
  await store.updateTask(taskId.value, { status });
}

async function handleAddComment(id: number, text: string) {
  await store.addComment(id, text);
}
</script>

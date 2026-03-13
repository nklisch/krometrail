<template>
  <div data-testid="task-list-page">
    <h1>Tasks</h1>
    <TaskFilter />
    <div data-testid="task-list">
      <p v-if="loading" data-testid="loading">Loading...</p>
      <p v-else-if="store.filteredTasks.length === 0" data-testid="no-tasks">No tasks found.</p>
      <TaskCard v-else v-for="task in store.filteredTasks" :key="task.id" :task="task" @toggle-status="handleToggleStatus" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import TaskCard from "../components/TaskCard.vue";
import TaskFilter from "../components/TaskFilter.vue";
import { useTaskStore } from "../stores/tasks.js";

const store = useTaskStore();
const loading = ref(true);

onMounted(async () => {
  try {
    await store.fetchTasks();
  } finally {
    loading.value = false;
  }
});

async function handleToggleStatus(id: number) {
  const task = store.tasks.find((t) => t.id === id);
  if (!task) return;
  const nextStatus = task.status === "todo" ? "in-progress" : task.status === "in-progress" ? "done" : "todo";
  await store.updateTask(id, { status: nextStatus });
}
</script>

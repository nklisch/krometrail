<template>
  <div data-testid="home-page">
    <h1>Task Dashboard</h1>
    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; margin-bottom: 2rem">
      <StatCard label="total" :value="stats.total" />
      <StatCard label="completed" :value="stats.completed" />
      <StatCard label="overdue" :value="stats.overdue" />
    </div>
    <p v-if="loading" data-testid="loading">Loading...</p>
    <p v-if="error" data-testid="error" style="color: red">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import StatCard from "../components/StatCard.vue";
import { useTaskStore } from "../stores/tasks.js";

const store = useTaskStore();
const stats = store.stats;
const loading = ref(true);
const error = ref<string | null>(null);

onMounted(async () => {
  try {
    await store.fetchTasks();
  } catch (e) {
    error.value = e instanceof Error ? e.message : "Failed to load";
  } finally {
    loading.value = false;
  }
});
</script>

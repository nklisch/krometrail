<template>
  <div :data-testid="`task-card-${task.id}`" style="border: 1px solid #ccc; padding: 1rem; border-radius: 4px; margin-bottom: 0.5rem">
    <h3>
      <RouterLink :to="`/tasks/${task.id}`" :data-testid="`task-link-${task.id}`">{{ task.title }}</RouterLink>
    </h3>
    <div style="display: flex; gap: 0.5rem; flex-wrap: wrap">
      <span :data-testid="`task-status-${task.id}`" :style="{ color: statusColor }">{{ task.status }}</span>
      <span :data-testid="`task-priority-${task.id}`">{{ task.priority }}</span>
      <span :data-testid="`task-assignee-${task.id}`">{{ task.assignee }}</span>
    </div>
    <button type="button" :data-testid="`task-status-toggle-${task.id}`" @click="$emit('toggle-status', task.id)">
      Toggle Status
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { Task } from "../stores/tasks.js";

const props = defineProps<{ task: Task }>();
defineEmits<{ "toggle-status": [id: number] }>();

const statusColor = computed(() => {
  if (props.task.status === "done") return "green";
  if (props.task.status === "in-progress") return "orange";
  return "gray";
});
</script>

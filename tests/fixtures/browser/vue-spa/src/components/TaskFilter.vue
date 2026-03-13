<template>
  <div data-testid="task-filter" style="display: flex; gap: 0.5rem; margin-bottom: 1rem; flex-wrap: wrap">
    <button type="button" data-testid="filter-priority-high" @click="store.setFilter({ priority: 'high' })">High Priority</button>
    <button type="button" data-testid="filter-priority-medium" @click="store.setFilter({ priority: 'medium' })">Medium</button>
    <button type="button" data-testid="filter-priority-low" @click="store.setFilter({ priority: 'low' })">Low</button>
    <button type="button" data-testid="filter-status-todo" @click="store.setFilter({ status: 'todo' })">To Do</button>
    <button type="button" data-testid="filter-status-done" @click="store.setFilter({ status: 'done' })">Done</button>
    <button type="button" data-testid="filter-clear" @click="store.setFilter({ status: null, priority: null, search: '' })">Clear</button>
    <input data-testid="filter-search" placeholder="Search..." :value="store.filter.search" @input="store.setFilter({ search: ($event.target as HTMLInputElement).value })" />
  </div>
</template>

<script setup lang="ts">
import { useTaskStore } from "../stores/tasks.js";
const store = useTaskStore();
</script>

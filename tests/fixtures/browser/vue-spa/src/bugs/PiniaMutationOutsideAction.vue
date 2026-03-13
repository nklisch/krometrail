<template>
  <div data-testid="pinia-mutation">
    <p>Tasks count: {{ taskStore.tasks.length }}</p>
    <p>Direct count: {{ directCount }}</p>
    <button type="button" data-testid="mutate-direct" @click="directMutate">Direct Mutate</button>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useTaskStore } from "../stores/tasks.js";

const taskStore = useTaskStore();
const directCount = ref(0);

function directMutate() {
  // Intentional bug: direct state mutation outside an action
  // This bypasses Pinia's action tracking
  taskStore.$state.tasks = [...taskStore.$state.tasks];
  directCount.value += 1;
}

onMounted(() => {
  (window as Window & { __TEST_CONTROLS__?: Record<string, () => void> }).__TEST_CONTROLS__ =
    (window as Window & { __TEST_CONTROLS__?: Record<string, () => void> }).__TEST_CONTROLS__ || {};
  (window as Window & { __TEST_CONTROLS__: Record<string, () => void> }).__TEST_CONTROLS__.activatePiniaMutation = () => {
    directMutate();
  };
});
</script>

<template>
  <div data-testid="infinite-watcher">
    <p>Count: {{ count }}</p>
    <p>Active: {{ active }}</p>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref, watch } from "vue";

const count = ref(0);
const active = ref(false);

// Intentional bug: watcher mutates its own source = infinite loop
watch(count, (val) => {
  if (!active.value) return;
  count.value = val + 1;
});

onMounted(() => {
  (window as Window & { __TEST_CONTROLS__?: Record<string, () => void> }).__TEST_CONTROLS__ =
    (window as Window & { __TEST_CONTROLS__?: Record<string, () => void> }).__TEST_CONTROLS__ || {};
  (window as Window & { __TEST_CONTROLS__: Record<string, () => void> }).__TEST_CONTROLS__.activateInfiniteWatcher = () => {
    active.value = true;
    count.value = 1;
  };
});
</script>

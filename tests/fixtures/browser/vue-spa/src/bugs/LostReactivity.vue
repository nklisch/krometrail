<template>
  <div data-testid="lost-reactivity">
    <p data-testid="x-value">x: {{ x }}</p>
    <p data-testid="y-value">y: {{ y }}</p>
    <p data-testid="state-x">state.x: {{ state.x }}</p>
    <button type="button" data-testid="increment" @click="increment">Increment</button>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive } from "vue";

const state = reactive({ x: 0, y: 0 });

// Intentional bug: destructuring reactive() loses reactivity
// x and y are plain numbers, not reactive refs
const { x, y } = state;

function increment() {
  // This updates state but x/y in template won't update (lost reactivity)
  state.x += 1;
  state.y += 1;
}

onMounted(() => {
  (window as Window & { __TEST_CONTROLS__?: Record<string, () => void> }).__TEST_CONTROLS__ =
    (window as Window & { __TEST_CONTROLS__?: Record<string, () => void> }).__TEST_CONTROLS__ || {};
  (window as Window & { __TEST_CONTROLS__: Record<string, () => void> }).__TEST_CONTROLS__.activateLostReactivity = () => {
    increment();
  };
});

// Suppress linting: x, y are used in template but TypeScript doesn't see it
void x;
void y;
</script>

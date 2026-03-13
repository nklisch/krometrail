<template>
  <div :data-testid="`stat-card-${label}`" style="border: 1px solid #ccc; padding: 1rem; border-radius: 4px; text-align: center">
    <div style="font-size: 2rem; font-weight: bold" :data-testid="`stat-value-${label}`">{{ value }}</div>
    <div :data-testid="`stat-label-${label}`">{{ label }}</div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ label: string; value: number }>();
</script>

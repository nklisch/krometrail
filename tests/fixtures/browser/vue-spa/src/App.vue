<template>
  <div>
    <AppNav />
    <main style="padding: 1rem">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import AppNav from "./components/AppNav.vue";
</script>

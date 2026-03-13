<template>
  <div data-testid="bug-page">
    <component :is="bugComponent" v-if="bugComponent" />
    <div v-else data-testid="unknown-bug">Unknown bug: {{ bugName }}</div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import InfiniteWatcher from "../bugs/InfiniteWatcher.vue";
import LostReactivity from "../bugs/LostReactivity.vue";
import PiniaMutationOutsideAction from "../bugs/PiniaMutationOutsideAction.vue";

const route = useRoute();
const bugName = computed(() => route.params.name as string);

const BUG_MAP: Record<string, unknown> = {
  "infinite-watcher": InfiniteWatcher,
  "lost-reactivity": LostReactivity,
  "pinia-mutation": PiniaMutationOutsideAction,
};

const bugComponent = computed(() => BUG_MAP[bugName.value] ?? null);
</script>

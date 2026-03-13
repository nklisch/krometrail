<template>
  <div data-testid="settings-page">
    <h1>Settings</h1>
    <div v-if="error" data-testid="settings-error" style="color: red">{{ error }}</div>
    <div v-if="success" data-testid="settings-success" style="color: green">Settings saved!</div>
    <form data-testid="settings-form" @submit.prevent="handleSubmit">
      <input v-model="form.displayName" data-testid="display-name" placeholder="Display Name" />
      <input v-model="form.email" data-testid="settings-email" type="email" placeholder="Email" />
      <select v-model="form.theme" data-testid="theme-select">
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
      <button type="submit" data-testid="settings-submit">Save Settings</button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref } from "vue";

const error = ref<string | null>(null);
const success = ref(false);

const form = reactive({
  displayName: "",
  email: "",
  theme: "light",
});

async function handleSubmit() {
  error.value = null;
  success.value = false;
  const res = await fetch("/api/settings", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(form),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    error.value = data.message || `Save failed: ${res.status}`;
    return;
  }
  success.value = true;
}
</script>

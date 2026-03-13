<template>
  <div data-testid="login-page">
    <h1>Login</h1>
    <div v-if="error" data-testid="login-error" style="color: red">{{ error }}</div>
    <form data-testid="login-form" @submit.prevent="handleSubmit">
      <input v-model="username" data-testid="username" placeholder="Username" />
      <input v-model="password" data-testid="password" type="password" placeholder="Password" />
      <button type="submit" data-testid="login-submit">Login</button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { useAuthStore } from "../stores/auth.js";

const auth = useAuthStore();
const router = useRouter();
const username = ref("");
const password = ref("");
const error = ref<string | null>(null);

async function handleSubmit() {
  error.value = null;
  try {
    await auth.login(username.value, password.value);
    router.push("/");
  } catch {
    error.value = "Invalid credentials";
  }
}
</script>

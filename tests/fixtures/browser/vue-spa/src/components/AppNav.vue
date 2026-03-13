<template>
  <nav data-testid="navbar" style="display: flex; gap: 1rem; padding: 0.5rem 1rem; background: #333; color: #fff">
    <RouterLink to="/" style="color: #fff" data-testid="nav-home">Dashboard</RouterLink>
    <RouterLink to="/tasks" style="color: #fff" data-testid="nav-tasks">Tasks</RouterLink>
    <RouterLink to="/tasks/new" style="color: #fff" data-testid="nav-new-task">New Task</RouterLink>
    <RouterLink to="/settings" style="color: #fff" data-testid="nav-settings">Settings</RouterLink>
    <span v-if="auth.isAuthenticated" data-testid="nav-user">{{ auth.user?.name }}</span>
    <RouterLink v-else to="/login" style="color: #fff" data-testid="nav-login">Login</RouterLink>
    <button v-if="auth.isAuthenticated" type="button" data-testid="nav-logout" @click="auth.logout()" style="color: #fff; background: none; border: none; cursor: pointer">
      Logout
    </button>
  </nav>
</template>

<script setup lang="ts">
import { useAuthStore } from "../stores/auth.js";
const auth = useAuthStore();
</script>

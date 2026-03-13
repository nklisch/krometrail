<template>
  <div data-testid="comment-thread">
    <h3>Comments ({{ comments.length }})</h3>
    <div v-for="comment in comments" :key="comment.id" :data-testid="`comment-${comment.id}`" style="border-left: 2px solid #ccc; padding-left: 0.5rem; margin-bottom: 0.5rem">
      <strong>{{ comment.author }}</strong>
      <p>{{ comment.text }}</p>
      <small>{{ comment.createdAt }}</small>
    </div>
    <form data-testid="comment-form" @submit.prevent="submitComment">
      <input v-model="newComment" data-testid="comment-input" placeholder="Add a comment..." />
      <button type="submit" data-testid="comment-submit">Add</button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import type { Task } from "../stores/tasks.js";

const props = defineProps<{ comments: Task["comments"]; taskId: number }>();
const emit = defineEmits<{ "add-comment": [taskId: number, text: string] }>();

const newComment = ref("");

function submitComment() {
  if (!newComment.value.trim()) return;
  emit("add-comment", props.taskId, newComment.value.trim());
  newComment.value = "";
}
</script>

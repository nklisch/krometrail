<template>
	<div class="setup-tabs">
		<div class="tab-bar">
			<button v-for="(tab, i) in tabs" :key="tab.label" :class="['tab-btn', { active: activeIndex === i }]" @click="activeIndex = i">
				{{ tab.label }}
			</button>
		</div>
		<div class="tab-content">
			<div class="code-header">
				<span class="code-lang">{{ activeTab.language }}</span>
				<button class="copy-btn" :class="{ copied }" @click="copyCode">
					{{ copied ? "Copied!" : "Copy" }}
				</button>
			</div>
			<pre class="tab-pre"><code>{{ activeTab.code }}</code></pre>
		</div>
	</div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";

interface Tab {
	label: string;
	language: string;
	code: string;
}

const props = defineProps<{
	tabs: Tab[];
}>();

const activeIndex = ref(0);
const copied = ref(false);

const activeTab = computed(() => props.tabs[activeIndex.value]);

async function copyCode() {
	await navigator.clipboard.writeText(activeTab.value.code);
	copied.value = true;
	setTimeout(() => {
		copied.value = false;
	}, 2000);
}
</script>

<style scoped>
.setup-tabs {
	border: 1px solid var(--vp-c-divider);
	border-radius: var(--kt-radius-card);
	overflow: hidden;
	box-shadow: var(--kt-shadow);
}

.tab-bar {
	display: flex;
	background: var(--vp-c-bg-mute);
	border-bottom: 1px solid var(--vp-c-divider);
	overflow-x: auto;
}

.tab-btn {
	padding: 10px 20px;
	background: transparent;
	border: none;
	border-bottom: 2px solid transparent;
	color: var(--vp-c-text-2);
	font-size: 0.875rem;
	font-family: var(--vp-font-family-base);
	cursor: pointer;
	transition: all var(--kt-transition);
	white-space: nowrap;
}

.tab-btn:hover {
	color: var(--vp-c-text-1);
}

.tab-btn.active {
	color: var(--vp-c-brand-1);
	border-bottom-color: var(--vp-c-brand-1);
	background: var(--vp-c-bg-soft);
}

.tab-content {
	background: var(--vp-c-bg-soft);
}

.code-header {
	display: flex;
	align-items: center;
	justify-content: space-between;
	padding: 8px 16px;
	border-bottom: 1px solid var(--vp-c-divider);
	background: var(--vp-c-bg-mute);
}

.code-lang {
	font-size: 0.75rem;
	color: var(--vp-c-text-3);
	font-family: var(--vp-font-family-mono);
}

.copy-btn {
	font-size: 0.75rem;
	padding: 2px 10px;
	border-radius: 3px;
	border: 1px solid var(--vp-c-divider);
	background: transparent;
	color: var(--vp-c-text-3);
	cursor: pointer;
	transition: all var(--kt-transition);
	font-family: var(--vp-font-family-base);
}

.copy-btn:hover {
	border-color: var(--kt-border-highlight);
	color: var(--vp-c-text-2);
}

.copy-btn.copied {
	color: #22c55e;
	border-color: rgba(34, 197, 94, 0.3);
}

.tab-pre {
	margin: 0;
	padding: 20px;
	background: transparent;
	border: none;
	overflow-x: auto;
}

.tab-pre code {
	font-family: var(--vp-font-family-mono);
	font-size: 0.82rem;
	line-height: 1.7;
	color: var(--vp-c-text-1);
	white-space: pre;
}
</style>

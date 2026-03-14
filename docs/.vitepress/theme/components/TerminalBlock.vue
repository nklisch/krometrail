<template>
	<div class="terminal-block" :class="`accent-${accent}`">
		<div class="terminal-bar">
			<span class="dot dot-red" />
			<span class="dot dot-yellow" />
			<span class="dot dot-green" />
			<span class="terminal-title">{{ title }}</span>
			<button class="copy-btn" :class="{ copied }" @click="copyCommands" :title="copied ? 'Copied!' : 'Copy commands'">
				{{ copied ? "✓" : "Copy" }}
			</button>
		</div>
		<pre class="terminal-body"><code><template v-for="(pair, i) in pairs" :key="i"><span class="prompt">$ </span><span class="command">{{ pair.command }}</span>
<span v-if="pair.output" class="output">{{ pair.output }}</span>
</template></code></pre>
	</div>
</template>

<script setup lang="ts">
import { ref } from "vue";

interface CommandPair {
	command: string;
	output?: string;
}

const props = withDefaults(
	defineProps<{
		title?: string;
		accent?: "violet" | "cyan";
		pairs: CommandPair[];
	}>(),
	{
		title: "Terminal",
		accent: "violet",
	},
);

const copied = ref(false);

async function copyCommands() {
	const commands = props.pairs.map((p) => p.command).join("\n");
	await navigator.clipboard.writeText(commands);
	copied.value = true;
	setTimeout(() => {
		copied.value = false;
	}, 2000);
}
</script>

<style scoped>
.terminal-block {
	background: var(--vp-c-bg-soft);
	border: 1px solid var(--vp-c-divider);
	border-radius: var(--kt-radius-card);
	overflow: hidden;
	box-shadow: var(--kt-shadow);
}

.terminal-block.accent-violet {
	border-left: 3px solid var(--vp-c-brand-1);
}

.terminal-block.accent-cyan {
	border-left: 3px solid var(--kt-accent-cyan);
}

.terminal-bar {
	background: var(--vp-c-bg-mute);
	padding: 10px 16px;
	display: flex;
	align-items: center;
	gap: 8px;
	border-bottom: 1px solid var(--vp-c-divider);
}

.dot {
	width: 12px;
	height: 12px;
	border-radius: 50%;
	flex-shrink: 0;
}

.dot-red {
	background: #ef4444;
}
.dot-yellow {
	background: #eab308;
}
.dot-green {
	background: #22c55e;
}

.terminal-title {
	font-size: 0.8rem;
	color: var(--vp-c-text-3);
	margin-left: 8px;
	flex: 1;
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

.terminal-body {
	margin: 0;
	padding: 20px;
	background: transparent;
	border: none;
	overflow-x: auto;
}

.terminal-body code {
	font-family: var(--vp-font-family-mono);
	font-size: 0.82rem;
	line-height: 1.7;
	color: var(--vp-c-text-1);
	white-space: pre;
}

.prompt {
	color: var(--vp-c-text-3);
}

.accent-violet .command {
	color: var(--vp-c-brand-1);
}

.accent-cyan .command {
	color: var(--kt-accent-cyan);
}

.output {
	color: var(--vp-c-text-2);
}
</style>

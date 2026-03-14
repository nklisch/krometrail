<template>
	<section class="kt-section browser-showcase">
		<div class="section-inner">
			<h2 class="section-title">Browser Observation</h2>
			<p class="section-subtitle">
				Full session recording via Chrome DevTools Protocol — network, console, DOM, storage, and framework state,
				compressed into an agent-readable viewport.
			</p>

			<div class="capability-grid">
				<div v-for="cap in capabilities" :key="cap.title" class="capability-card kt-card kt-border-cyan">
					<div class="cap-icon">{{ cap.icon }}</div>
					<h3 class="cap-title">{{ cap.title }}</h3>
					<p class="cap-desc">{{ cap.desc }}</p>
				</div>
			</div>

			<div class="pipeline">
				<div v-for="(step, i) in pipeline" :key="step.label" class="pipeline-step">
					<a :href="step.link" class="step-box">
						<span class="step-label">{{ step.label }}</span>
					</a>
					<span v-if="i < pipeline.length - 1" class="step-arrow" aria-hidden="true">→</span>
				</div>
			</div>

			<div class="workflow-terminal">
				<div class="terminal-bar">
					<span class="dot dot-red" />
					<span class="dot dot-yellow" />
					<span class="dot dot-green" />
					<span class="terminal-title">Browser Investigation Workflow</span>
				</div>
				<pre class="terminal-body"><code><span class="cmd">$ krometrail browser record --url https://app.example.com --session checkout-flow</span>
Recording session... Press Ctrl+C to stop.

<span class="cmd">$ krometrail browser search --session checkout-flow "500 error"</span>
Found 1 match in Network events:
  POST /api/orders → 500 Internal Server Error (00:51)
  Response: {"error": "discount_code_invalid"}

<span class="cmd">$ krometrail browser inspect --session checkout-flow --at 00:51 --focus network,console</span>
── BROWSER STATE at 00:51 ──
Network:  POST /api/orders 500 (2.1s)
Console:  Error: Unhandled promise rejection: discount_code_invalid
Framework React: OrderForm state { code: "SAVE15", applying: true }
          → discountApplied never set (component stuck in loading state)</code></pre>
			</div>
		</div>
	</section>
</template>

<script setup lang="ts">
const capabilities = [
	{
		icon: "🌐",
		title: "Network",
		desc: "Capture all XHR/fetch requests with status codes, timing, headers, and response bodies.",
	},
	{
		icon: "⚛️",
		title: "Framework State",
		desc: "Inspect React, Vue component trees, props, and state at any moment in the session.",
	},
	{
		icon: "🖥️",
		title: "Console & Errors",
		desc: "Every console.log, warning, and unhandled error — timestamped and correlated with network events.",
	},
	{
		icon: "🗂️",
		title: "DOM & Input",
		desc: "Track DOM mutations, user interactions, form inputs, and scroll position over time.",
	},
	{
		icon: "📸",
		title: "Screenshots",
		desc: "Automatic screenshots at markers and on errors. Visual context for every bug.",
	},
	{
		icon: "💾",
		title: "Storage",
		desc: "localStorage, sessionStorage, cookies, and IndexedDB — all changes captured and diffed.",
	},
];

const pipeline = [
	{ label: "Search", link: "/browser/investigation-tools/search" },
	{ label: "Inspect", link: "/browser/investigation-tools/inspect" },
	{ label: "Diff", link: "/browser/investigation-tools/diff" },
	{ label: "Replay", link: "/browser/investigation-tools/replay-context" },
];
</script>

<style scoped>
.browser-showcase {
	border-top: 1px solid var(--vp-c-divider);
}

.section-inner {
	max-width: 1200px;
	margin: 0 auto;
	padding: 0 24px;
}

.section-title {
	font-size: 2rem;
	font-weight: 600;
	color: var(--vp-c-text-1);
	margin: 0 0 12px;
}

.section-subtitle {
	font-size: 1.05rem;
	color: var(--vp-c-text-2);
	line-height: 1.7;
	max-width: 640px;
	margin: 0 0 48px;
}

.capability-grid {
	display: grid;
	grid-template-columns: repeat(3, 1fr);
	gap: 16px;
	margin-bottom: 48px;
}

.capability-card {
	transition: border-color var(--kt-transition), box-shadow var(--kt-transition);
}

.capability-card:hover {
	box-shadow: 0 4px 20px rgba(34, 211, 238, 0.12);
}

.cap-icon {
	font-size: 1.5rem;
	margin-bottom: 10px;
}

.cap-title {
	font-size: 1rem;
	font-weight: 600;
	color: var(--vp-c-text-1);
	margin: 0 0 6px;
}

.cap-desc {
	font-size: 0.875rem;
	color: var(--vp-c-text-2);
	line-height: 1.6;
	margin: 0;
}

.pipeline {
	display: flex;
	align-items: center;
	gap: 8px;
	margin-bottom: 40px;
	flex-wrap: wrap;
}

.pipeline-step {
	display: flex;
	align-items: center;
	gap: 8px;
}

.step-box {
	display: inline-flex;
	align-items: center;
	padding: 8px 20px;
	background: var(--vp-c-bg-soft);
	border: 1px solid var(--kt-accent-cyan);
	border-radius: var(--kt-radius-button);
	color: var(--kt-accent-cyan);
	font-size: 0.875rem;
	font-weight: 500;
	text-decoration: none;
	transition: background var(--kt-transition), color var(--kt-transition);
}

.step-box:hover {
	background: var(--kt-accent-cyan-soft);
}

.step-arrow {
	color: var(--vp-c-text-3);
	font-size: 1.1rem;
}

.workflow-terminal {
	background: var(--vp-c-bg-soft);
	border: 1px solid var(--vp-c-divider);
	border-radius: var(--kt-radius-card);
	overflow: hidden;
	box-shadow: var(--kt-shadow);
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

.cmd {
	color: var(--kt-accent-cyan);
}

@media (max-width: 900px) {
	.capability-grid {
		grid-template-columns: repeat(2, 1fr);
	}
}

@media (max-width: 560px) {
	.capability-grid {
		grid-template-columns: 1fr;
	}
}
</style>

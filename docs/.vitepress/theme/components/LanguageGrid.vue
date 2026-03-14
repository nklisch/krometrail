<template>
	<section class="kt-section language-grid-section">
		<div class="section-inner">
			<h2 class="section-title">Language Support</h2>
			<p class="section-subtitle">10 languages via the Debug Adapter Protocol — one interface, any runtime.</p>

			<div class="lang-grid">
				<div v-for="lang in languages" :key="lang.name" class="lang-card kt-card">
					<div class="lang-header">
						<span class="lang-name">{{ lang.name }}</span>
						<span class="status-badge stable">Stable</span>
					</div>
					<span class="debugger-name">{{ lang.debugger }}</span>
				</div>
			</div>
		</div>
	</section>
</template>

<script setup lang="ts">
const languages = [
	{ name: "Python", debugger: "debugpy" },
	{ name: "Node.js / TypeScript", debugger: "js-debug" },
	{ name: "Go", debugger: "Delve" },
	{ name: "Rust", debugger: "GDB / LLDB" },
	{ name: "Java", debugger: "JDWP" },
	{ name: "C / C++", debugger: "GDB / lldb-dap" },
	{ name: "Ruby", debugger: "rdbg" },
	{ name: "C#", debugger: "netcoredbg" },
	{ name: "Swift", debugger: "lldb" },
	{ name: "Kotlin", debugger: "JDWP" },
];
</script>

<style scoped>
.language-grid-section {
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
	margin: 0 0 40px;
}

.lang-grid {
	display: grid;
	grid-template-columns: repeat(5, 1fr);
	gap: 16px;
	margin-bottom: 16px;
}

.lang-card {
	display: flex;
	flex-direction: column;
	gap: 6px;
	transition: border-color var(--kt-transition);
}

.lang-card:hover {
	border-color: var(--vp-c-brand-1);
}

.lang-header {
	display: flex;
	align-items: center;
	justify-content: space-between;
	gap: 8px;
}

.lang-name {
	font-weight: 600;
	font-size: 0.95rem;
	color: var(--vp-c-text-1);
}

.debugger-name {
	font-family: var(--vp-font-family-mono);
	font-size: 0.78rem;
	color: var(--vp-c-text-3);
}

.status-badge {
	font-size: 0.7rem;
	font-weight: 600;
	padding: 2px 8px;
	border-radius: 10px;
	white-space: nowrap;
}

.status-badge.stable {
	background: rgba(34, 197, 94, 0.12);
	color: #22c55e;
	border: 1px solid rgba(34, 197, 94, 0.3);
}

@media (max-width: 1100px) {
	.lang-grid {
		grid-template-columns: repeat(4, 1fr);
	}
}

@media (max-width: 900px) {
	.lang-grid {
		grid-template-columns: repeat(3, 1fr);
	}
}

@media (max-width: 640px) {
	.lang-grid {
		grid-template-columns: repeat(2, 1fr);
	}
}

@media (max-width: 400px) {
	.lang-grid {
		grid-template-columns: 1fr;
	}
}
</style>

<template>
	<div class="kt-landing">
		<HeroSection />
		<BrowserShowcase />
		<ViewportDemo />
		<LanguageGrid />

		<section class="kt-section quick-start-section">
			<div class="section-inner">
				<h2 class="section-title">Quick Start</h2>
				<p class="section-subtitle">Three ways to get Krometrail into your agent's hands.</p>
				<SetupTabs :tabs="setupTabs" />
			</div>
		</section>

		<ComparisonTable />

		<footer class="kt-footer">
			<div class="footer-inner">
				<div class="footer-links">
					<a href="/guide/">Docs</a>
					<a href="https://github.com/nklisch/krometrail" target="_blank" rel="noopener">GitHub</a>
				</div>
				<p class="footer-license">MIT License</p>
				<p class="footer-tagline">Built with Bun, TypeScript, and too many debugger protocols</p>
			</div>
		</footer>
	</div>
</template>

<script setup lang="ts">
import BrowserShowcase from "./components/BrowserShowcase.vue";
import ComparisonTable from "./components/ComparisonTable.vue";
import HeroSection from "./components/HeroSection.vue";
import LanguageGrid from "./components/LanguageGrid.vue";
import SetupTabs from "./components/SetupTabs.vue";
import ViewportDemo from "./components/ViewportDemo.vue";

const setupTabs = [
	{
		label: "MCP Config",
		language: "json",
		code: `{
  "mcpServers": {
    "krometrail": {
      "command": "krometrail",
      "args": ["mcp"]
    }
  }
}`,
	},
	{
		label: "CLI Install",
		language: "bash",
		code: `# One-line install (Linux/macOS)
curl -fsSL https://krometrail.dev/install.sh | sh

# Verify
krometrail --version`,
	},
	{
		label: "bunx (no install)",
		language: "bash",
		code: `# Run directly with bunx — no install needed
bunx krometrail mcp

# Or add to MCP config:
{
  "mcpServers": {
    "krometrail": {
      "command": "bunx",
      "args": ["krometrail", "mcp"]
    }
  }
}`,
	},
];
</script>

<style scoped>
.kt-landing {
	width: 100%;
}

.quick-start-section {
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

.kt-footer {
	border-top: 1px solid var(--vp-c-divider);
	padding: 40px 0;
	background: var(--vp-c-bg-soft);
}

.footer-inner {
	max-width: 1200px;
	margin: 0 auto;
	padding: 0 24px;
	text-align: center;
	display: flex;
	flex-direction: column;
	gap: 12px;
	align-items: center;
}

.footer-links {
	display: flex;
	gap: 24px;
}

.footer-links a {
	color: var(--vp-c-text-2);
	text-decoration: none;
	font-size: 0.9rem;
	transition: color var(--kt-transition);
}

.footer-links a:hover {
	color: var(--vp-c-brand-1);
}

.footer-license {
	font-size: 0.8rem;
	color: var(--vp-c-text-3);
	margin: 0;
}

.footer-tagline {
	font-size: 0.8rem;
	color: var(--vp-c-text-3);
	margin: 0;
	font-style: italic;
}
</style>

<template>
	<section class="kt-section comparison-section">
		<div class="section-inner">
			<h2 class="section-title">How Krometrail Compares</h2>
			<p class="section-subtitle">Purpose-built for AI agents, not adapted from human debugger tools.</p>

			<div class="table-wrapper">
				<table class="comparison-table">
					<thead>
						<tr>
							<th class="feature-col">Feature</th>
							<th class="kt-col">Krometrail</th>
							<th>AIDB</th>
							<th>mcp-debugger</th>
							<th>mcp-dap-server</th>
						</tr>
					</thead>
					<tbody>
						<tr v-for="row in rows" :key="row.feature">
							<td class="feature-col">{{ row.feature }}</td>
							<td class="kt-col">
								<span class="cell-val" :class="getClass(row.krometrail)">{{ getSymbol(row.krometrail) }}</span>
							</td>
							<td>
								<span class="cell-val" :class="getClass(row.aidb)">{{ getSymbol(row.aidb) }}</span>
							</td>
							<td>
								<span class="cell-val" :class="getClass(row.mcpDebugger)">{{ getSymbol(row.mcpDebugger) }}</span>
							</td>
							<td>
								<span class="cell-val" :class="getClass(row.mcpDap)">{{ getSymbol(row.mcpDap) }}</span>
							</td>
						</tr>
					</tbody>
				</table>
			</div>

			<p class="legend">✓ Full support &nbsp; ◐ Partial &nbsp; ✗ Not supported</p>
		</div>
	</section>
</template>

<script setup lang="ts">
type Val = "yes" | "no" | "partial";

interface Row {
	feature: string;
	krometrail: Val;
	aidb: Val;
	mcpDebugger: Val;
	mcpDap: Val;
}

const rows: Row[] = [
	{ feature: "Viewport abstraction", krometrail: "yes", aidb: "partial", mcpDebugger: "no", mcpDap: "no" },
	{ feature: "Context compression", krometrail: "yes", aidb: "no", mcpDebugger: "no", mcpDap: "no" },
	{ feature: "Browser observation", krometrail: "yes", aidb: "no", mcpDebugger: "no", mcpDap: "no" },
	{ feature: "10+ languages", krometrail: "yes", aidb: "partial", mcpDebugger: "partial", mcpDap: "partial" },
	{ feature: "Conditional breakpoints", krometrail: "yes", aidb: "yes", mcpDebugger: "partial", mcpDap: "yes" },
	{ feature: "Watch expressions", krometrail: "yes", aidb: "no", mcpDebugger: "no", mcpDap: "no" },
	{ feature: "Framework detection", krometrail: "yes", aidb: "yes", mcpDebugger: "no", mcpDap: "no" },
	{ feature: "CLI parity", krometrail: "yes", aidb: "no", mcpDebugger: "no", mcpDap: "no" },
	{ feature: "Token awareness", krometrail: "yes", aidb: "no", mcpDebugger: "no", mcpDap: "no" },
];

function getSymbol(val: Val): string {
	if (val === "yes") return "✓";
	if (val === "partial") return "◐";
	return "✗";
}

function getClass(val: Val): string {
	if (val === "yes") return "val-yes";
	if (val === "partial") return "val-partial";
	return "val-no";
}
</script>

<style scoped>
.comparison-section {
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

.table-wrapper {
	overflow-x: auto;
	border-radius: var(--kt-radius-card);
	border: 1px solid var(--vp-c-divider);
	box-shadow: var(--kt-shadow);
}

.comparison-table {
	width: 100%;
	border-collapse: collapse;
	font-size: 0.9rem;
}

.comparison-table th {
	background: var(--vp-c-bg-mute);
	color: var(--vp-c-text-2);
	font-weight: 600;
	font-size: 0.8rem;
	text-transform: uppercase;
	letter-spacing: 0.05em;
	padding: 12px 16px;
	text-align: center;
	border-bottom: 1px solid var(--vp-c-divider);
	white-space: nowrap;
}

.comparison-table th.feature-col {
	text-align: left;
	min-width: 180px;
}

.comparison-table th.kt-col {
	background: rgba(124, 58, 237, 0.08);
	color: var(--vp-c-brand-1);
}

.comparison-table td {
	padding: 12px 16px;
	text-align: center;
	border-bottom: 1px solid var(--vp-c-divider);
	color: var(--vp-c-text-1);
}

.comparison-table td.feature-col {
	text-align: left;
	color: var(--vp-c-text-2);
	font-size: 0.875rem;
}

.comparison-table td.kt-col {
	background: rgba(124, 58, 237, 0.05);
}

.comparison-table tr:last-child td {
	border-bottom: none;
}

.comparison-table tr:hover td {
	background: var(--vp-c-bg-soft);
}

.comparison-table tr:hover td.kt-col {
	background: rgba(124, 58, 237, 0.1);
}

.cell-val {
	font-size: 1rem;
	font-weight: 600;
}

.val-yes {
	color: #22c55e;
}

.val-partial {
	color: #f59e0b;
}

.val-no {
	color: var(--vp-c-text-3);
}

.legend {
	font-size: 0.8rem;
	color: var(--vp-c-text-3);
	margin-top: 12px;
}

@media (max-width: 640px) {
	.comparison-table th.feature-col {
		position: sticky;
		left: 0;
		background: var(--vp-c-bg-mute);
		z-index: 1;
	}

	.comparison-table td.feature-col {
		position: sticky;
		left: 0;
		background: var(--vp-c-bg);
		z-index: 1;
	}
}
</style>

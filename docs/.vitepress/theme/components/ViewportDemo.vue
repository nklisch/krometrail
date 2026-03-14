<template>
	<section class="kt-section viewport-demo-section">
		<div class="section-inner">
			<h2 class="section-title">The Debug Viewport</h2>
			<p class="section-subtitle">
				A token-aware, compressed representation of the debugger state — call stack, source context, variables, and
				watch expressions in one structured view.
			</p>

			<div class="demo-controls">
				<button :class="['mode-btn', { active: mode === 'full' }]" @click="mode = 'full'">Full Viewport</button>
				<button :class="['mode-btn', { active: mode === 'diff' }]" @click="mode = 'diff'">Diff Mode</button>
			</div>

			<div class="viewport-wrapper">
				<div class="annotation-rail">
					<div
						v-for="ann in annotations"
						:key="ann.id"
						class="annotation-badge"
						:style="{ top: ann.top }"
						@mouseenter="hoveredAnn = ann.id"
						@mouseleave="hoveredAnn = null"
					>
						{{ ann.label }}
						<div v-if="hoveredAnn === ann.id" class="ann-tooltip">{{ ann.tooltip }}</div>
					</div>
				</div>

				<div class="viewport-code-block kt-border-violet">
					<pre class="viewport-pre"><code>{{ mode === 'full' ? fullViewport : diffViewport }}</code></pre>
				</div>
			</div>
		</div>
	</section>
</template>

<script setup lang="ts">
import { ref } from "vue";

const mode = ref<"full" | "diff">("full");
const hoveredAnn = ref<string | null>(null);

const fullViewport = `── STOPPED at app/services/order.py:147 ──

Call Stack (3 frames)
  → order.py:147  apply_discount
    cart.py:89    checkout
    views.py:34   post

Source
  145│   subtotal = sum(item.price for item in items)
  146│   discount = calculate_discount(code)
→ 147│   final = subtotal * discount  # Bug: discount is -0.15
  148│   return Order(total=final)

Locals
  items     = [Item("Widget", 29.99), Item("Gadget", 49.99)]
  subtotal  = 79.98
  discount  = -0.15  ← expected 0.85
  code      = "SAVE15"`;

const diffViewport = `── STEP: order.py:147 → order.py:148 ──

Changed locals:
  final     (new) = -11.997  ← subtotal * negative discount
  discount  = -0.15  (unchanged — bug confirmed)

No stack change.

Next line:
  148│   return Order(total=final)

Watch expressions:
  subtotal * 0.85  = 67.983  ← what it should be`;

const annotations = [
	{ id: "location", label: "①", top: "4px", tooltip: "Current stop location — file:line shown in header" },
	{ id: "stack", label: "②", top: "52px", tooltip: "Compressed call stack — only relevant frames shown" },
	{ id: "source", label: "③", top: "112px", tooltip: "Source context window — 2 lines before/after current" },
	{ id: "locals", label: "④", top: "192px", tooltip: "All local variables at current frame" },
];
</script>

<style scoped>
.viewport-demo-section {
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
	margin: 0 0 32px;
}

.demo-controls {
	display: flex;
	gap: 8px;
	margin-bottom: 24px;
}

.mode-btn {
	padding: 6px 16px;
	border-radius: var(--kt-radius-button);
	border: 1px solid var(--vp-c-divider);
	background: transparent;
	color: var(--vp-c-text-2);
	font-family: var(--vp-font-family-base);
	font-size: 0.875rem;
	cursor: pointer;
	transition: all var(--kt-transition);
}

.mode-btn.active {
	background: var(--vp-c-brand-soft);
	border-color: var(--vp-c-brand-1);
	color: var(--vp-c-brand-1);
}

.mode-btn:hover:not(.active) {
	border-color: var(--kt-border-highlight);
	color: var(--vp-c-text-1);
}

.viewport-wrapper {
	position: relative;
	display: flex;
	gap: 16px;
}

.annotation-rail {
	position: relative;
	width: 32px;
	flex-shrink: 0;
}

.annotation-badge {
	position: absolute;
	width: 24px;
	height: 24px;
	border-radius: 50%;
	background: var(--vp-c-brand-soft);
	border: 1px solid var(--vp-c-brand-1);
	color: var(--vp-c-brand-1);
	font-size: 0.7rem;
	font-weight: 600;
	display: flex;
	align-items: center;
	justify-content: center;
	cursor: default;
	z-index: 10;
}

.ann-tooltip {
	position: absolute;
	left: 32px;
	top: 50%;
	transform: translateY(-50%);
	background: var(--vp-c-bg-mute);
	border: 1px solid var(--vp-c-divider);
	border-radius: 4px;
	padding: 6px 10px;
	font-size: 0.78rem;
	color: var(--vp-c-text-1);
	white-space: nowrap;
	z-index: 20;
	pointer-events: none;
	box-shadow: var(--kt-shadow);
}

.viewport-code-block {
	flex: 1;
	background: var(--vp-c-bg-soft);
	border: 1px solid var(--vp-c-divider);
	border-radius: var(--kt-radius-card);
	overflow: hidden;
	box-shadow: var(--kt-shadow);
}

.viewport-pre {
	margin: 0;
	padding: 20px;
	background: transparent;
	border: none;
	overflow-x: auto;
}

.viewport-pre code {
	font-family: var(--vp-font-family-mono);
	font-size: 0.82rem;
	line-height: 1.7;
	color: var(--vp-c-text-1);
	white-space: pre;
}

@media (max-width: 640px) {
	.annotation-rail {
		display: none;
	}
}
</style>

<template>
	<section class="hero-section">
		<div class="hero-bg" aria-hidden="true" />
		<div class="hero-inner">
			<div class="hero-headline">
				<h1 class="hero-title">
					Runtime debugging<br />
					<span class="hero-title-accent">for AI agents</span>
				</h1>
				<p class="hero-subtitle">
					Give your coding agent a real debugger. Set breakpoints, step through code, inspect variables — and observe the
					browser session at every step.
				</p>
				<div class="hero-ctas">
					<a :href="withBase('/guide/')" class="kt-btn-primary">Get Started</a>
					<a href="https://github.com/nklisch/krometrail" class="kt-btn-outline" target="_blank" rel="noopener">
						View on GitHub
					</a>
				</div>
			</div>

			<div class="hero-demo">
				<div class="demo-block demo-browser">
					<div class="demo-label">Browser Session</div>
					<pre class="demo-code"><code>── SESSION: checkout-flow ──────────────
▸ Navigation    3 pages
▸ Network      47 requests (2 failed)
▸ Console       8 errors, 3 warnings
▸ DOM          12 mutations
▸ Storage       4 changes
▸ Framework    React — 3 bug patterns

⚑ Markers
  00:12  "login completed"
  00:34  "added item to cart"
  00:51  "checkout submitted"  ← 500 error</code></pre>
				</div>

				<div class="demo-block demo-debug">
					<div class="demo-label">Debug Viewport</div>
					<pre class="demo-code"><code>── STOPPED at app/services/order.py:147 ──

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
  code      = "SAVE15"</code></pre>
				</div>
			</div>
		</div>
	</section>
</template>

<script setup lang="ts">
import { withBase } from "vitepress";
</script>

<style scoped>
.hero-section {
	position: relative;
	overflow: hidden;
	padding: 80px 0 96px;
}

.hero-bg {
	position: absolute;
	inset: 0;
	background: radial-gradient(ellipse 80% 60% at 50% -10%, rgba(124, 58, 237, 0.18) 0%, transparent 70%);
	pointer-events: none;
}

.hero-inner {
	max-width: 1200px;
	margin: 0 auto;
	padding: 0 24px;
	display: flex;
	flex-direction: column;
	gap: 56px;
	align-items: center;
}

.hero-headline {
	text-align: center;
	max-width: 680px;
}

.hero-title {
	font-size: clamp(2.2rem, 5vw, 3.5rem);
	font-weight: 600;
	line-height: 1.15;
	letter-spacing: -0.02em;
	color: var(--vp-c-text-1);
	margin: 0 0 20px;
}

.hero-title-accent {
	color: var(--vp-c-brand-1);
}

.hero-subtitle {
	font-size: 1.125rem;
	line-height: 1.7;
	color: var(--vp-c-text-2);
	margin: 0 0 32px;
}

.hero-ctas {
	display: flex;
	gap: 12px;
	justify-content: center;
	flex-wrap: wrap;
}

.hero-demo {
	display: grid;
	grid-template-columns: 1fr 1fr;
	gap: 16px;
	width: 100%;
	max-width: 960px;
}

.demo-block {
	background: var(--vp-c-bg-soft);
	border: 1px solid var(--vp-c-divider);
	border-radius: var(--kt-radius-card);
	padding: 20px;
	box-shadow: var(--kt-shadow);
}

.demo-browser {
	border-left: 3px solid var(--kt-accent-cyan);
}

.demo-debug {
	border-left: 3px solid var(--vp-c-brand-1);
}

.demo-label {
	font-size: 0.75rem;
	font-weight: 600;
	letter-spacing: 0.08em;
	text-transform: uppercase;
	color: var(--vp-c-text-3);
	margin-bottom: 12px;
}

.demo-browser .demo-label {
	color: var(--kt-accent-cyan);
}

.demo-debug .demo-label {
	color: var(--vp-c-brand-1);
}

.demo-code {
	margin: 0;
	background: transparent !important;
	border: none !important;
	padding: 0 !important;
	overflow-x: auto;
}

.demo-code code {
	font-family: var(--vp-font-family-mono);
	font-size: 0.78rem;
	line-height: 1.6;
	color: var(--vp-c-text-1);
	white-space: pre;
}

@media (max-width: 768px) {
	.hero-demo {
		grid-template-columns: 1fr;
	}

	.hero-section {
		padding: 56px 0 72px;
	}
}
</style>

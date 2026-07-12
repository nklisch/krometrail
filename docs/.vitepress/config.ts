import { defineConfig } from "vitepress";

export default defineConfig({
	title: "Krometrail",
	description: "Rust foundations for local browser control and temporal visual evidence",
	lang: "en-US",
	appearance: "dark",
	lastUpdated: true,
	cleanUrls: true,

	sitemap: {
		hostname: "https://krometrail.dev",
	},

	head: [
		[
			"script",
			{},
			`window.dataLayer = window.dataLayer || [];
function gtag(){dataLayer.push(arguments);}
gtag('js', new Date());
gtag('config', 'G-8VK84SJ371');
if (typeof requestIdleCallback === 'function') {
  requestIdleCallback(function() {
    var s = document.createElement('script');
    s.src = 'https://www.googletagmanager.com/gtag/js?id=G-8VK84SJ371';
    document.head.appendChild(s);
  });
} else {
  setTimeout(function() {
    var s = document.createElement('script');
    s.src = 'https://www.googletagmanager.com/gtag/js?id=G-8VK84SJ371';
    document.head.appendChild(s);
  }, 3000);
}`,
		],
		["link", { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" }],
		["meta", { property: "og:type", content: "website" }],
		["meta", { property: "og:title", content: "Krometrail — Rust browser capture foundation" }],
		["meta", { property: "og:description", content: "Rust foundations for local browser control and temporal visual evidence" }],
		["meta", { property: "og:image", content: "https://krometrail.dev/og-image.png" }],
		["meta", { property: "og:url", content: "https://krometrail.dev/" }],
		["meta", { name: "twitter:card", content: "summary_large_image" }],
		["meta", { name: "twitter:title", content: "Krometrail — Rust browser capture foundation" }],
		["meta", { name: "twitter:description", content: "Rust foundations for local browser control and temporal visual evidence" }],
	],

	themeConfig: {
		logo: "/favicon.svg",
		siteTitle: "Krometrail",

		nav: [
			{ text: "Guide", link: "/guide/development" },
			{
				text: "Foundation",
				items: [
					{ text: "Vision", link: "/VISION" },
					{ text: "Specification", link: "/SPEC" },
					{ text: "Architecture", link: "/ARCHITECTURE" },
					{ text: "Visual Evidence", link: "/VISUAL-EVIDENCE" },
					{ text: "Evaluation", link: "/EVALUATION" },
				],
			},
			{ text: "Reference", link: "/reference/runtime" },
			{ text: "Research", link: "/research/rust-cdp-transport-2026-07" },
		],

		sidebar: {
			"/guide/": [
				{
					text: "Contributor guide",
					items: [
						{ text: "Development", link: "/guide/development" },
						{ text: "MCP configuration", link: "/guide/mcp-configuration" },
					],
				},
			],
			"/reference/": [
				{
					text: "Reference",
					items: [
						{ text: "Runtime", link: "/reference/runtime" },
						{ text: "Configuration", link: "/reference/configuration" },
					],
				},
			],
			"/research/": [
				{
					text: "Technology research",
					items: [{ text: "Rust CDP transport — 2026-07", link: "/research/rust-cdp-transport-2026-07" }],
				},
			],
		},

		search: { provider: "local" },
		socialLinks: [{ icon: "github", link: "https://github.com/nklisch/krometrail" }],
		footer: {
			message: 'Released under the <a href="https://opensource.org/licenses/MIT">MIT License</a>. <a href="/legal/privacy">Privacy Policy</a>.',
			copyright: 'By <a href="https://nathanklisch.dev">Nathan Klisch</a>',
		},
	},
});

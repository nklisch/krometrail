import { defineConfig } from "vitepress";

export default defineConfig({
	title: "Krometrail",
	description: "Browser memory for coding agents — inspect the visual moments a screenshot misses",
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
		["meta", { property: "og:title", content: "Krometrail — Browser memory for coding agents" }],
		[
			"meta",
			{
				property: "og:description",
				content: "Catch flicker, layout jumps, reversed motion, and other transient browser bugs that disappear before the next screenshot",
			},
		],
		["meta", { property: "og:image", content: "https://krometrail.dev/og-image.png" }],
		["meta", { property: "og:url", content: "https://krometrail.dev/" }],
		["meta", { name: "twitter:card", content: "summary_large_image" }],
		["meta", { name: "twitter:title", content: "Krometrail — Browser memory for coding agents" }],
		[
			"meta",
			{
				name: "twitter:description",
				content: "Catch the visual moments a screenshot misses",
			},
		],
	],

	themeConfig: {
		logo: "/favicon.svg",
		siteTitle: "Krometrail",

		nav: [
			{ text: "Install", link: "/guide/installation" },
			{ text: "Use with your agent", link: "/guide/using-krometrail" },
			{ text: "Troubleshooting", link: "/guide/troubleshooting" },
			{
				text: "Reference",
				items: [
					{ text: "Commands", link: "/reference/runtime" },
					{ text: "Configuration", link: "/reference/configuration" },
					{ text: "Manual MCP setup", link: "/guide/mcp-configuration" },
					{ text: "Development", link: "/guide/development" },
				],
			},
		],

		sidebar: {
			"/guide/": [
				{
					text: "Start here",
					items: [
						{ text: "Install Krometrail", link: "/guide/installation" },
						{ text: "Use with your agent", link: "/guide/using-krometrail" },
						{ text: "Troubleshooting", link: "/guide/troubleshooting" },
					],
				},
				{
					text: "Manual setup",
					items: [{ text: "MCP configuration", link: "/guide/mcp-configuration" }],
				},
				{
					text: "Contributors",
					items: [{ text: "Development", link: "/guide/development" }],
				},
			],
			"/reference/": [
				{
					text: "Reference",
					items: [
						{ text: "Commands", link: "/reference/runtime" },
						{ text: "Configuration", link: "/reference/configuration" },
					],
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

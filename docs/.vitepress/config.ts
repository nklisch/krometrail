import { defineConfig } from "vitepress";

export default defineConfig({
	title: "Krometrail",
	description: "Browser observation and runtime debugging for AI coding agents",
	lang: "en-US",

	appearance: "dark",

	lastUpdated: true,

	cleanUrls: true,

	ignoreDeadLinks: [/\.generated\//],

	srcExclude: ["designs/**", "legacy/**", "framework-state/**"],

	sitemap: {
		hostname: "https://krometrail.dev",
		transformItems: (items) =>
			items.filter(
				(item) =>
					!item.url.startsWith("designs/") &&
					!item.url.startsWith("legacy/") &&
					!item.url.startsWith("framework-state/") &&
					!["ARCH", "SPEC", "UX", "VISION", "PRIOR_ART", "ADAPTER-SDK", "agents"].some((p) => item.url.startsWith(p)),
			),
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
		["meta", { property: "og:title", content: "Krometrail — AI Agent Debugging" }],
		["meta", { property: "og:description", content: "Browser observation and runtime debugging for AI coding agents" }],
		["meta", { property: "og:image", content: "https://krometrail.dev/og-image.png" }],
		["meta", { property: "og:url", content: "https://krometrail.dev/" }],
		["meta", { name: "twitter:card", content: "summary_large_image" }],
		["meta", { name: "twitter:title", content: "Krometrail — AI agent debugging and browser observation" }],
		["meta", { name: "twitter:description", content: "Give AI coding agents runtime debugging via the Debug Adapter Protocol and browser observation via Chrome DevTools Protocol" }],
		["meta", { name: "keywords", content: "AI agent debugging, browser observation for AI, MCP debugging, runtime debugging AI, Debug Adapter Protocol" }],
		[
			"script",
			{ type: "application/ld+json" },
			JSON.stringify({
				"@context": "https://schema.org",
				"@type": "WebSite",
				name: "Krometrail",
				url: "https://krometrail.dev/",
				description: "Browser observation and runtime debugging for AI coding agents",
				inLanguage: "en-US",
			}),
		],
		[
			"script",
			{ type: "application/ld+json" },
			JSON.stringify({
				"@context": "https://schema.org",
				"@type": "SoftwareApplication",
				name: "Krometrail",
				url: "https://krometrail.dev/",
				description: "MCP server and CLI that gives AI coding agents browser observation and runtime debugging via the Debug Adapter Protocol",
				applicationCategory: "DeveloperApplication",
				operatingSystem: "Linux, macOS, Windows",
				downloadUrl: "https://krometrail.dev/install.sh",
				installUrl: "https://www.npmjs.com/package/krometrail",
				license: "https://opensource.org/licenses/MIT",
				isAccessibleForFree: true,
				offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
				codeRepository: "https://github.com/nklisch/krometrail",
				programmingLanguage: ["TypeScript", "JavaScript"],
				featureList: [
					"Browser session recording via Chrome DevTools Protocol",
					"Runtime debugging across 10 languages via Debug Adapter Protocol",
					"React and Vue framework state observation",
					"MCP server for AI coding agent integration",
					"Session investigation with search, inspect, diff, and replay",
				],
			}),
		],
	],

	themeConfig: {
		logo: "/favicon.svg",
		siteTitle: "Krometrail",

		nav: [
			{ text: "Guide", link: "/guide/getting-started" },
			{ text: "Browser", link: "/browser/overview" },
			{ text: "Debugging", link: "/debugging/overview" },
			{ text: "Languages", link: "/languages/python" },
			{ text: "Reference", link: "/reference/mcp-tools" },
		],

		sidebar: {
			"/guide/": [
				{
					text: "Guide",
					items: [
						{ text: "Getting Started", link: "/guide/getting-started" },
						{ text: "MCP Configuration", link: "/guide/mcp-configuration" },
						{ text: "CLI Installation", link: "/guide/cli-installation" },
						{ text: "Your First Debug Session", link: "/guide/first-debug-session" },
						{ text: "FAQ", link: "/guide/faq" },
					],
				},
			],
			"/browser/": [
				{
					text: "Browser Observation",
					items: [
						{ text: "Overview", link: "/browser/overview" },
						{ text: "Recording & Controls", link: "/browser/recording-sessions" },
						{ text: "Markers & Screenshots", link: "/browser/markers-screenshots" },
						{
							text: "What Your Agent Sees",
							items: [
								{ text: "Search", link: "/browser/investigation-tools/search" },
								{ text: "Inspect", link: "/browser/investigation-tools/inspect" },
								{ text: "Diff", link: "/browser/investigation-tools/diff" },
								{ text: "Replay Context", link: "/browser/investigation-tools/replay-context" },
							],
						},
						{
							text: "Framework Observation",
							items: [
								{ text: "React", link: "/browser/framework-observation/react" },
								{ text: "Vue", link: "/browser/framework-observation/vue" },
							],
						},
					],
				},
			],
			"/debugging/": [
				{
					text: "Runtime Debugging",
					items: [
						{ text: "Overview", link: "/debugging/overview" },
						{ text: "Breakpoints & Stepping", link: "/debugging/breakpoints-stepping" },
						{ text: "Variables & Evaluation", link: "/debugging/variables-evaluation" },
						{ text: "Watch Expressions", link: "/debugging/watch-expressions" },
						{ text: "Context Compression", link: "/debugging/context-compression" },
						{ text: "Multi-threaded Debugging", link: "/debugging/multi-threaded" },
						{ text: "Framework Detection", link: "/debugging/framework-detection" },
					],
				},
			],
			"/languages/": [
				{
					text: "Language Support",
					items: [
						{ text: "Python", link: "/languages/python" },
						{ text: "Node.js / TypeScript", link: "/languages/nodejs" },
						{ text: "Go", link: "/languages/go" },
						{ text: "Rust", link: "/languages/rust" },
						{ text: "Java", link: "/languages/java" },
						{ text: "C / C++", link: "/languages/cpp" },
						{ text: "Ruby", link: "/languages/ruby" },
						{ text: "C#", link: "/languages/csharp" },
						{ text: "Swift", link: "/languages/swift" },
						{ text: "Kotlin", link: "/languages/kotlin" },
					],
				},
			],
			"/reference/": [
				{
					text: "Reference",
					items: [
						{ text: "MCP Tools", link: "/reference/mcp-tools" },
						{ text: "CLI Commands", link: "/reference/cli-commands" },
						{ text: "Viewport Format", link: "/reference/viewport-format" },
						{ text: "Configuration", link: "/reference/configuration" },
						{ text: "Adapter SDK", link: "/reference/adapter-sdk" },
						{ text: "Changelog", link: "/reference/changelog" },
					],
				},
			],
		},

		search: {
			provider: "local",
		},

		socialLinks: [{ icon: "github", link: "https://github.com/nklisch/krometrail" }],

		footer: {
			message: 'Released under the <a href="https://opensource.org/licenses/MIT">MIT License</a>. <a href="/legal/privacy">Privacy Policy</a>.',
			copyright: 'Built with Bun, TypeScript, and too many debugger protocols. <a href="https://github.com/nklisch/krometrail/issues">Report an issue</a>.',
		},
	},
});

import type { Theme } from "vitepress";
import DefaultTheme from "vitepress/theme";
import BrowserShowcase from "./components/BrowserShowcase.vue";
import ComparisonTable from "./components/ComparisonTable.vue";
import HeroSection from "./components/HeroSection.vue";
import LanguageGrid from "./components/LanguageGrid.vue";
import SetupTabs from "./components/SetupTabs.vue";
import TerminalBlock from "./components/TerminalBlock.vue";
import ViewportDemo from "./components/ViewportDemo.vue";
import "./custom.css";
import LandingLayout from "./Layout.vue";

export default {
	extends: DefaultTheme,

	enhanceApp({ app }) {
		// Register all custom components globally
		app.component("HeroSection", HeroSection);
		app.component("BrowserShowcase", BrowserShowcase);
		app.component("ViewportDemo", ViewportDemo);
		app.component("LanguageGrid", LanguageGrid);
		app.component("TerminalBlock", TerminalBlock);
		app.component("ComparisonTable", ComparisonTable);
		app.component("SetupTabs", SetupTabs);
		app.component("Landing", LandingLayout);
	},
} satisfies Theme;

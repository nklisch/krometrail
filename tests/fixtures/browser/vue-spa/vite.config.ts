import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

export default defineConfig({
	plugins: [vue()],
	define: {
		// Keep the target inspectable in browser-control and temporal-evaluation builds
		__VUE_PROD_DEVTOOLS__: "true",
	},
	build: {
		outDir: "dist",
		minify: false,
		sourcemap: true,
	},
	server: {
		port: 5174,
	},
});

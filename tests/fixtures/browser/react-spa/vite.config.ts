import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
	plugins: [react()],
	build: {
		outDir: "dist",
		// No minification: keeps the target easy to inspect during browser evaluation
		minify: false,
		sourcemap: true,
	},
	server: {
		port: 5173,
	},
});

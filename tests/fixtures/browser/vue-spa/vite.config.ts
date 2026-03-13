import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
	plugins: [vue()],
	build: {
		outDir: "dist",
		sourcemap: false,
	},
	server: {
		port: 5174,
	},
});

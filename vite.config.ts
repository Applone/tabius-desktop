import { svelte } from "@sveltejs/vite-plugin-svelte";
import { sveltePhosphorOptimize } from "phosphor-svelte/vite";
import { defineConfig } from "vite";
export default defineConfig({
  plugins: [svelte(), sveltePhosphorOptimize()],
  root: "frontend",
  build: { outDir: "../dist", emptyOutDir: true },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
});

import { defineConfig } from "vite";

// Vite config for the Surf FED Tauri frontend.
// We build a vanilla-JS frontend (no framework) that Tauri loads from the
// dist/ directory.  The Tauri dev server is proxied so HMR works during dev.
export default defineConfig({
  // Use relative paths so the built assets work inside Tauri's webview
  // regardless of the custom protocol used (tauri://localhost or https).
  base: "./",
  root: "src",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2021",
  },
  // During `tauri dev` the Vite dev server runs on a fixed port and Tauri
  // loads it.  We keep the config minimal.
  server: {
    port: 1420,
    strictPort: true,
  },
  clearScreen: false,
});

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri runs the dev server itself and expects a fixed port it can point the
// webview at, so `strictPort` must stay true: silently sliding to 5174 would
// leave the window staring at an empty page.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust rebuilds are cargo's job; don't let Vite churn on target/.
      ignored: ["**/src-tauri/**"],
    },
  },
});

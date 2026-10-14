import { resolve } from "node:path";
import { defineConfig } from "vite";

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2021",
    rollupOptions: {
      // janela principal + aviso rápido na tela
      input: { main: resolve(__dirname, "index.html"), osd: resolve(__dirname, "osd.html") },
    },
  },
});

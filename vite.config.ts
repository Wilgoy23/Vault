import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Set by the Tauri CLI during `tauri ios/android dev` so a physical device
// can reach the dev server over the local network.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        overlay: "overlay.html",
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
  },
});

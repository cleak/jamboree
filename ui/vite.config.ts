import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

const apiTarget = process.env.JAM_UI_API_TARGET ?? "http://127.0.0.1:8787";

export default defineConfig({
  plugins: [solid()],
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": apiTarget,
      "/ws": {
        target: apiTarget,
        ws: true
      }
    }
  }
});

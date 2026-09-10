import { defineConfig } from "vite";

/**
 * The Studio talks to a runtime over HTTP, so during development we proxy
 * `/api` to the local runtime and keep the app itself origin-agnostic. That
 * means the same build works when served from a Tauri shell, a static host or
 * `vite preview`, and no CORS configuration is needed in development.
 */
export default defineConfig({
  server: {
    port: 4173,
    strictPort: true,
    proxy: {
      "/api": {
        target: process.env.RF_RUNTIME_URL ?? "http://127.0.0.1:8710",
        changeOrigin: true,
        ws: true,
      },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
  },
});

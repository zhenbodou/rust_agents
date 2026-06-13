import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// 开发态把 /api 代理到 Rust 后端（ch36），SSE 不能被缓冲
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": { target: "http://localhost:8080", changeOrigin: true },
    },
  },
  build: { sourcemap: true },
});

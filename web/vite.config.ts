import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // Relative asset paths + hash routing = works behind any reverse-proxy sub-path.
  base: "./",
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    // `pnpm dev` talks to a Kinora server running locally.
    proxy: { "/api": "http://localhost:8420" },
  },
  test: {
    environment: "jsdom",
  },
});

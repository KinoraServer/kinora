import { fileURLToPath, URL } from "node:url";
import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";
import { VISUAL_DEVICES } from "./.storybook/devices.ts";

const storybookDir = fileURLToPath(new URL("./.storybook", import.meta.url));

/**
 * Real Chromium, headless. Set PLAYWRIGHT_CHROMIUM_PATH to use a browser that's already installed.
 * A function, because Vitest writes into this object: every project needs its own copy.
 */
const browser = () => ({
  enabled: true,
  headless: true,
  provider: playwright({ launchOptions: { executablePath: process.env.PLAYWRIGHT_CHROMIUM_PATH || undefined } }),
  instances: [{ browser: "chromium" as const }],
});

export default defineConfig({
  // Relative asset paths + hash routing = works behind any reverse-proxy sub-path.
  base: "./",
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  // Pre-bundle these up front. Otherwise the first Storybook test run finds them
  // halfway through, reloads, and reports "failed to collect tests".
  optimizeDeps: {
    include: [
      "react",
      "react-dom",
      "react-router",
      "lucide-react",
      "@tanstack/react-hotkeys",
      "clsx",
      "tailwind-merge",
    ],
  },
  server: {
    // `pnpm dev` talks to a Kinora server running locally.
    proxy: { "/api": "http://localhost:8420" },
    // The app skin rules and starter skins live with the server (crates/), shared by both.
    fs: { allow: [".", "../crates/plugin-api/skin-rules.json", "../crates/server/src/modules/app_skin_samples"] },
  },
  test: {
    projects: [
      // Plain unit tests for lib/ and friends.
      {
        extends: true,
        test: { name: "unit", environment: "jsdom", include: ["src/**/*.test.{ts,tsx}"] },
      },
      // Every story is a test: it must render, its play function must pass,
      // and it must pass the a11y checks (see .storybook/preview.tsx).
      {
        extends: true,
        plugins: [storybookTest({ configDir: storybookDir, storybookScript: "pnpm storybook --no-open" })],
        test: { name: "storybook", browser: browser(), setupFiles: ["./.storybook/vitest.setup.ts"] },
      },
      // Visual tests: stories tagged "visual", screenshotted on each device in
      // .storybook/devices.ts and compared with the saved screenshots.
      // Opt-in (`pnpm test:visual`), because screenshots differ between
      // operating systems and fonts. See docs/storybook.md.
      ...(process.env.KINORA_VISUAL
        ? VISUAL_DEVICES.map(([id]) => ({
            extends: true as const,
            plugins: [
              storybookTest({
                configDir: storybookDir,
                tags: { include: ["visual"] },
                initialGlobals: { viewport: { value: id, isRotated: false } },
              }),
            ],
            test: {
              name: `visual:${id}`,
              browser: browser(),
              setupFiles: ["./.storybook/vitest.setup.ts"],
              provide: { visualDevice: id },
            },
          }))
        : []),
    ],
  },
});

import type { StorybookConfig } from "@storybook/react-vite";

const config: StorybookConfig = {
  stories: ["../src/**/*.mdx", "../src/**/*.stories.@(ts|tsx)"],
  addons: [
    "@storybook/addon-docs",
    // Accessibility checks, shown in the "Accessibility" panel and run as part of every story test.
    "@storybook/addon-a11y",
    // Runs every story as a Vitest test in real Chromium; results show in the sidebar's test widget.
    "@storybook/addon-vitest",
    // MCP server at http://localhost:6006/mcp so coding agents can read the components and run tests.
    "@storybook/addon-mcp",
  ],
  framework: { name: "@storybook/react-vite", options: {} },
  // The sample video for the player stories.
  staticDirs: ["./public"],
  features: {
    // Component API manifest for the MCP server's docs tools.
    componentsManifest: true,
  },
  core: { disableTelemetry: true },
};

export default config;

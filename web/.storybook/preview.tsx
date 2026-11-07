import type { Decorator, Preview } from "@storybook/react-vite";
import { useEffect, type ReactNode } from "react";
import { clearTheme, ThemeProvider, useTheme } from "../src/lib/theme";
import { builtinThemes } from "../src/stories/fixtures";
import { defaultApi, installMockApi, type MockApi } from "../src/stories/mock-api";
import { VIEWPORT_OPTIONS } from "./devices";
import "../src/index.css";

/** Keeps the app's theme in step with the "Theme" toolbar menu. */
function ThemeSync({ id, children }: { id: string; children: ReactNode }) {
  const { setTheme, setMode } = useTheme();
  useEffect(() => {
    setTheme(id);
    // Show exactly the picked theme, not the other half of its pair.
    setMode(builtinThemes.find((t) => t.id === id)?.scheme ?? "dark");
  }, [id, setTheme, setMode]);
  return children;
}

const withTheme: Decorator = (Story, { globals }) => (
  <ThemeProvider>
    <ThemeSync id={(globals.theme as string) ?? "kinora"}>
      <Story />
    </ThemeSync>
  </ThemeProvider>
);

/** Device-local settings the app keeps in localStorage. Cleared so every story starts the same. */
const STORAGE_KEYS = ["kinora.theme", "kinora.theme-mode", "kinora.subtitles", "kinora.subtitle-offset"];

const preview: Preview = {
  decorators: [withTheme],

  // Runs before every story (in Storybook and in the tests): fresh settings
  // and a fake server. The returned function undoes it afterwards.
  beforeEach: ({ parameters, globals }) => {
    for (const key of STORAGE_KEYS) localStorage.removeItem(key);
    const restore = installMockApi({
      ...defaultApi((globals.theme as string) ?? "kinora"),
      ...(parameters.api as MockApi),
    });
    return () => {
      restore();
      clearTheme();
    };
  },

  globalTypes: {
    theme: {
      description: "Kinora theme",
      toolbar: {
        title: "Theme",
        icon: "paintbrush",
        items: builtinThemes.map((t) => ({ value: t.id, title: t.name })),
        dynamicTitle: true,
      },
    },
  },

  initialGlobals: {
    theme: "kinora",
  },

  parameters: {
    viewport: { options: VIEWPORT_OPTIONS },
    backgrounds: { disable: true },
    controls: {
      matchers: { color: /(background|color)$/i, date: /Date$/i },
    },
    a11y: {
      // "error" fails the story's test on any accessibility violation.
      test: "error",
    },
  },

  tags: ["autodocs"],
};

export default preview;

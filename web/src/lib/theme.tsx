// Theme system. A theme is a set of design tokens (see the server's
// plugin-api `Theme`). Built-in themes and drop-in theme files come from
// /api/themes; applying one just sets CSS custom properties on <html>, so
// every Tailwind colour utility updates instantly.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "./api";
import type { Theme } from "./types";

const STORAGE_KEY = "kinora.theme";

export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement) {
  for (const [token, value] of Object.entries(theme.tokens)) {
    root.style.setProperty(`--kn-${token}`, value);
  }
  for (const [option, value] of Object.entries(theme.player?.options ?? {})) {
    root.style.setProperty(`--kn-player-${option}`, value);
  }
  root.style.colorScheme = theme.scheme;
  root.dataset.theme = theme.id;
}

/** Remove tokens a previous theme set, so missing tokens fall back to the defaults in index.css. */
export function clearTheme(root: HTMLElement = document.documentElement) {
  for (const prop of Array.from(root.style)) {
    if (prop.startsWith("--kn-")) root.style.removeProperty(prop);
  }
}

interface ThemeContextValue {
  themes: Theme[];
  current: string;
  serverDefault: string;
  setTheme: (id: string | null) => void;
  reload: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function readStored(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [themes, setThemes] = useState<Theme[]>([]);
  const [serverDefault, setServerDefault] = useState("kinora");
  const [choice, setChoice] = useState<string | null>(readStored());

  const reload = useCallback(() => {
    api
      .get<{ default_theme: string; themes: Theme[] }>("api/themes")
      .then((r) => {
        setThemes(r.themes);
        setServerDefault(r.default_theme);
      })
      .catch(() => {});
  }, []);

  useEffect(reload, [reload]);

  const current = choice && themes.some((t) => t.id === choice) ? choice : serverDefault;

  useEffect(() => {
    const theme = themes.find((t) => t.id === current);
    if (!theme) return;
    clearTheme();
    applyTheme(theme);
  }, [themes, current]);

  const setTheme = useCallback((id: string | null) => {
    try {
      if (id) localStorage.setItem(STORAGE_KEY, id);
      else localStorage.removeItem(STORAGE_KEY);
    } catch {
      // private mode: theme just won't persist
    }
    setChoice(id);
  }, []);

  const value = useMemo(
    () => ({ themes, current, serverDefault, setTheme, reload }),
    [themes, current, serverDefault, setTheme, reload],
  );
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside ThemeProvider");
  return ctx;
}

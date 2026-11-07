// Theme system. A theme is a set of design tokens (see the server's
// plugin-api `Theme`). Built-in themes and drop-in theme files come from
// /api/themes; applying one just sets CSS custom properties on <html>, so
// every Tailwind colour utility updates instantly.
//
// Two themes can be a pair (the same look in dark and light). The viewer picks
// a theme and a mode (light, dark or system); for a pair the mode decides
// which half is shown, a theme without a pair always uses its own scheme.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "./api";
import type { Theme } from "./types";

const STORAGE_KEY = "kinora.theme";
const MODE_KEY = "kinora.theme-mode";

export type Scheme = Theme["scheme"];
export type ThemeMode = Scheme | "system";

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

/** The other half of a theme's pair, if both themes are installed and have different schemes. */
export function pairOf(theme: Theme, themes: Theme[]): Theme | undefined {
  const other = theme.pair ? themes.find((t) => t.id === theme.pair) : themes.find((t) => t.pair === theme.id);
  return other && other.scheme !== theme.scheme ? other : undefined;
}

/** One choice in the theme picker: a dark/light pair, or a single theme. */
export interface ThemeFamily {
  /** The id saved when this family is picked (the dark half of a pair). */
  id: string;
  name: string;
  dark?: Theme;
  light?: Theme;
}

/** Themes grouped the way the picker shows them, in the server's order. */
export function groupThemes(themes: Theme[]) {
  const seen = new Set<string>();
  const paired: ThemeFamily[] = [];
  const dark: ThemeFamily[] = [];
  const light: ThemeFamily[] = [];
  for (const t of themes) {
    if (seen.has(t.id)) continue;
    seen.add(t.id);
    const other = pairOf(t, themes);
    if (other) {
      seen.add(other.id);
      const [d, l] = t.scheme === "dark" ? [t, other] : [other, t];
      // "Kinora" + "Kinora Light" reads as "Kinora"; anything else names both.
      const name = l.name.startsWith(d.name) ? d.name : `${d.name} / ${l.name}`;
      paired.push({ id: d.id, name, dark: d, light: l });
    } else if (t.scheme === "light") {
      light.push({ id: t.id, name: t.name, light: t });
    } else {
      dark.push({ id: t.id, name: t.name, dark: t });
    }
  }
  return { paired, dark, light };
}

/**
 * Which theme to show: the chosen one, or its pair when that matches the mode.
 * `mode` null means "not picked yet": follow the chosen theme's own scheme.
 */
export function resolveTheme(
  themes: Theme[],
  chosenId: string,
  mode: ThemeMode | null,
  systemScheme: Scheme,
): Theme | undefined {
  const chosen = themes.find((t) => t.id === chosenId);
  if (!chosen || !mode) return chosen;
  const want = mode === "system" ? systemScheme : mode;
  if (chosen.scheme === want) return chosen;
  return pairOf(chosen, themes) ?? chosen;
}

interface ThemeContextValue {
  themes: Theme[];
  /** The theme on screen right now. */
  current: string;
  /** The theme the viewer picked (or the server default); may be the other half of `current`'s pair. */
  chosen: string;
  /** The picked mode, or the on-screen theme's scheme when none was picked. */
  mode: ThemeMode;
  serverDefault: string;
  setTheme: (id: string | null) => void;
  setMode: (mode: ThemeMode) => void;
  reload: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string | null) {
  try {
    if (value) localStorage.setItem(key, value);
    else localStorage.removeItem(key);
  } catch {
    // private mode: the choice just won't persist
  }
}

function readMode(): ThemeMode | null {
  const m = readStored(MODE_KEY);
  return m === "light" || m === "dark" || m === "system" ? m : null;
}

const darkQuery = () =>
  typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;

function useSystemScheme(): Scheme {
  const [scheme, setScheme] = useState<Scheme>(() => (darkQuery()?.matches === false ? "light" : "dark"));
  useEffect(() => {
    const q = darkQuery();
    if (!q) return;
    const update = () => setScheme(q.matches ? "dark" : "light");
    q.addEventListener("change", update);
    return () => q.removeEventListener("change", update);
  }, []);
  return scheme;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [themes, setThemes] = useState<Theme[]>([]);
  const [serverDefault, setServerDefault] = useState("kinora");
  const [choice, setChoice] = useState<string | null>(() => readStored(STORAGE_KEY));
  const [modeChoice, setModeChoice] = useState<ThemeMode | null>(readMode);
  const systemScheme = useSystemScheme();

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

  const chosen = choice && themes.some((t) => t.id === choice) ? choice : serverDefault;
  const theme = resolveTheme(themes, chosen, modeChoice, systemScheme);
  const current = theme?.id ?? chosen;
  const mode: ThemeMode = modeChoice ?? theme?.scheme ?? "dark";

  useEffect(() => {
    if (!theme) return;
    clearTheme();
    applyTheme(theme);
  }, [theme]);

  const setTheme = useCallback((id: string | null) => {
    writeStored(STORAGE_KEY, id);
    setChoice(id);
  }, []);

  const setMode = useCallback((m: ThemeMode) => {
    writeStored(MODE_KEY, m);
    setModeChoice(m);
  }, []);

  const value = useMemo(
    () => ({ themes, current, chosen, mode, serverDefault, setTheme, setMode, reload }),
    [themes, current, chosen, mode, serverDefault, setTheme, setMode, reload],
  );
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside ThemeProvider");
  return ctx;
}

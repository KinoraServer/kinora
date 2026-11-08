import { Check, Monitor, Moon, Sun, type LucideIcon } from "lucide-react";
import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { groupThemes, useTheme, type ThemeFamily, type ThemeMode } from "@/lib/theme";
import type { Theme } from "@/lib/types";
import { cn } from "@/lib/utils";

const MODES: { id: ThemeMode; labelKey: "theme.light" | "theme.dark" | "theme.system"; icon: LucideIcon }[] = [
  { id: "light", labelKey: "theme.light", icon: Sun },
  { id: "dark", labelKey: "theme.dark", icon: Moon },
  { id: "system", labelKey: "theme.system", icon: Monitor },
];

/**
 * Compact theme picker: a light/dark/system switch and a small card per theme.
 * Native radio inputs underneath, so arrow keys and screen readers work.
 * Changes apply instantly and are saved in this browser.
 */
export function ThemePicker() {
  const { t } = useTranslation();
  const { themes, current, chosen, mode, setTheme, setMode } = useTheme();
  const groups = groupThemes(themes);
  const name = useId();

  const all = [...groups.paired, ...groups.dark, ...groups.light];
  const selected = all.find((f) => f.dark?.id === current || f.light?.id === current);
  const shown = themes.find((t) => t.id === current);
  // A theme without a pair has one look, so the mode switch doesn't apply.
  const fixedScheme = selected && !(selected.dark && selected.light) ? shown?.scheme : undefined;
  const shownMode = fixedScheme ?? mode;

  const pick = (f: ThemeFamily) => {
    if (f === selected) return;
    // Keep what's on screen light or dark when switching between pairs.
    const sameScheme = shown?.scheme === "light" ? f.light : f.dark;
    setTheme((sameScheme ?? f.dark ?? f.light)!.id);
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <span className="text-sm font-medium" id={`${name}-mode`}>
          {t("theme.mode")}
        </span>
        <div
          role="radiogroup"
          aria-labelledby={`${name}-mode`}
          className="inline-flex rounded-md border bg-background p-0.5"
        >
          {MODES.map((m) => (
            <label
              key={m.id}
              className={cn(
                "relative flex h-7 items-center gap-1.5 rounded-[calc(var(--kn-radius,0.75rem)*0.6)] px-2.5 text-xs font-medium transition",
                fixedScheme ? "cursor-not-allowed" : "cursor-pointer",
                shownMode === m.id ? "bg-accent text-accent-foreground" : "text-muted-foreground",
                !fixedScheme && shownMode !== m.id && "hover:text-foreground",
                fixedScheme && shownMode !== m.id && "opacity-50",
              )}
            >
              <input
                type="radio"
                name={`${name}-mode`}
                value={m.id}
                checked={shownMode === m.id}
                disabled={!!fixedScheme}
                onChange={() => setMode(m.id)}
                className="peer sr-only"
              />
              <span className="pointer-events-none absolute inset-0 rounded-[inherit] peer-focus-visible:ring-2 peer-focus-visible:ring-ring" />
              <m.icon className="h-3.5 w-3.5" aria-hidden />
              {t(m.labelKey)}
            </label>
          ))}
        </div>
        {fixedScheme && selected && (
          <span className="text-xs text-muted-foreground">
            {t("theme.always_scheme", { name: selected.name, scheme: fixedScheme })}
          </span>
        )}
      </div>

      <div className="space-y-3" role="radiogroup" aria-label={t("theme.aria")}>
        <Group title={t("theme.group_paired")} hint={t("theme.follow_mode")}>
          {groups.paired.map((f) => (
            <ThemeCard key={f.id} family={f} name={name} checked={f === selected} onPick={pick} />
          ))}
        </Group>
        <Group title={t("theme.group_dark")}>
          {groups.dark.map((f) => (
            <ThemeCard key={f.id} family={f} name={name} checked={f === selected} onPick={pick} />
          ))}
        </Group>
        <Group title={t("theme.group_light")}>
          {groups.light.map((f) => (
            <ThemeCard key={f.id} family={f} name={name} checked={f === selected} onPick={pick} />
          ))}
        </Group>
      </div>
      {/* Kept for the case where the chosen id isn't installed any more. */}
      {!selected && themes.length > 0 && (
        <p className="text-xs text-muted-foreground">{t("theme.not_installed", { name: chosen })}</p>
      )}
    </div>
  );
}

function Group({ title, hint, children }: { title: string; hint?: string; children: ReactNode[] }) {
  if (children.length === 0) return null;
  return (
    <div>
      <p className="mb-1.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">
        {title}
        {hint && <span className="ml-2 font-normal tracking-normal normal-case">{hint}</span>}
      </p>
      <div className="flex flex-wrap gap-2">{children}</div>
    </div>
  );
}

function ThemeCard({
  family,
  name,
  checked,
  onPick,
}: {
  family: ThemeFamily;
  name: string;
  checked: boolean;
  onPick: (f: ThemeFamily) => void;
}) {
  return (
    <label
      className={cn(
        "relative block w-28 cursor-pointer rounded-md border bg-background p-1 transition",
        checked ? "border-primary ring-1 ring-primary" : "hover:border-muted-foreground",
      )}
      title={[family.light?.author, family.dark?.author].find((a) => a && a !== "Built-in") ?? undefined}
    >
      <input
        type="radio"
        name={`${name}-theme`}
        value={family.id}
        checked={checked}
        onChange={() => onPick(family)}
        className="peer sr-only"
      />
      <span className="pointer-events-none absolute -inset-px rounded-[inherit] peer-focus-visible:ring-2 peer-focus-visible:ring-ring" />
      <ThemeSwatch light={family.light ?? family.dark!} dark={family.dark ?? family.light!} />
      <span className="flex items-center gap-1 px-0.5 pt-1 text-xs leading-tight">
        <span className="min-w-0 flex-1 truncate" title={family.name}>
          {family.name}
        </span>
        {checked && <Check className="h-3 w-3 shrink-0 text-primary" aria-hidden />}
      </span>
    </label>
  );
}

/**
 * A tiny preview split on the diagonal: the light theme top-left, the dark one
 * bottom-right. A theme without a pair fills both halves with its own colours.
 */
export function ThemeSwatch({ light, dark }: { light: Theme; dark: Theme }) {
  const c = (t: Theme, token: string, fallback: string) => t.tokens[token] ?? fallback;
  return (
    <svg viewBox="0 0 88 48" className="block h-12 w-full overflow-hidden rounded-sm" aria-hidden>
      <rect width="88" height="48" fill={c(dark, "background", "#111")} />
      <polygon points="0,0 88,0 0,48" fill={c(light, "background", "#fff")} />
      {/* top-left: the light half */}
      <rect x="6" y="6" width="26" height="4" rx="2" fill={c(light, "foreground", "#111")} />
      <rect x="6" y="13" width="16" height="4" rx="2" fill={c(light, "muted-foreground", "#777")} />
      <rect x="6" y="21" width="14" height="6" rx="3" fill={c(light, "primary", "#888")} />
      {/* bottom-right: the dark half */}
      <rect x="54" y="25" width="28" height="17" rx="3" fill={c(dark, "card", "#222")} />
      <rect x="58" y="29" width="16" height="3" rx="1.5" fill={c(dark, "foreground", "#eee")} />
      <rect x="58" y="35" width="12" height="4" rx="2" fill={c(dark, "primary", "#888")} />
      <line
        x1="88"
        y1="0"
        x2="0"
        y2="48"
        stroke={c(dark, "border", "#444")}
        strokeWidth="0.75"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

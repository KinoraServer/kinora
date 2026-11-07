import { MonitorPlay, Puzzle } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote, Switch } from "@/components/ui/card";
import { ThemePicker } from "@/components/ThemePicker";
import { Field, Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { BUILTIN_STYLES, type PlayerStyleSetting, THEME_STYLE } from "@/lib/player-styles";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";
import type { PlayerSkin } from "@/lib/types";

interface SettingField {
  key: string;
  label: string;
  kind: "text" | "secret" | "bool";
  help: string | null;
  /** Used until a value is saved. */
  default?: string | null;
}

interface ModuleView {
  id: string;
  name: string;
  description: string;
  version: string;
  provides: string[];
  enabled: boolean;
  settings: SettingField[];
  values: Record<string, string>;
  secrets_set: string[];
}

export function ModulesPage() {
  const { data, reload, error } = useApi<ModuleView[]>("api/modules");
  return (
    <div className="space-y-6">
      <Card>
        <CardHeader
          title="Theme"
          description="Pick a mode and a theme for this browser. The default for everyone else is under Settings."
        />
        <div className="space-y-4 p-5 text-sm">
          <ThemePicker />
          <p className="border-t pt-4 text-muted-foreground">
            Themes are small JSON files of colours and sizes. To add one, start from{" "}
            <code className="rounded bg-muted px-1">themes/example-ocean.json.sample</code> in your config directory:
            copy it to a new name ending in <code className="rounded bg-muted px-1">.json</code>, change the id, name
            and colours. See docs/themes.md for every token.
          </p>
        </div>
      </Card>

      <ErrorNote message={error} />
      {data?.map((m) => (
        <ModuleCard key={m.id} module={m} onChange={reload} />
      ))}

      <PlayerStyleCard skinsEnabled={data?.find((m) => m.id === "player-skins")?.enabled ?? false} />

      <p className="text-xs text-muted-foreground">
        This product uses the TMDB API but is not endorsed or certified by TMDB.
      </p>
    </div>
  );
}

/** The one player layout every client uses, saved on the server. */
function PlayerStyleCard({ skinsEnabled }: { skinsEnabled: boolean }) {
  const setting = useApi<PlayerStyleSetting>("api/player/style");
  // Refetch when the skins module is switched on or off so the list matches.
  const { data: skins } = useApi<PlayerSkin[]>("api/player-skins", [skinsEnabled]);
  const { themes, current } = useTheme();
  const [choice, setChoice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const savedStyle = setting.data?.style ?? THEME_STYLE;
  useEffect(() => setChoice(null), [savedStyle]);
  const value = choice ?? savedStyle;

  const themeLayout = themes.find((t) => t.id === current)?.player?.layout;
  const nameOf = (id: string | null | undefined) =>
    BUILTIN_STYLES.find((s) => s.id === id)?.name ?? skins?.find((s) => s.id === id)?.name ?? "Minimal";
  const known =
    value === THEME_STYLE || BUILTIN_STYLES.some((s) => s.id === value) || !!skins?.some((s) => s.id === value);

  const save = async () => {
    setError(null);
    try {
      const next = await api.put<PlayerStyleSetting>("api/player/style", { style: value });
      setting.setData(next);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <Card>
      <CardHeader
        title="Player style"
        description="The video player every app and browser on this server uses. Pick a built-in style or an installed custom skin."
      />
      <div className="space-y-4 p-5">
        <div className="flex gap-3">
          <MonitorPlay className="mt-2 h-5 w-5 shrink-0 text-primary" />
          <div className="w-full max-w-sm">
            <Select
              aria-label="Player style"
              value={value}
              onChange={(e) => setChoice(e.target.value)}
              disabled={setting.loading}
            >
              <option value={THEME_STYLE}>Follow the theme ({nameOf(themeLayout)})</option>
              {BUILTIN_STYLES.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}: {s.description}
                </option>
              ))}
              {skins?.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name} (custom skin)
                </option>
              ))}
              {!known && <option value={value}>{value} (not installed, using Minimal)</option>}
            </Select>
            {!skinsEnabled && (
              <p className="mt-2 text-xs text-muted-foreground">
                Turn on Custom player skins above to choose a skin from the player-skins folder.
              </p>
            )}
          </div>
        </div>
        <div className="flex items-center gap-3">
          <Button size="sm" onClick={save} disabled={value === savedStyle}>
            Save
          </Button>
          {saved && <span className="text-sm text-muted-foreground">Saved. Players use it from the next video.</span>}
        </div>
        <ErrorNote message={error} />
      </div>
    </Card>
  );
}

function ModuleCard({ module: m, onChange }: { module: ModuleView; onChange: () => void }) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const update = async (body: object) => {
    setError(null);
    try {
      await api.put(`api/modules/${m.id}`, body);
      onChange();
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    }
  };

  const textFields = m.settings.filter((f) => f.kind !== "bool");
  const boolFields = m.settings.filter((f) => f.kind === "bool");
  const boolValue = (f: SettingField) => (m.values[f.key] ?? f.default ?? "false") === "true";

  const saveSettings = async () => {
    if (await update({ settings: values })) {
      setValues({});
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    }
  };

  return (
    <Card>
      <div className="flex items-start justify-between gap-4 p-5">
        <div className="flex gap-3">
          <Puzzle className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
          <div>
            <p className="font-medium">
              {m.name} <span className="text-xs text-muted-foreground">v{m.version}</span>
            </p>
            <p className="text-sm text-muted-foreground">{m.description}</p>
            <div className="mt-2 flex gap-1">
              {m.provides.map((p) => (
                <Badge key={p}>{p.replace(/_/g, " ")}</Badge>
              ))}
            </div>
          </div>
        </div>
        <Switch checked={m.enabled} onChange={(v) => update({ enabled: v })} label={`Enable ${m.name}`} />
      </div>
      {boolFields.length > 0 && (
        <div className="space-y-4 border-t p-5">
          {boolFields.map((f) => (
            <div key={f.key} className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium">{f.label}</p>
                {f.help && <p className="text-sm text-muted-foreground">{f.help}</p>}
              </div>
              <Switch
                checked={boolValue(f)}
                onChange={(v) => update({ settings: { [f.key]: String(v) } })}
                label={f.label}
              />
            </div>
          ))}
          {textFields.length === 0 && <ErrorNote message={error} />}
        </div>
      )}
      {m.id === "auto-tracks" && <ForgetTrackChoices />}
      {textFields.length > 0 && (
        <div className="space-y-4 border-t p-5">
          {textFields.map((f) => (
            <Field key={f.key} label={f.label} help={f.help ?? undefined}>
              <Input
                type={f.kind === "secret" ? "password" : "text"}
                placeholder={f.kind === "secret" && m.secrets_set.includes(f.key) ? "•••••••• (saved)" : ""}
                value={values[f.key] ?? (f.kind === "secret" ? "" : (m.values[f.key] ?? ""))}
                onChange={(e) => setValues({ ...values, [f.key]: e.target.value })}
                autoComplete="off"
              />
            </Field>
          ))}
          <div className="flex items-center gap-3">
            <Button size="sm" onClick={saveSettings} disabled={Object.keys(values).length === 0}>
              Save
            </Button>
            {saved && <span className="text-sm text-muted-foreground">Saved.</span>}
          </div>
          <ErrorNote message={error} />
        </div>
      )}
    </Card>
  );
}

/** Clears the signed-in viewer's remembered audio and subtitle choices (not other viewers'). */
function ForgetTrackChoices() {
  const [state, setState] = useState<"idle" | "busy" | "done">("idle");
  const [error, setError] = useState<string | null>(null);
  const forget = async () => {
    setState("busy");
    setError(null);
    try {
      await api.del("api/track-choices");
      setState("done");
    } catch (e) {
      setError((e as Error).message);
      setState("idle");
    }
  };
  return (
    <div className="flex items-center gap-3 border-t p-5">
      <Button size="sm" variant="outline" onClick={forget} disabled={state === "busy"}>
        Forget my track choices
      </Button>
      <span className="text-sm text-muted-foreground">
        {state === "done"
          ? "Done. Every show starts with its default tracks until you pick again."
          : "Only clears your own choices."}
      </span>
      <ErrorNote message={error} />
    </div>
  );
}

import { Puzzle } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote, Switch } from "@/components/ui/card";
import { Field, Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";

interface SettingField {
  key: string;
  label: string;
  kind: "text" | "secret" | "bool";
  help: string | null;
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
  const { themes } = useTheme();
  return (
    <div className="space-y-6">
      <ErrorNote message={error} />
      {data?.map((m) => <ModuleCard key={m.id} module={m} onChange={reload} />)}

      <Card>
        <CardHeader
          title="Themes"
          description="Themes are small JSON files of colours and sizes. Drop one into the themes folder in your config directory and refresh."
        />
        <div className="space-y-3 p-5 text-sm">
          <div className="flex flex-wrap gap-2">
            {themes.map((t) => (
              <Badge key={t.id}>
                {t.name}
                {t.author && t.author !== "Built-in" ? ` · ${t.author}` : ""}
              </Badge>
            ))}
          </div>
          <p className="text-muted-foreground">
            Start from <code className="rounded bg-muted px-1">themes/example-ocean.json.sample</code>: copy it to a new name ending
            in <code className="rounded bg-muted px-1">.json</code>, change the id, name and colours. See docs/themes.md for every token.
          </p>
        </div>
      </Card>

      <p className="text-xs text-muted-foreground">
        This product uses the TMDB API but is not endorsed or certified by TMDB.
      </p>
    </div>
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
      {m.settings.length > 0 && (
        <div className="space-y-4 border-t p-5">
          {m.settings.map((f) => (
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
            {saved && <span className="text-sm text-muted-foreground">Saved. New metadata is fetched on the next scan.</span>}
          </div>
          <ErrorNote message={error} />
        </div>
      )}
    </Card>
  );
}

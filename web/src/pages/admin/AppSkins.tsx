// Admin: app skins. Pick the look everyone gets, preview a skin just for
// yourself, set its options, add a starter or upload your own, and see what
// the checker (and real viewers' browsers) found wrong.

import { AlertTriangle, Eye, FolderUp, Palette, RefreshCw, Trash2, Upload } from "lucide-react";
import { useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote, Switch } from "@/components/ui/card";
import { Field, Input, Select } from "@/components/ui/input";
import { i18n } from "@/i18n";
import { api } from "@/lib/api";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";
import { useSkin } from "@/skin-engine/SkinProvider";
import { readZip } from "@/skin-engine/zip";

interface Issue {
  file: string;
  line: number;
  col: number;
  severity: "error" | "warning";
  message: string;
}

interface SkinOptionDef {
  type: "bool" | "enum" | "color" | "number";
  label: string;
  default?: unknown;
  values?: string[];
  min?: number;
  max?: number;
}

interface RuntimeError {
  page: string;
  message: string;
  at: number;
  count: number;
}

export interface SkinSummary {
  id: string;
  folder: string;
  name: string;
  version: string | null;
  author: string | null;
  description: string | null;
  homepage: string | null;
  skin_api: number | null;
  devices: string[];
  usable: boolean;
  theme: string | null;
  scheme: string | null;
  player: string | null;
  has_preview: boolean;
  readme: string | null;
  options: Record<string, SkinOptionDef>;
  option_values: Record<string, string>;
  templates: string[];
  disabled: string[];
  issues: Issue[];
  runtime_errors: RuntimeError[];
}

export interface SkinList {
  enabled: boolean;
  active: string;
  user_choice: boolean;
  skin_api: number;
  supported: number[];
  skins: SkinSummary[];
  starters: string[];
}

/** Tooltips for the starter skins the server ships. */
const STARTERS = {
  marquee: "admin.app_skins.starter_marquee",
  blank: "admin.app_skins.starter_blank",
} as const;

const starterHint = (name: string) =>
  Object.hasOwn(STARTERS, name) ? i18n.t(STARTERS[name as keyof typeof STARTERS]) : undefined;

/** Files sent one by one, then checked and installed by the server. */
async function uploadSkin(files: { path: string; data: Blob }[], replace: boolean): Promise<Response> {
  const token = crypto.randomUUID();
  for (const f of files) {
    const path = f.path.split("/").map(encodeURIComponent).join("/");
    const res = await fetch(`api/app-skins/upload/${token}/${path}`, {
      method: "PUT",
      credentials: "same-origin",
      body: f.data,
    });
    if (!res.ok) {
      const body = await res.json().catch(() => null);
      throw new Error(body?.error ?? i18n.t("admin.app_skins.upload_file_failed", { path: f.path }));
    }
  }
  return fetch(`api/app-skins/finish/${token}${replace ? "?replace=true" : ""}`, {
    method: "POST",
    credentials: "same-origin",
  });
}

/** Leave out hidden files and folders (.git, .DS_Store, .vscode). */
const visible = (path: string) => !path.split("/").some((p) => p.startsWith("."));

export function AppSkinsCard() {
  const { t } = useTranslation();
  const list = useApi<SkinList>("api/app-skins");
  const skinCtx = useSkin();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [issues, setIssues] = useState<Issue[] | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const folderInput = useRef<HTMLInputElement>(null);
  const zipInput = useRef<HTMLInputElement>(null);

  const data = list.data;
  const refresh = () => {
    list.reload();
    skinCtx.reload();
  };

  const run = async (label: string, fn: () => Promise<void>) => {
    setBusy(label);
    setError(null);
    setNote(null);
    try {
      await fn();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  const setActive = (id: string) =>
    run("active", async () => {
      await api.put("api/app-skin", { skin: id });
      refresh();
    });

  const install = (files: { path: string; data: Blob }[]) =>
    run("upload", async () => {
      setIssues(null);
      const kept = files.filter((f) => visible(f.path));
      if (!kept.length) throw new Error(t("admin.app_skins.no_files"));
      let res = await uploadSkin(kept, false);
      if (res.status === 409) {
        const body = await res.json().catch(() => null);
        const reason = body?.error ?? t("admin.app_skins.already_installed");
        if (!window.confirm(t("admin.app_skins.replace_confirm", { reason }))) return;
        res = await uploadSkin(kept, true);
      }
      const body = await res.json().catch(() => null);
      if (res.status === 422) {
        setIssues(body?.issues ?? []);
        throw new Error(t("admin.app_skins.has_errors_upload"));
      }
      if (!res.ok) throw new Error(body?.error ?? t("admin.app_skins.upload_failed"));
      setNote(t(body.replaced ? "admin.app_skins.installed_replaced" : "admin.app_skins.installed", { id: body.id }));
      refresh();
    });

  const onFolder = (fl: FileList | null) => {
    if (!fl?.length) return;
    install(Array.from(fl).map((f) => ({ path: f.webkitRelativePath || f.name, data: f })));
  };

  const onZip = async (f: File | undefined) => {
    if (!f) return;
    try {
      const entries = await readZip(f);
      install(entries.map((e) => ({ path: e.path, data: new Blob([e.data as BlobPart]) })));
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (!data) {
    return (
      <Card>
        <CardHeader title={t("admin.app_skins.title")} />
        <div className="p-5">
          <ErrorNote message={list.error} />
        </div>
      </Card>
    );
  }

  const usable = data.skins.filter((s) => s.usable);

  return (
    <Card>
      <CardHeader
        title={t("admin.app_skins.title")}
        description={t("admin.app_skins.help")}
        action={
          <Button size="sm" variant="ghost" onClick={refresh} aria-label={t("admin.app_skins.refresh")}>
            <RefreshCw className="h-4 w-4" />
          </Button>
        }
      />
      <div className="space-y-6 p-5">
        <Field label={t("admin.app_skins.look_for_everyone")} help={t("admin.app_skins.look_for_everyone_help")}>
          <Select
            aria-label={t("admin.app_skins.look_for_everyone")}
            className="max-w-sm"
            value={data.active}
            disabled={busy !== null}
            onChange={(e) => setActive(e.target.value)}
          >
            <option value="">{t("skins.standard_look")}</option>
            {usable.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
            {data.active && !usable.some((s) => s.id === data.active) && (
              <option value={data.active}>{t("admin.app_skins.missing", { id: data.active })}</option>
            )}
          </Select>
        </Field>

        <ErrorNote message={error} />
        {issues && <IssueList issues={issues} />}
        {note && <p className="text-sm text-muted-foreground">{note}</p>}

        <div className="space-y-3">
          {data.skins.length === 0 && (
            <p className="text-sm text-muted-foreground">{t("admin.app_skins.none_installed")}</p>
          )}
          {data.skins.map((s) => (
            <SkinRow
              key={s.folder}
              skin={s}
              active={data.active === s.id}
              previewing={skinCtx.preview === s.id}
              busy={busy !== null}
              onUse={() => setActive(s.id)}
              onChanged={refresh}
              run={run}
            />
          ))}
        </div>

        <div className="space-y-3 border-t pt-5">
          <h3 className="text-sm font-semibold">{t("admin.app_skins.add")}</h3>
          <div className="flex flex-wrap gap-2">
            {data.starters.map((name) => (
              <Button
                key={name}
                size="sm"
                variant="outline"
                title={starterHint(name)}
                disabled={busy !== null}
                onClick={() =>
                  run("starter", async () => {
                    const r = await api.post<{ id: string }>(`api/app-skins/starters/${name}`);
                    setNote(t("admin.app_skins.starter_added", { id: r.id }));
                    refresh();
                  })
                }
              >
                <Palette className="h-4 w-4" />{" "}
                {t("admin.app_skins.start_from", { name: name[0].toUpperCase() + name.slice(1) })}
              </Button>
            ))}
            <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => folderInput.current?.click()}>
              <FolderUp className="h-4 w-4" /> {t("admin.app_skins.upload_folder")}
            </Button>
            <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => zipInput.current?.click()}>
              <Upload className="h-4 w-4" /> {t("admin.app_skins.upload_zip")}
            </Button>
            <input
              ref={folderInput}
              type="file"
              hidden
              multiple
              // @ts-expect-error: non-standard but supported by every current browser
              webkitdirectory=""
              onChange={(e) => {
                onFolder(e.target.files);
                e.target.value = "";
              }}
            />
            <input
              ref={zipInput}
              type="file"
              hidden
              accept=".zip,application/zip"
              onChange={(e) => {
                onZip(e.target.files?.[0]);
                e.target.value = "";
              }}
            />
          </div>
          {busy === "upload" && <p className="text-sm text-muted-foreground">{t("admin.app_skins.uploading")}</p>}
          <p className="text-xs text-muted-foreground">
            <Trans
              i18nKey="admin.app_skins.folder_help"
              values={{ command: "kinora skin check <folder>" }}
              components={{ code: <code className="rounded bg-muted px-1" /> }}
            />
          </p>
        </div>

        <div className="flex items-center justify-between gap-4 border-t pt-5">
          <div>
            <p className="text-sm font-medium">{t("skins.safe_mode")}</p>
            <p className="text-xs text-muted-foreground">
              <Trans i18nKey="admin.app_skins.safe_mode_help" components={{ code: <code /> }} />
            </p>
          </div>
          <Switch label={t("skins.safe_mode")} checked={skinCtx.safeMode} onChange={skinCtx.setSafeMode} />
        </div>
      </div>
    </Card>
  );
}

function IssueList({ issues }: { issues: Issue[] }) {
  if (!issues.length) return null;
  return (
    <ul className="max-h-64 space-y-1 overflow-auto rounded-md bg-muted/50 p-3 font-mono text-xs">
      {issues.map((i, n) => (
        <li key={n} className={i.severity === "error" ? "text-destructive" : "text-muted-foreground"}>
          {i.file}
          {i.line ? `:${i.line}:${i.col}` : ""} {i.severity}: {i.message}
        </li>
      ))}
    </ul>
  );
}

function SkinRow({
  skin: s,
  active,
  previewing,
  busy,
  onUse,
  onChanged,
  run,
}: {
  skin: SkinSummary;
  active: boolean;
  previewing: boolean;
  busy: boolean;
  onUse: () => void;
  onChanged: () => void;
  run: (label: string, fn: () => Promise<void>) => Promise<void>;
}) {
  const { t } = useTranslation();
  const { startPreview, stopPreview } = useSkin();
  const { themes } = useTheme();
  const errors = s.issues.filter((i) => i.severity === "error");
  const warnings = s.issues.filter((i) => i.severity === "warning");
  const hasOptions = Object.keys(s.options).length > 0;
  const theme = s.theme ? themes.find((th) => th.id === s.theme) : null;

  return (
    <div className="rounded-lg border">
      <div className="flex flex-wrap items-start gap-4 p-4">
        {s.has_preview ? (
          <img
            src={`api/app-skins/${encodeURIComponent(s.id)}/preview`}
            alt=""
            className="h-20 w-32 shrink-0 rounded-md border object-cover"
          />
        ) : (
          <div className="flex h-20 w-32 shrink-0 items-center justify-center rounded-md border bg-muted text-muted-foreground">
            <Palette className="h-6 w-6" />
          </div>
        )}
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{s.name}</span>
            {s.version && <span className="text-xs text-muted-foreground">v{s.version}</span>}
            {active && <Badge className="bg-primary text-primary-foreground">{t("admin.app_skins.in_use")}</Badge>}
            {previewing && <Badge>{t("admin.app_skins.previewing")}</Badge>}
            {!s.usable && <Badge className="bg-destructive text-white">{t("admin.app_skins.has_errors")}</Badge>}
            {s.usable && s.disabled.length > 0 && (
              <Badge>{t("admin.app_skins.pages_left_out", { count: s.disabled.length })}</Badge>
            )}
            {warnings.length > 0 && <Badge>{t("admin.app_skins.warnings", { count: warnings.length })}</Badge>}
          </div>
          {s.description && <p className="text-sm text-muted-foreground">{s.description}</p>}
          <p className="text-xs text-muted-foreground">
            {[
              s.author && t("admin.app_skins.by_author", { author: s.author }),
              t("admin.app_skins.folder", { folder: s.folder }),
              s.devices.length
                ? t("admin.app_skins.for_devices", { devices: s.devices.join(", ") })
                : t("admin.app_skins.every_device"),
              s.player && t("admin.app_skins.includes_player"),
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {!active && (
            <Button size="sm" disabled={busy || !s.usable} onClick={onUse}>
              {t("admin.app_skins.use_for_everyone")}
            </Button>
          )}
          {previewing ? (
            <Button size="sm" variant="outline" onClick={stopPreview}>
              {t("admin.app_skins.stop_preview")}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="outline"
              disabled={!s.usable}
              onClick={() => {
                startPreview(s.id);
                window.location.hash = "#/";
              }}
              title={t("admin.app_skins.preview_title")}
            >
              <Eye className="h-4 w-4" /> {t("admin.app_skins.preview")}
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            aria-label={t("common.remove", { name: s.name })}
            disabled={busy}
            onClick={() => {
              if (!window.confirm(t("admin.app_skins.delete_confirm", { name: s.name, folder: s.folder }))) return;
              run("remove", async () => {
                await api.del(`api/app-skins/${encodeURIComponent(s.id)}`);
                if (previewing) stopPreview();
                onChanged();
              });
            }}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {theme && (
        <div className="flex flex-wrap items-center gap-3 border-t px-4 py-3 text-sm">
          <span className="text-muted-foreground">{t("admin.app_skins.made_for_theme", { name: theme.name })}</span>
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() =>
              run("theme", async () => {
                const current = await api.get<Record<string, unknown>>("api/system/settings");
                await api.put("api/system/settings", { ...current, default_theme: theme.id });
              })
            }
          >
            {t("admin.app_skins.make_default_theme")}
          </Button>
        </div>
      )}

      {hasOptions && s.usable && <SkinOptions skin={s} onSaved={onChanged} />}

      {s.runtime_errors.length > 0 && (
        <details className="border-t px-4 py-3 text-sm">
          <summary className="flex cursor-pointer items-center gap-2 text-destructive">
            <AlertTriangle className="h-4 w-4" />{" "}
            {t("admin.app_skins.runtime_errors", { count: s.runtime_errors.length })}
          </summary>
          <p className="mt-2 text-xs text-muted-foreground">{t("admin.app_skins.runtime_errors_help")}</p>
          <ul className="mt-2 space-y-1 font-mono text-xs">
            {s.runtime_errors.map((e, n) => (
              <li key={n}>
                {e.page}: {e.message} {e.count > 1 && `(×${e.count})`}
              </li>
            ))}
          </ul>
        </details>
      )}

      {s.issues.length > 0 && (
        <details className="border-t px-4 py-3 text-sm" open={errors.length > 0}>
          <summary className="cursor-pointer">
            {t("admin.app_skins.checker_found", {
              errors: t("admin.app_skins.errors", { count: errors.length }),
              warnings: t("admin.app_skins.warnings", { count: warnings.length }),
            })}
          </summary>
          <div className="mt-2">
            <IssueList issues={s.issues} />
          </div>
        </details>
      )}
    </div>
  );
}

function SkinOptions({ skin: s, onSaved }: { skin: SkinSummary; onSaved: () => void }) {
  const { t } = useTranslation();
  const [values, setValues] = useState<Record<string, string>>(s.option_values);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const changed = Object.keys(values).some((k) => values[k] !== s.option_values[k]);

  const save = async () => {
    setError(null);
    try {
      await api.put(`api/app-skins/${encodeURIComponent(s.id)}/options`, values);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <details className="border-t px-4 py-3 text-sm">
      <summary className="cursor-pointer">{t("admin.app_skins.options")}</summary>
      <div className="mt-3 grid max-w-xl gap-4">
        {Object.entries(s.options).map(([key, o]) => {
          const v = values[key] ?? "";
          const set = (nv: string) => setValues({ ...values, [key]: nv });
          if (o.type === "bool")
            return (
              <div key={key} className="flex items-center justify-between gap-4">
                <span>{o.label}</span>
                <Switch label={o.label} checked={v === "true"} onChange={(c) => set(String(c))} />
              </div>
            );
          if (o.type === "enum")
            return (
              <Field key={key} label={o.label}>
                <Select value={v} onChange={(e) => set(e.target.value)}>
                  {o.values?.map((x) => (
                    <option key={x} value={x}>
                      {x}
                    </option>
                  ))}
                </Select>
              </Field>
            );
          if (o.type === "color")
            return (
              <Field key={key} label={o.label}>
                <div className="flex gap-2">
                  <input
                    type="color"
                    aria-label={t("admin.app_skins.color_picker", { label: o.label })}
                    className="h-10 w-12 cursor-pointer rounded-md border bg-transparent"
                    value={/^#[0-9a-f]{6}$/i.test(v) ? v : "#000000"}
                    onChange={(e) => set(e.target.value)}
                  />
                  <Input
                    value={v}
                    placeholder={t("admin.app_skins.color_placeholder")}
                    onChange={(e) => set(e.target.value)}
                  />
                </div>
              </Field>
            );
          return (
            <Field key={key} label={o.label}>
              <Input type="number" value={v} min={o.min} max={o.max} onChange={(e) => set(e.target.value)} />
            </Field>
          );
        })}
        <div className="flex items-center gap-3">
          <Button size="sm" onClick={save} disabled={!changed}>
            {t("admin.app_skins.save_options")}
          </Button>
          {saved && <span className="text-muted-foreground">{t("common.saved")}</span>}
        </div>
        <ErrorNote message={error} />
      </div>
    </details>
  );
}

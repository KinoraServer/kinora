// Admin > Modules & themes: the registry. Lists every package the registry
// offers (modules, themes, player skins, app skins) and installs, updates or
// uninstalls them. Nothing in this list is part of the server until installed.
import { Download, MonitorPlay, Palette, Puzzle, RefreshCw, Trash2, AppWindow } from "lucide-react";
import { useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote, Spinner } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { useApi } from "@/lib/session";
import { cn } from "@/lib/utils";

export type PackageKind = "module" | "theme" | "player-skin" | "app-skin";

export interface RegistryPackage {
  id: string;
  kind: PackageKind;
  name: string;
  description: string;
  author: string | null;
  license: string | null;
  homepage: string | null;
  recommended: boolean;
  provides: string[];
  permissions: { http?: string[]; storage?: boolean; activity?: boolean };
  /** Newest version this server can install; null when there is none. */
  version: string | null;
  size: number | null;
  installed_version: string | null;
  installed_at: number | null;
  update_available: boolean;
}

export interface RegistryView {
  url: string;
  signed: boolean;
  error: string | null;
  packages: RegistryPackage[];
}

const KINDS: PackageKind[] = ["module", "theme", "player-skin", "app-skin"];
const KIND_ICON = { module: Puzzle, theme: Palette, "player-skin": MonitorPlay, "app-skin": AppWindow };
const KIND_LABEL = {
  module: "admin.registry.kind_module",
  theme: "admin.registry.kind_theme",
  "player-skin": "admin.registry.kind_player_skin",
  "app-skin": "admin.registry.kind_app_skin",
} as const;
const FILTER_LABEL = {
  all: "admin.registry.filter_all",
  module: "admin.registry.filter_module",
  theme: "admin.registry.filter_theme",
  "player-skin": "admin.registry.filter_player_skin",
  "app-skin": "admin.registry.filter_app_skin",
} as const;

/** What a module may do outside its sandbox, in words. */
function usePermissionLines(p: RegistryPackage): string[] {
  const { t } = useTranslation();
  const lines: string[] = [];
  if (p.permissions.http?.length) lines.push(t("admin.registry.perm_http", { hosts: p.permissions.http.join(", ") }));
  if (p.permissions.storage) lines.push(t("admin.registry.perm_storage"));
  if (p.permissions.activity) lines.push(t("admin.registry.perm_activity"));
  return lines;
}

/** `onChanged` runs after anything was installed, updated or uninstalled. */
export function RegistryCard({ onChanged }: { onChanged: () => void }) {
  const { t } = useTranslation();
  const [refresh, setRefresh] = useState(0);
  const { data, error, loading, reload } = useApi<RegistryView>(
    refresh ? `api/registry?refresh=true&n=${refresh}` : "api/registry",
  );
  const [filter, setFilter] = useState<PackageKind | "all">("all");
  const [busy, setBusy] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [removing, setRemoving] = useState<RegistryPackage | null>(null);

  const run = async (id: string, work: () => Promise<unknown>) => {
    setBusy(id);
    setFailed(null);
    try {
      await work();
      reload();
      onChanged();
    } catch (e) {
      setFailed((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  const packages = data?.packages ?? [];
  const kinds = KINDS.filter((k) => packages.some((p) => p.kind === k));
  const shown = packages.filter((p) => filter === "all" || p.kind === filter);

  return (
    <Card>
      <CardHeader
        title={t("admin.registry.title")}
        description={t("admin.registry.help")}
        action={
          <Button
            size="sm"
            variant="ghost"
            className="w-8 px-0"
            aria-label={t("admin.registry.refresh")}
            title={t("admin.registry.refresh")}
            disabled={loading}
            onClick={() => setRefresh((n) => n + 1)}
          >
            <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} />
          </Button>
        }
      />
      <div className="space-y-4 p-5">
        <ErrorNote message={error} />
        {data?.error && (
          <p className="rounded-md bg-muted px-3 py-2 text-sm text-muted-foreground">
            {t("admin.registry.unreachable", { error: data.error })}
          </p>
        )}
        <ErrorNote message={failed} />
        {!data && loading && <Spinner />}

        {kinds.length > 1 && (
          <div className="flex flex-wrap gap-2" role="group" aria-label={t("admin.registry.filter")}>
            {(["all", ...kinds] as const).map((k) => (
              <Button
                key={k}
                size="sm"
                variant={filter === k ? "secondary" : "outline"}
                aria-pressed={filter === k}
                onClick={() => setFilter(k)}
              >
                {t(FILTER_LABEL[k])}
              </Button>
            ))}
          </div>
        )}

        {data && packages.length === 0 && !data.error && (
          <p className="text-sm text-muted-foreground">{t("admin.registry.empty")}</p>
        )}
        <ul className="divide-y rounded-lg border">
          {shown.map((p) => (
            <PackageRow
              key={p.id}
              pkg={p}
              busy={busy === p.id}
              disabled={busy !== null}
              onInstall={() => run(p.id, () => api.post(`api/registry/${p.id}/install`))}
              onRemove={() => setRemoving(p)}
            />
          ))}
        </ul>

        {data && (
          <p className="text-xs text-muted-foreground">
            <Trans
              i18nKey={data.signed ? "admin.registry.source_signed" : "admin.registry.source"}
              values={{ url: data.url }}
              components={{ code: <code className="rounded bg-muted px-1 break-all" /> }}
            />
          </p>
        )}
      </div>

      {removing && (
        <UninstallDialog
          pkg={removing}
          onClose={() => setRemoving(null)}
          onConfirm={(purge) => {
            const p = removing;
            setRemoving(null);
            run(p.id, () => api.del(`api/registry/${p.id}${purge ? "?purge=true" : ""}`));
          }}
        />
      )}
    </Card>
  );
}

function PackageRow({
  pkg: p,
  busy,
  disabled,
  onInstall,
  onRemove,
}: {
  pkg: RegistryPackage;
  busy: boolean;
  disabled: boolean;
  onInstall: () => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const Icon = KIND_ICON[p.kind];
  const permissions = usePermissionLines(p);
  const installed = p.installed_version !== null;
  return (
    <li className="flex flex-wrap items-start justify-between gap-x-4 gap-y-3 p-4">
      <div className="flex min-w-0 flex-1 basis-64 gap-3">
        <Icon className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
        <div className="min-w-0">
          <p className="font-medium">
            {p.name}{" "}
            <span className="text-xs font-normal text-muted-foreground">
              v{p.installed_version ?? p.version ?? "?"}
              {p.author ? ` · ${p.author}` : ""}
            </span>
          </p>
          <p className="text-sm text-muted-foreground">{p.description}</p>
          <div className="mt-2 flex flex-wrap gap-1">
            <Badge>{t(KIND_LABEL[p.kind])}</Badge>
            {p.recommended && !installed && (
              <Badge className="bg-primary/15 text-foreground">{t("admin.registry.recommended")}</Badge>
            )}
            {installed && <Badge className="bg-primary/15 text-foreground">{t("admin.registry.installed")}</Badge>}
            {!installed && p.size !== null && <Badge>{formatBytes(p.size)}</Badge>}
          </div>
          {permissions.length > 0 && (
            <p className="mt-2 text-xs text-muted-foreground">
              {t("admin.registry.perm_intro")} {permissions.join(" ")}
            </p>
          )}
          {installed && p.version === null && (
            <p className="mt-2 text-xs text-muted-foreground">{t("admin.registry.not_offered")}</p>
          )}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {busy && <Spinner className="h-5 w-5" />}
        {!installed && (
          <Button
            size="sm"
            disabled={disabled || p.version === null}
            onClick={onInstall}
            aria-label={t("admin.registry.install_name", { name: p.name })}
          >
            <Download className="h-4 w-4" /> {t("admin.registry.install")}
          </Button>
        )}
        {installed && p.update_available && (
          <Button
            size="sm"
            disabled={disabled}
            onClick={onInstall}
            aria-label={t("admin.registry.update_name", { name: p.name, version: p.version })}
          >
            <Download className="h-4 w-4" /> {t("admin.registry.update", { version: p.version })}
          </Button>
        )}
        {installed && (
          <Button
            size="sm"
            variant="ghost"
            className="w-8 px-0"
            disabled={disabled}
            onClick={onRemove}
            aria-label={t("admin.registry.uninstall_name", { name: p.name })}
            title={t("admin.registry.uninstall_name", { name: p.name })}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        )}
      </div>
    </li>
  );
}

/** Asks before uninstalling. A module can take its settings and saved data with it. */
export function UninstallDialog({
  pkg,
  onClose,
  onConfirm,
}: {
  pkg: RegistryPackage;
  onClose: () => void;
  onConfirm: (purge: boolean) => void;
}) {
  const { t } = useTranslation();
  const [purge, setPurge] = useState(false);
  return (
    <Dialog open onClose={onClose} title={t("admin.registry.uninstall_title", { name: pkg.name })}>
      <div className="space-y-4 text-sm">
        <p className="text-muted-foreground">
          {pkg.kind === "module" ? t("admin.registry.uninstall_module") : t("admin.registry.uninstall_files")}
        </p>
        {pkg.kind === "module" && (
          <label className="flex cursor-pointer items-start gap-2">
            <input
              type="checkbox"
              className="mt-0.5 h-4 w-4 cursor-pointer accent-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              checked={purge}
              onChange={(e) => setPurge(e.target.checked)}
            />
            <span>{t("admin.registry.uninstall_purge")}</span>
          </label>
        )}
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" variant="destructive" onClick={() => onConfirm(purge)}>
            {t("admin.registry.uninstall")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

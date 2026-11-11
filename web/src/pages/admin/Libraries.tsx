import { FolderPlus, Pencil, RefreshCw, Trash2, X } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { FolderPicker } from "@/components/FolderPicker";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, Empty, ErrorNote } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { parentFolder } from "@/lib/paths";
import { useApi } from "@/lib/session";
import type { Library } from "@/lib/types";

export function LibrariesPage() {
  const { t } = useTranslation();
  const { data, reload, error } = useApi<Library[]>("api/libraries");
  // null: closed, "new": adding, a library: editing it.
  const [editing, setEditing] = useState<Library | "new" | null>(null);
  const anyRunning = data?.some((l) => l.scan?.running);

  // Poll while a scan is running.
  useEffect(() => {
    if (!anyRunning) return;
    const t = setInterval(reload, 1500);
    return () => clearInterval(t);
  }, [anyRunning, reload]);

  const scan = async (id: string) => {
    await api.post(`api/libraries/${id}/scan`);
    reload();
  };
  const remove = async (lib: Library) => {
    if (!confirm(t("admin.libraries.remove_confirm", { name: lib.name }))) return;
    await api.del(`api/libraries/${lib.id}`);
    reload();
  };

  return (
    <Card>
      <CardHeader
        title={t("admin.libraries.title")}
        description={t("admin.libraries.help")}
        action={
          <Button onClick={() => setEditing("new")}>
            <FolderPlus className="h-4 w-4" /> {t("admin.libraries.add")}
          </Button>
        }
      />
      <ErrorNote message={error} />
      <div className="divide-y">
        {data?.length === 0 && (
          <div className="p-5">
            <Empty title={t("admin.libraries.empty")}>{t("admin.libraries.empty_body")}</Empty>
          </div>
        )}
        {data?.map((lib) => (
          <div key={lib.id} className="flex flex-wrap items-center justify-between gap-4 p-5">
            <div className="min-w-0 space-y-1">
              <p className="flex items-center gap-2 font-medium">
                {lib.name}{" "}
                <Badge>{lib.kind === "tv" ? t("admin.libraries.kind_tv") : t("admin.libraries.kind_movies")}</Badge>
              </p>
              <p className="truncate font-mono text-xs text-muted-foreground">{lib.paths.join(", ")}</p>
              <p className="text-sm text-muted-foreground">
                {lib.scan?.running
                  ? t("admin.libraries.scanning", { phase: lib.scan.phase }) +
                    (lib.scan.total
                      ? ` ${t("admin.libraries.scan_progress", { processed: lib.scan.processed, total: lib.scan.total })}`
                      : "")
                  : t("admin.libraries.stats", { count: lib.item_count, lastScan: formatDate(lib.last_scan_at) })}
                {lib.scan &&
                  !lib.scan.running &&
                  lib.scan.errors > 0 &&
                  t("admin.libraries.scan_errors", { count: lib.scan.errors })}
              </p>
              {lib.scan?.last_error && !lib.scan.running && (
                <p className="text-xs text-destructive">{lib.scan.last_error}</p>
              )}
            </div>
            <div className="flex gap-2">
              <Button variant="secondary" size="sm" onClick={() => scan(lib.id)} disabled={lib.scan?.running}>
                <RefreshCw className={`h-4 w-4 ${lib.scan?.running ? "animate-spin" : ""}`} />{" "}
                {t("admin.libraries.scan")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setEditing(lib)}
                aria-label={t("admin.libraries.edit", { name: lib.name })}
                title={t("admin.libraries.edit", { name: lib.name })}
              >
                <Pencil className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => remove(lib)}
                aria-label={t("admin.libraries.remove", { name: lib.name })}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        ))}
      </div>
      {editing && (
        <LibraryDialog
          // A fresh form for each library opened.
          key={editing === "new" ? "new" : editing.id}
          library={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            reload();
          }}
        />
      )}
    </Card>
  );
}

/** Add a library, or edit one: rename it and change its folders. Its type is fixed once added. */
export function LibraryDialog({
  library,
  onClose,
  onSaved,
}: {
  library: Library | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(library?.name ?? "");
  const [kind, setKind] = useState<"movies" | "tv">(library?.kind ?? "movies");
  const [paths, setPaths] = useState<string[]>(library?.paths ?? []);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const removedPaths = library ? library.paths.filter((p) => !paths.includes(p)) : [];
  const pathsChanged = library ? removedPaths.length > 0 || paths.some((p) => !library.paths.includes(p)) : true;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (library) await api.put(`api/libraries/${library.id}`, { name, paths });
      else await api.post("api/libraries", { name, kind, paths });
      onSaved();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={library ? t("admin.libraries.edit", { name: library.name }) : t("admin.libraries.add")}
    >
      <form onSubmit={submit} className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("common.name")}>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("admin.libraries.name_placeholder")}
              required
            />
          </Field>
          <Field label={t("common.type")} help={library ? t("admin.libraries.kind_fixed") : undefined}>
            <Select
              value={kind}
              onChange={(e) => setKind(e.target.value as "movies" | "tv")}
              disabled={library !== null}
            >
              <option value="movies">{t("admin.libraries.kind_movies")}</option>
              <option value="tv">{t("admin.libraries.kind_tv")}</option>
            </Select>
          </Field>
        </div>
        <Field label={t("admin.libraries.folders")} help={t("admin.libraries.folders_help")}>
          <div className="mb-2 space-y-1">
            {paths.map((p) => (
              <div
                key={p}
                className="flex items-center justify-between gap-2 rounded-md bg-muted px-3 py-1.5 font-mono text-xs"
              >
                <span className="min-w-0 break-all">{p}</span>
                <button
                  type="button"
                  onClick={() => setPaths(paths.filter((x) => x !== p))}
                  className="shrink-0 cursor-pointer rounded-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                  aria-label={t("common.remove", { name: p })}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
          </div>
          <FolderPicker
            initialPath={library ? parentFolder(library.paths[0] ?? "") : undefined}
            onPick={(p) => !paths.includes(p) && setPaths([...paths, p])}
          />
        </Field>
        {library && pathsChanged && (
          <p className="rounded-md border px-3 py-2 text-sm text-muted-foreground" role="status">
            {removedPaths.length > 0
              ? t("admin.libraries.folders_removed", { count: removedPaths.length })
              : t("admin.libraries.folders_changed")}
          </p>
        )}
        <ErrorNote message={error} />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" disabled={busy || paths.length === 0 || !name.trim()}>
            {library
              ? pathsChanged
                ? t("admin.libraries.save_and_scan")
                : t("common.save")
              : t("admin.libraries.add_and_scan")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

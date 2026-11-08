import { FolderPlus, RefreshCw, Trash2, X } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { FolderPicker } from "@/components/FolderPicker";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, Empty, ErrorNote } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { useApi } from "@/lib/session";
import type { Library } from "@/lib/types";

export function LibrariesPage() {
  const { t } = useTranslation();
  const { data, reload, error } = useApi<Library[]>("api/libraries");
  const [adding, setAdding] = useState(false);
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
          <Button onClick={() => setAdding(true)}>
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
                onClick={() => remove(lib)}
                aria-label={t("admin.libraries.remove", { name: lib.name })}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        ))}
      </div>
      <AddLibraryDialog
        open={adding}
        onClose={() => setAdding(false)}
        onAdded={() => {
          setAdding(false);
          reload();
        }}
      />
    </Card>
  );
}

function AddLibraryDialog({ open, onClose, onAdded }: { open: boolean; onClose: () => void; onAdded: () => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"movies" | "tv">("movies");
  const [paths, setPaths] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.post("api/libraries", { name, kind, paths });
      setName("");
      setPaths([]);
      onAdded();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onClose={onClose} title={t("admin.libraries.add")}>
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
          <Field label={t("common.type")}>
            <Select value={kind} onChange={(e) => setKind(e.target.value as "movies" | "tv")}>
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
                className="flex items-center justify-between rounded-md bg-muted px-3 py-1.5 font-mono text-xs"
              >
                {p}
                <button
                  type="button"
                  onClick={() => setPaths(paths.filter((x) => x !== p))}
                  className="cursor-pointer"
                  aria-label={t("common.remove", { name: p })}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
          </div>
          <FolderPicker onPick={(p) => !paths.includes(p) && setPaths([...paths, p])} />
        </Field>
        <ErrorNote message={error} />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" disabled={busy || paths.length === 0}>
            {t("admin.libraries.add_and_scan")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

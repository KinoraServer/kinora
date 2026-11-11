import { ChevronUp, Folder } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/lib/session";
import { Button } from "./ui/button";
import { Spinner } from "./ui/card";
import { Input } from "./ui/input";

interface FolderList {
  path: string;
  parent: string | null;
  folders: string[];
}

/**
 * Browse folders on the server (inside Docker: the container's view, usually /media).
 * Starts at `initialPath`, or the server's default (/media when it exists).
 */
export function FolderPicker({ onPick, initialPath = "" }: { onPick: (path: string) => void; initialPath?: string }) {
  const { t } = useTranslation();
  const [path, setPath] = useState(initialPath);
  const { data, error, loading } = useApi<FolderList>(`api/fs/folders?path=${encodeURIComponent(path)}`);
  const join = (base: string, name: string) =>
    base.endsWith("/") || base.endsWith("\\") ? base + name : `${base}/${name}`;

  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        <Input value={data?.path ?? path} onChange={(e) => setPath(e.target.value)} aria-label={t("folder.path")} />
        <Button type="button" onClick={() => data && onPick(data.path)} disabled={!data}>
          {t("folder.use")}
        </Button>
      </div>
      <div className="h-56 overflow-auto rounded-md border bg-background">
        {loading && <Spinner className="m-4" />}
        {error && <p className="p-3 text-sm text-destructive">{error}</p>}
        {data?.parent && (
          <button
            type="button"
            onClick={() => setPath(data.parent!)}
            className="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-sm hover:bg-accent"
          >
            <ChevronUp className="h-4 w-4" /> ..
          </button>
        )}
        {data?.folders.map((f) => (
          <button
            type="button"
            key={f}
            onClick={() => setPath(join(data.path, f))}
            className="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-sm hover:bg-accent"
          >
            <Folder className="h-4 w-4 text-primary" /> {f}
          </button>
        ))}
        {data && data.folders.length === 0 && <p className="p-3 text-sm text-muted-foreground">{t("folder.empty")}</p>}
      </div>
    </div>
  );
}

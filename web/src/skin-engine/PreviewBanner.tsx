// A small bar while an admin previews a skin, so it's clear only they see it.

import { useSkin } from "./SkinProvider";

export function PreviewBanner() {
  const { preview, bundle, reason, stopPreview } = useSkin();
  if (!preview) return null;
  return (
    <div
      role="status"
      data-kn-standard
      className="fixed bottom-4 left-1/2 z-[100] flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-3 rounded-full border bg-card px-4 py-2 text-sm text-foreground shadow-xl"
    >
      <span className="min-w-0 truncate">
        Previewing <strong>{bundle?.name ?? preview}</strong>
        {reason ? ` (${reason})` : ". Only you see this."}
      </span>
      <a href="#/admin/modules" className="cursor-pointer text-muted-foreground underline-offset-2 hover:underline">
        Skins
      </a>
      <button
        type="button"
        onClick={stopPreview}
        className="cursor-pointer rounded-full bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
      >
        Stop preview
      </button>
    </div>
  );
}

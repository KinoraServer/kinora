// A small bar while an admin previews a skin, so it's clear only they see it.

import { Trans, useTranslation } from "react-i18next";
import { useSkin } from "./SkinProvider";

export function PreviewBanner() {
  const { t } = useTranslation();
  const { preview, bundle, reason, stopPreview } = useSkin();
  if (!preview) return null;
  return (
    <div
      role="status"
      data-kn-standard
      className="fixed bottom-4 left-1/2 z-[100] flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-3 rounded-full border bg-card px-4 py-2 text-sm text-foreground shadow-xl"
    >
      <span className="min-w-0 truncate">
        <Trans
          i18nKey={reason ? "skins.previewing_reason" : "skins.previewing"}
          values={{ name: bundle?.name ?? preview, reason }}
          components={{ strong: <strong /> }}
        />
      </span>
      <a href="#/admin/modules" className="cursor-pointer text-muted-foreground underline-offset-2 hover:underline">
        {t("skins.admin_link")}
      </a>
      <button
        type="button"
        onClick={stopPreview}
        className="cursor-pointer rounded-full bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
      >
        {t("skins.stop_preview")}
      </button>
    </div>
  );
}

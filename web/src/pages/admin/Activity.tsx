import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardHeader } from "@/components/ui/card";
import { i18n } from "@/i18n";
import { formatDate } from "@/lib/format";
import { useApi } from "@/lib/session";

interface ActivityRow {
  id: number;
  time: number;
  username: string | null;
  event: string;
  payload: string;
}

function describe(a: ActivityRow): string {
  const p = JSON.parse(a.payload);
  const user = a.username ?? i18n.t("admin.dashboard.someone");
  switch (a.event) {
    case "playback_started":
      return i18n.t("admin.dashboard.playback_started", { user, method: p.method });
    case "playback_stopped":
      return i18n.t("admin.dashboard.playback_stopped", { user });
    case "item_added":
      return i18n.t("admin.dashboard.item_added", { title: p.title });
    case "library_scan_started":
      return i18n.t("admin.dashboard.scan_started");
    case "library_scan_finished":
      return i18n.t("admin.dashboard.scan_finished", { added: p.added, removed: p.removed });
    case "user_created":
      return i18n.t("admin.dashboard.user_created", { username: p.username });
    default:
      return a.event;
  }
}

/** Admin > Activity: the activity log (module "Activity log"), on its own page. */
export function ActivityPage() {
  const { t } = useTranslation();
  const activity = useApi<ActivityRow[]>("api/system/activity");

  useEffect(() => {
    const timer = setInterval(activity.reload, 10_000);
    return () => clearInterval(timer);
  }, [activity.reload]);

  return (
    <Card>
      <CardHeader title={t("admin.dashboard.activity")} description={t("admin.dashboard.activity_help")} />
      <div className="divide-y text-sm">
        {activity.data?.length === 0 && (
          <p className="p-5 text-muted-foreground">{t("admin.dashboard.activity_empty")}</p>
        )}
        {activity.data?.map((a) => (
          <div key={a.id} className="flex justify-between gap-4 px-5 py-3">
            <span>{describe(a)}</span>
            <span className="shrink-0 text-muted-foreground">{formatDate(a.time)}</span>
          </div>
        ))}
      </div>
    </Card>
  );
}

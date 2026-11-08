import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Badge, Card, CardHeader, Empty } from "@/components/ui/card";
import { i18n } from "@/i18n";
import { formatDate } from "@/lib/format";
import { useApi } from "@/lib/session";

interface Session {
  id: string;
  username: string;
  title: string;
  method: string;
  hw: string;
  running_seconds: number;
  plan: { reasons: string[] };
}

interface Activity {
  id: number;
  time: number;
  username: string | null;
  event: string;
  payload: string;
}

function describe(a: Activity): string {
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

export function DashboardPage() {
  const { t } = useTranslation();
  const sessions = useApi<Session[]>("api/sessions");
  const activity = useApi<Activity[]>("api/system/activity");

  useEffect(() => {
    const t = setInterval(() => {
      sessions.reload();
      activity.reload();
    }, 5000);
    return () => clearInterval(t);
  }, [sessions.reload, activity.reload]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader title={t("admin.dashboard.streams")} description={t("admin.dashboard.streams_help")} />
        <div className="divide-y">
          {sessions.data?.length === 0 && (
            <div className="p-5">
              <Empty title={t("admin.dashboard.streams_empty")} />
            </div>
          )}
          {sessions.data?.map((s) => (
            <div key={s.id} className="flex flex-wrap items-center justify-between gap-2 p-5 text-sm">
              <div>
                <p className="font-medium">{s.title}</p>
                <p className="text-muted-foreground">
                  {s.username} · {t("admin.dashboard.running_min", { count: Math.round(s.running_seconds / 60) })} ·{" "}
                  {s.plan.reasons[0] ?? ""}
                </p>
              </div>
              <div className="flex gap-2">
                <Badge>{s.method}</Badge>
                <Badge>{s.hw}</Badge>
              </div>
            </div>
          ))}
        </div>
      </Card>
      <Card>
        <CardHeader title={t("admin.dashboard.activity")} />
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
    </div>
  );
}

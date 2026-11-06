import { useEffect } from "react";
import { Badge, Card, CardHeader, Empty } from "@/components/ui/card";
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
  switch (a.event) {
    case "playback_started":
      return `${a.username ?? "Someone"} started playing (${p.method})`;
    case "playback_stopped":
      return `${a.username ?? "Someone"} stopped playing`;
    case "item_added":
      return `Added "${p.title}"`;
    case "library_scan_started":
      return "Library scan started";
    case "library_scan_finished":
      return `Library scan finished: ${p.added} added, ${p.removed} removed`;
    case "user_created":
      return `New user ${p.username}`;
    default:
      return a.event;
  }
}

export function DashboardPage() {
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
        <CardHeader
          title="Active streams"
          description="Direct play streams don't use the transcoder and aren't listed here."
        />
        <div className="divide-y">
          {sessions.data?.length === 0 && (
            <div className="p-5">
              <Empty title="Nothing is transcoding right now" />
            </div>
          )}
          {sessions.data?.map((s) => (
            <div key={s.id} className="flex flex-wrap items-center justify-between gap-2 p-5 text-sm">
              <div>
                <p className="font-medium">{s.title}</p>
                <p className="text-muted-foreground">
                  {s.username} · {Math.round(s.running_seconds / 60)} min · {s.plan.reasons[0] ?? ""}
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
        <CardHeader title="Recent activity" />
        <div className="divide-y text-sm">
          {activity.data?.length === 0 && <p className="p-5 text-muted-foreground">No activity yet.</p>}
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

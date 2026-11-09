import { Gauge, MonitorPlay } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { StreamCard } from "@/components/StreamCard";
import { Empty } from "@/components/ui/card";
import { api } from "@/lib/api";
import { formatBitrate } from "@/lib/playback-stats";
import { livePosition, streamTotals } from "@/lib/streams";
import type { StreamInfo } from "@/lib/types";

/** How often the list of streams is fetched while the page is visible. */
const REFRESH_MS = 2000;

/** Live playbacks, polled every couple of seconds while the tab is visible. */
function useStreams() {
  const [streams, setStreams] = useState<StreamInfo[] | null>(null);
  const [fetchedAt, setFetchedAt] = useState(() => Date.now());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      if (document.hidden) return;
      api
        .get<StreamInfo[]>("api/sessions")
        .then((s) => {
          if (cancelled) return;
          setStreams(s);
          setFetchedAt(Date.now());
          setError(null);
        })
        .catch((e: Error) => !cancelled && setError(e.message));
    };
    load();
    const timer = window.setInterval(load, REFRESH_MS);
    document.addEventListener("visibilitychange", load);
    return () => {
      cancelled = true;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", load);
    };
  }, []);

  return { streams, fetchedAt, error };
}

/** A clock that ticks every second, so markers move between refreshes. */
function useNow() {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

export function StreamsView({
  streams,
  fetchedAt,
  now,
  error,
  defaultOpen,
}: {
  streams: StreamInfo[] | null;
  fetchedAt: number;
  now: number;
  error?: string | null;
  /** Stories: open some cards' sections up front (by stream id). */
  defaultOpen?: Record<string, { info?: boolean; trace?: boolean }>;
}) {
  const { t } = useTranslation();
  const totals = streamTotals(streams ?? []);
  const stats: [string, React.ReactNode][] = [
    [t("admin.streams.total"), totals.total],
    [t("admin.streams.method_direct_play"), totals.direct_play],
    [t("admin.streams.method_remux"), totals.remux],
    [t("admin.streams.method_transcode"), totals.transcode],
    [
      t("admin.streams.bandwidth"),
      <span key="bw" className="inline-flex items-center gap-1.5">
        <Gauge className="h-4 w-4 text-muted-foreground" />
        {totals.bandwidth > 0 ? formatBitrate(totals.bandwidth) : "0 Mbps"}
      </span>,
    ],
  ];

  return (
    <div className="space-y-5">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {stats.map(([label, value]) => (
          <div key={label} className="rounded-lg border bg-card p-4">
            <p className="text-xs text-muted-foreground">{label}</p>
            <p className="mt-1 text-xl font-semibold tabular-nums">{value}</p>
          </div>
        ))}
      </div>

      <div className="flex items-baseline justify-between gap-4">
        <h2 className="flex items-center gap-2 font-semibold">
          <MonitorPlay className="h-4 w-4 text-primary" />
          {t("admin.streams.title")}
        </h2>
        <span className="inline-flex items-center gap-1.5 text-xs text-primary">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-primary" />
          {t("admin.streams.live")}
        </span>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}
      {streams?.length === 0 && <Empty title={t("admin.streams.empty")}>{t("admin.streams.empty_body")}</Empty>}
      <div className="space-y-3">
        {streams?.map((s) => (
          <StreamCard
            key={s.id}
            stream={s}
            position={livePosition(s, fetchedAt, now)}
            now={now}
            defaultOpen={defaultOpen?.[s.id]}
          />
        ))}
      </div>
    </div>
  );
}

/** Admin > Dashboard: everything playing right now, whatever the method. */
export function DashboardPage() {
  const { streams, fetchedAt, error } = useStreams();
  const now = useNow();
  return <StreamsView streams={streams} fetchedAt={fetchedAt} now={now} error={error} />;
}

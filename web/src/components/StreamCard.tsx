import {
  Activity,
  AppWindow,
  AudioLines,
  Check,
  ChevronDown,
  Clock,
  Copy,
  Film,
  Gauge,
  Globe,
  HardDrive,
  Info,
  ListTree,
  LoaderCircle,
  MonitorSmartphone,
  Network,
  Palette,
  Pause,
  Play,
  Server,
  User,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Artwork } from "@/components/Artwork";
import { Card } from "@/components/ui/card";
import { formatPlaybackMarker, progressPercent } from "@/lib/format";
import { infoToText, type InfoIcon, type InfoSection } from "@/lib/playback-stats";
import {
  methodName,
  startedAgo,
  streamBadges,
  streamBandwidth,
  streamInfoSections,
  streamReason,
  streamSubtitle,
  streamTitle,
  streamTraceSections,
} from "@/lib/streams";
import type { PlaybackPlan, StreamInfo } from "@/lib/types";
import { cn } from "@/lib/utils";

const SECTION_ICONS: Record<InfoIcon, LucideIcon> = {
  video: Film,
  audio: AudioLines,
  color: Palette,
  performance: Activity,
  buffer: HardDrive,
  app: AppWindow,
  server: Server,
  network: Network,
  list: ListTree,
};

/** Colour per method: green for direct play, blue for direct stream, amber for transcodes. */
const METHOD_STYLE: Record<PlaybackPlan["method"], string> = {
  direct_play: "border-primary/40 bg-primary/10 text-primary",
  remux: "border-sky-400/40 bg-sky-400/10 text-sky-400",
  transcode: "border-amber-400/40 bg-amber-400/10 text-amber-400",
};

export function MethodBadge({ method }: { method: PlaybackPlan["method"] }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap",
        METHOD_STYLE[method],
      )}
    >
      {methodName(method)}
    </span>
  );
}

function FactBadge({ children }: { children: React.ReactNode }) {
  return (
    <span className="inline-flex items-center rounded-full border bg-muted px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap">
      {children}
    </span>
  );
}

function StateLabel({ state }: { state: StreamInfo["state"] }) {
  const { t } = useTranslation();
  const Icon = state === "playing" ? Play : state === "paused" ? Pause : LoaderCircle;
  return (
    <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
      <Icon className={cn("h-3.5 w-3.5", state === "buffering" && "animate-spin")} />
      {t(`admin.streams.state_${state}`)}
    </span>
  );
}

function SectionGrid({ sections, mono }: { sections: InfoSection[]; mono?: boolean }) {
  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
      {sections.map((sec) => {
        const Icon = SECTION_ICONS[sec.icon];
        return (
          <section key={sec.title} className="min-w-0 rounded-md border bg-card p-3">
            <h4 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold tracking-wide text-primary uppercase">
              <Icon className="h-3.5 w-3.5" /> {sec.title}
            </h4>
            <dl className={cn("text-sm", mono && "font-mono text-xs")}>
              {sec.rows.map(([k, v], i) => (
                <div
                  key={`${k}-${i}`}
                  className="flex justify-between gap-3 border-b border-dashed border-border/70 py-1 last:border-0"
                >
                  <dt className="shrink-0 text-muted-foreground">{k}</dt>
                  <dd className="min-w-0 text-right [overflow-wrap:anywhere]">{v}</dd>
                </div>
              ))}
            </dl>
          </section>
        );
      })}
    </div>
  );
}

function ToggleButton({
  open,
  onClick,
  icon: Icon,
  label,
}: {
  open: boolean;
  onClick: () => void;
  icon: LucideIcon;
  label: string;
}) {
  return (
    <button
      type="button"
      aria-expanded={open}
      onClick={onClick}
      className={cn(
        "inline-flex cursor-pointer items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-medium transition-colors",
        open ? "border-primary/40 bg-accent text-primary" : "hover:bg-accent",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      {label}
      <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", open && "rotate-180")} />
    </button>
  );
}

/**
 * One live playback on the dashboard: poster, title, where they are, who and
 * how. "Info" and "Trace" each open a section under the card.
 */
export function StreamCard({
  stream: s,
  position,
  now,
  defaultOpen,
}: {
  stream: StreamInfo;
  /** Live position (moves between refreshes while playing). */
  position: number | null;
  now: number;
  defaultOpen?: { info?: boolean; trace?: boolean };
}) {
  const { t } = useTranslation();
  const [infoOpen, setInfoOpen] = useState(defaultOpen?.info ?? false);
  const [traceOpen, setTraceOpen] = useState(defaultOpen?.trace ?? false);
  const [copied, setCopied] = useState(false);
  const title = streamTitle(s);
  const subtitle = streamSubtitle(s);
  const marker = formatPlaybackMarker(position, s.duration_ms);
  const percent = progressPercent(position, s.duration_ms);
  const trace = traceOpen ? streamTraceSections(s, now) : [];

  const copyTrace = () => {
    const text = infoToText({ basic: streamInfoSections(s, now), trace: streamTraceSections(s, now) });
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };

  return (
    <Card className="overflow-hidden">
      <div className="flex gap-4 p-4">
        <Artwork
          id={s.item.id}
          kind="poster"
          available
          title={title}
          className="aspect-[2/3] w-20 shrink-0 rounded-md sm:w-24"
        />
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
            <div className="min-w-0">
              <h3 className="truncate text-base font-semibold sm:text-lg">
                {title} {subtitle && <span className="text-sm font-medium text-muted-foreground">{subtitle}</span>}
              </h3>
              <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1">
                <div
                  role="progressbar"
                  aria-label={t("admin.streams.progress")}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(percent)}
                  aria-valuetext={marker}
                  className="h-1.5 w-40 overflow-hidden rounded-full bg-muted sm:w-64"
                >
                  <div className="h-full rounded-full bg-primary" style={{ width: `${percent}%` }} />
                </div>
                <span className="text-sm font-semibold whitespace-nowrap tabular-nums">{marker}</span>
                <StateLabel state={s.state} />
              </div>
            </div>
            <div className="flex shrink-0 flex-wrap gap-1.5 sm:justify-end">
              <MethodBadge method={s.method} />
              {streamBadges(s).map((b) => (
                <FactBadge key={b}>{b}</FactBadge>
              ))}
            </div>
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
            <span className="inline-flex items-center gap-1">
              <User className="h-3.5 w-3.5" />
              {s.user.username}
            </span>
            <span className="inline-flex items-center gap-1">
              <MonitorSmartphone className="h-3.5 w-3.5" />
              {[s.client.name ?? t("admin.streams.unknown_app"), s.client.device].filter(Boolean).join(" · ")}
            </span>
            {s.client.address && (
              <span className="inline-flex items-center gap-1">
                <Globe className="h-3.5 w-3.5" />
                {s.client.address}
              </span>
            )}
            <span className="inline-flex items-center gap-1">
              <Clock className="h-3.5 w-3.5" />
              {startedAgo(s.started_at, now)}
            </span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">{streamReason(s)}</p>
          <div className="mt-auto flex flex-wrap items-center gap-2 pt-3">
            <ToggleButton
              open={infoOpen}
              onClick={() => setInfoOpen((o) => !o)}
              icon={Info}
              label={t("admin.streams.info")}
            />
            <ToggleButton
              open={traceOpen}
              onClick={() => setTraceOpen((o) => !o)}
              icon={ListTree}
              label={t("admin.streams.trace")}
            />
            <span className="ml-auto inline-flex items-center gap-1 text-xs text-muted-foreground">
              <Gauge className="h-3.5 w-3.5" />
              {streamBandwidth(s)}
            </span>
          </div>
        </div>
      </div>
      {infoOpen && (
        <div className="border-t bg-background/40 p-4">
          <h4 className="mb-3 flex items-center gap-1.5 text-sm font-semibold">
            <Info className="h-4 w-4 text-primary" />
            {t("admin.streams.info")}
          </h4>
          <SectionGrid sections={streamInfoSections(s, now)} />
        </div>
      )}
      {traceOpen && (
        <div className="border-t border-dashed bg-background/40 p-4">
          <div className="mb-3 flex items-center justify-between">
            <h4 className="flex items-center gap-1.5 text-sm font-semibold">
              <ListTree className="h-4 w-4 text-primary" />
              {t("admin.streams.trace")}
            </h4>
            <button
              type="button"
              onClick={copyTrace}
              className="inline-flex cursor-pointer items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs hover:bg-accent"
            >
              {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
              {copied ? t("admin.streams.copied") : t("admin.streams.copy_trace")}
            </button>
          </div>
          <SectionGrid sections={trace} mono />
        </div>
      )}
    </Card>
  );
}

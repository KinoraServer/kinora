// What the admin dashboard shows for each live playback (pure, unit tested).
//
// The server reports every playback (GET api/sessions). These helpers turn a
// row into the card's texts and its two expandable sections: "Info" for a
// quick look, "Trace" for the same kind of detail as the player's own Trace tab.

import { i18n } from "@/i18n";
import { formatClock } from "./format";
import { codecName, formatBitrate, hdrLabel, type InfoSection } from "./playback-stats";
import type { PlaybackPlan, StreamInfo } from "./types";

type Method = PlaybackPlan["method"];

export const METHOD_KEY = {
  direct_play: "admin.streams.method_direct_play",
  remux: "admin.streams.method_remux",
  transcode: "admin.streams.method_transcode",
} as const satisfies Record<Method, string>;

export const methodName = (m: Method) => i18n.t(METHOD_KEY[m]);

const or = (v: string | number | null | undefined, fallback = "–") =>
  v === null || v === undefined || v === "" ? fallback : String(v);

/** "Severance", or the show's name for an episode. */
export function streamTitle(s: StreamInfo): string {
  return s.item.kind === "episode" && s.item.series_title ? s.item.series_title : s.item.title;
}

/** "S2 E5 · Trojan's Horse" for episodes, "(2024)" for movies. */
export function streamSubtitle(s: StreamInfo): string {
  const it = s.item;
  if (it.kind === "episode") {
    const code = [it.season != null ? `S${it.season}` : null, it.episode != null ? `E${it.episode}` : null]
      .filter(Boolean)
      .join(" ");
    return [code, it.series_title ? it.title : null].filter(Boolean).join(" · ");
  }
  return it.year ? `(${it.year})` : "";
}

/** "4K", "1080p", ... from a height (and width, for wide films). */
export function resolutionLabel(width: number, height: number): string {
  if (height >= 2000 || width >= 3800) return "4K";
  if (height >= 1000 || width >= 1900) return "1080p";
  if (height >= 700 || width >= 1260) return "720p";
  return "SD";
}

/** Small facts for the badges next to the method: "NVENC", "4K → 1080p", "HEVC · 4K HDR10". */
export function streamBadges(s: StreamInfo): string[] {
  const v = s.media.video;
  const out: string[] = [];
  if (s.plan.method === "transcode" && s.hardware) out.push(s.hardware);
  if (v && s.plan.video?.kind === "encode") {
    const from = resolutionLabel(v.width, v.height);
    const to = resolutionLabel(s.plan.video.width, s.plan.video.height);
    out.push(from === to ? to : `${from} → ${to}`);
  } else if (v) {
    const hdr = v.hdr && v.hdr !== "sdr" ? ` ${hdrLabel(v.hdr, v.dv_profile, v.dv_compat_id)}` : "";
    out.push(`${codecName(v.codec)} · ${resolutionLabel(v.width, v.height)}${hdr}`);
  }
  return out;
}

/** One line saying what the server does to the file and why. */
export function streamReason(s: StreamInfo): string {
  if (s.plan.method === "direct_play") return i18n.t("admin.streams.reason_direct_play");
  const reasons = s.plan.reasons.join("; ") || "–";
  return s.plan.method === "remux"
    ? i18n.t("admin.streams.reason_remux", { reasons })
    : i18n.t("admin.streams.reason_transcode", { reasons });
}

/** "started 12 min ago". */
export function startedAgo(startedAt: number, now: number): string {
  const min = Math.floor(Math.max(0, now - startedAt) / 60000);
  if (min < 1) return i18n.t("admin.streams.started_now");
  if (min < 60) return i18n.t("admin.streams.started_min", { count: min });
  return i18n.t("admin.streams.started_hr", { count: Math.floor(min / 60) });
}

/**
 * Where the viewer is `now`, moving on from what the server said at `fetchedAt`
 * while playing (so the marker ticks between refreshes).
 */
export function livePosition(s: StreamInfo, fetchedAt: number, now: number): number | null {
  if (s.position_ms == null) return null;
  const moved = s.state === "playing" ? Math.max(0, now - fetchedAt) : 0;
  const p = s.position_ms + moved;
  return s.duration_ms > 0 ? Math.min(p, s.duration_ms) : p;
}

export interface StreamTotals {
  total: number;
  direct_play: number;
  remux: number;
  transcode: number;
  /** Bits per second, summed over the streams that know theirs. */
  bandwidth: number;
}

export function streamTotals(streams: StreamInfo[]): StreamTotals {
  const t: StreamTotals = { total: streams.length, direct_play: 0, remux: 0, transcode: 0, bandwidth: 0 };
  for (const s of streams) {
    t[s.method]++;
    t.bandwidth += s.bitrate_bps ?? 0;
  }
  return t;
}

/** "18.2 Mbps · ffmpeg 2.4× realtime", shown on the card's bottom row. */
export function streamBandwidth(s: StreamInfo): string {
  const parts = [
    s.state === "paused" ? `0 Mbps · ${i18n.t("admin.streams.paused_bandwidth")}` : formatBitrate(s.bitrate_bps),
  ];
  const speed = s.transcoder?.speed;
  if (s.state !== "paused" && speed && s.transcoder?.running && !s.transcoder.throttled) {
    parts.push(i18n.t("admin.streams.ffmpeg_speed", { speed: speed.toFixed(1) }));
  }
  return parts.join(" · ");
}

const r = (key: string) => i18n.t(`admin.streams.row.${key}` as "admin.streams.row.source");
const arrow = (from: string, to: string) => `${from} → ${to}`;

/** The card's Info section: what is being sent, and to whom. */
export function streamInfoSections(s: StreamInfo, now: number): InfoSection[] {
  const v = s.media.video;
  const audio = s.media.audio.find((a) => a.index === s.plan.audio_index) ?? s.media.audio[0] ?? null;
  const vOut = s.plan.video;
  const aOut = s.plan.audio;
  const copied = r("copied");
  const direct = s.plan.method === "direct_play";

  const videoSource = v ? [codecName(v.codec), v.profile, `${v.width}×${v.height}`].filter(Boolean).join(" · ") : "–";
  const videoOutput = direct
    ? copied
    : vOut?.kind === "encode"
      ? `H.264 · ${vOut.width}×${vOut.height}`
      : vOut?.kind === "copy"
        ? copied
        : "–";
  const range = v ? hdrLabel(v.hdr, v.dv_profile, v.dv_compat_id) : "–";

  const audioSource = audio
    ? [codecName(audio.codec), audio.channel_layout ?? `${audio.channels}ch`, audio.language?.toUpperCase()]
        .filter(Boolean)
        .join(" · ")
    : "–";
  const audioOutput = direct
    ? copied
    : aOut?.kind === "encode"
      ? `${codecName(aOut.codec)} · ${aOut.channels}ch`
      : aOut?.kind === "copy"
        ? copied
        : "–";
  const container = direct
    ? s.media.container.toUpperCase()
    : arrow(s.media.container.toUpperCase(), s.plan.segments === "fmp4" ? "fMP4 HLS" : "MPEG-TS HLS");

  return [
    {
      title: i18n.t("admin.streams.section.video"),
      icon: "video",
      rows: [
        [r("source"), videoSource],
        [r("output"), videoOutput],
        [r("dynamic_range"), vOut?.kind === "encode" && vOut.tonemap ? arrow(range, "SDR") : range],
        [r("frame_rate"), v?.frame_rate ? v.frame_rate.toFixed(3).replace(/\.?0+$/, "") : "–"],
      ],
    },
    {
      title: i18n.t("admin.streams.section.audio"),
      icon: "audio",
      rows: [
        [r("source"), audioSource],
        [r("output"), audioOutput],
        [r("container"), container],
      ],
    },
    {
      title: i18n.t("admin.streams.section.stream"),
      icon: "network",
      rows: [
        [r("method"), methodName(s.method)],
        [r("source_bitrate"), formatBitrate(s.media.bitrate)],
        [r("stream_bitrate"), formatBitrate(s.bitrate_bps)],
        [r("why"), s.plan.reasons.length ? s.plan.reasons.join("; ") : r("no_errors")],
      ],
    },
    {
      title: i18n.t("admin.streams.section.viewer"),
      icon: "app",
      rows: [
        [r("user"), s.user.username],
        [r("app"), or(s.client.name, i18n.t("admin.streams.unknown_app"))],
        [r("device"), or(s.client.device)],
        [r("address"), or(s.client.address)],
        [r("started"), new Date(s.started_at).toLocaleTimeString(i18n.language)],
        [
          r("last_seen"),
          i18n.t("player.trace.s_ago", { count: Math.max(0, Math.round((now - s.last_seen_at) / 1000)) }),
        ],
      ],
    },
  ];
}

const tr = (key: string) => i18n.t(`player.trace.${key}` as "player.trace.method");

/** The card's Trace section: everything known about the stream, like the player's Trace tab. */
export function streamTraceSections(s: StreamInfo, now: number): InfoSection[] {
  const plan = s.plan;
  const session: InfoSection = {
    title: i18n.t("admin.streams.section.session"),
    icon: "server",
    rows: [
      [tr("session_id"), s.id],
      [tr("method"), `${plan.method} (${methodName(s.method)})`],
      [r("state"), i18n.t(`admin.streams.state_${s.state}`)],
      [tr("url"), s.url],
      [tr("video_plan"), plan.video ? JSON.stringify(plan.video) : i18n.t("common.none")],
      [tr("audio_plan"), plan.audio ? JSON.stringify(plan.audio) : i18n.t("common.none")],
      [tr("audio_stream_index"), or(plan.audio_index)],
      [tr("reasons"), plan.reasons.length ? plan.reasons.join("; ") : i18n.t("common.none")],
      [tr("hardware"), or(s.hardware)],
      [tr("open_for"), formatClock(Math.max(0, now - s.started_at) / 1000)],
    ],
  };

  const out: InfoSection[] = [session];

  const st = s.transcoder;
  if (st) {
    const ahead =
      st.transcoded_through != null ? (st.transcoded_through - st.last_requested_segment) * st.segment_seconds : null;
    out.push({
      title: i18n.t("admin.streams.section.transcoder"),
      icon: "performance",
      rows: [
        [tr("ffmpeg_running"), st.throttled ? tr("ffmpeg_throttled") : i18n.t(st.running ? "common.yes" : "common.no")],
        [tr("encoder"), st.hw],
        [r("speed_fps"), st.speed ? `${st.speed.toFixed(1)}× · ${st.fps ? Math.round(st.fps) : "–"} fps` : "–"],
        [tr("started_at_segment"), String(st.started_at_segment)],
        [tr("last_segment_requested"), String(st.last_requested_segment)],
        [
          tr("transcoded_through"),
          st.transcoded_through == null
            ? tr("nothing_yet")
            : `${st.transcoded_through}${ahead !== null && ahead > 0 ? ` (+${ahead} s)` : ""}`,
        ],
        [tr("last_ffmpeg_line"), or(st.last_ffmpeg_line, i18n.t("common.none"))],
      ],
    });
  }

  const c = s.client_stats;
  out.push({
    title: i18n.t("admin.streams.section.player"),
    icon: "app",
    rows: c
      ? [
          [i18n.t("player.info.render_fps"), c.render_fps == null ? "–" : c.render_fps.toFixed(2)],
          [r("dropped_total"), c.total_frames ? `${c.dropped_frames ?? 0} / ${c.total_frames}` : or(c.dropped_frames)],
          [r("stalls"), c.stalls ? `${c.stalls} (${((c.stalled_ms ?? 0) / 1000).toFixed(1)} s)` : "0"],
          [r("buffered_ahead"), c.buffered_ahead_s == null ? "–" : `${c.buffered_ahead_s.toFixed(1)} s`],
          [r("download_speed"), formatBitrate(c.download_bps)],
          [r("decoded_size"), or(c.decoded_size)],
          [r("player"), or(c.player)],
          [r("recoveries"), String(c.recoveries ?? 0)],
        ]
      : [[r("player"), r("no_client_stats")]],
  });

  out.push({
    title: i18n.t("admin.streams.section.errors"),
    icon: "list",
    rows: c?.recent_errors?.length
      ? c.recent_errors.map((e, i) => [String(i + 1), e] as [string, string])
      : [["", r("no_errors")]],
  });
  return out;
}

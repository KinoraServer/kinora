// Live playback statistics for the player's info panel.
//
// Player.tsx owns a PlaybackStats for each playback. It listens to the
// <video> element and hls.js, keeps a short history (fragment loads, errors,
// the transcoder's status) and turns it all into two sets of sections:
// "basic" (what most people want: codecs, resolution, frame rate, buffer)
// and "trace" (everything else, for debugging a stream).

import type Hls from "hls.js";
import { i18n } from "@/i18n";
import { formatBytes, formatClock } from "./format";
import { describeAutoTracks } from "./subtitles";
import type { MediaFile, PlaybackStart, SessionStatus } from "./types";

export type InfoIcon = "video" | "audio" | "color" | "performance" | "buffer" | "app" | "server" | "network" | "list";

export interface InfoSection {
  title: string;
  icon: InfoIcon;
  rows: [label: string, value: string][];
}

export interface PlayerInfo {
  basic: InfoSection[];
  trace: InfoSection[];
}

/** What the page knows that the stats can't see for themselves. */
export interface InfoContext {
  playback: PlaybackStart | null;
  file: MediaFile | undefined;
  audioIndex: number | null;
  subtitleLabel: string | null;
}

// ---------------------------------------------------------------------------
// Formatting helpers (exported for tests)
// ---------------------------------------------------------------------------

const CODEC_NAMES: Record<string, string> = {
  h264: "H.264",
  hevc: "HEVC",
  av1: "AV1",
  vp9: "VP9",
  vp8: "VP8",
  mpeg2video: "MPEG-2",
  mpeg4: "MPEG-4",
  aac: "AAC",
  ac3: "AC3",
  eac3: "E-AC3",
  truehd: "TrueHD",
  dts: "DTS",
  flac: "FLAC",
  opus: "Opus",
  mp3: "MP3",
  vorbis: "Vorbis",
  pcm_s16le: "PCM",
  pcm_s24le: "PCM",
};

export function codecName(codec: string | null | undefined): string {
  if (!codec) return "–";
  return CODEC_NAMES[codec.toLowerCase()] ?? codec.toUpperCase();
}

export function formatBitrate(bps: number | null | undefined): string {
  if (!bps || !Number.isFinite(bps) || bps <= 0) return "–";
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(bps >= 10_000_000 ? 0 : 1)} Mbps`;
  return `${Math.round(bps / 1000)} kbps`;
}

/** 1920×1080 → "16:9"; odd sizes fall back to a decimal ratio like "2.39:1". */
export function aspectRatio(width: number, height: number): string {
  if (!width || !height) return "–";
  const known: [number, string][] = [
    [16 / 9, "16:9"],
    [4 / 3, "4:3"],
    [21 / 9, "21:9"],
    [1.85, "1.85:1"],
    [2.39, "2.39:1"],
    [2.35, "2.35:1"],
    [1, "1:1"],
  ];
  const r = width / height;
  const match = known.find(([k]) => Math.abs(k - r) < 0.01);
  return match ? match[1] : `${r.toFixed(2)}:1`;
}

const READY_STATES = ["HAVE_NOTHING", "HAVE_METADATA", "HAVE_CURRENT_DATA", "HAVE_FUTURE_DATA", "HAVE_ENOUGH_DATA"];
const NETWORK_STATES = ["NETWORK_EMPTY", "NETWORK_IDLE", "NETWORK_LOADING", "NETWORK_NO_SOURCE"];

export function readyStateName(n: number): string {
  return `${n} ${READY_STATES[n] ?? ""}`.trim();
}

export function networkStateName(n: number): string {
  return `${n} ${NETWORK_STATES[n] ?? ""}`.trim();
}

/** "0.0–12.4, 30.0–61.2" */
export function formatRanges(r: TimeRanges): string {
  const out: string[] = [];
  for (let i = 0; i < r.length; i++) out.push(`${r.start(i).toFixed(1)}–${r.end(i).toFixed(1)}`);
  return out.length ? out.join(", ") : i18n.t("common.none");
}

/** Seconds buffered ahead of `t`. */
export function bufferedAhead(r: TimeRanges, t: number): number {
  for (let i = 0; i < r.length; i++) {
    if (r.start(i) <= t + 0.5 && r.end(i) >= t) return Math.max(0, r.end(i) - t);
  }
  return 0;
}

function hdrLabel(hdr: string | undefined, dvProfile?: number | null, compat?: number | null): string {
  switch (hdr) {
    case "hdr10":
      return "HDR10";
    case "hlg":
      return "HLG";
    case "dolby_vision":
      return `Dolby Vision${dvProfile ? ` ${dvProfile}${compat ? `.${compat}` : ""}` : ""}`;
    default:
      return "SDR";
  }
}

const yesNo = (b: boolean) => i18n.t(b ? "common.yes" : "common.no");
const or = (v: string | number | null | undefined, fallback = "–") =>
  v === null || v === undefined || v === "" ? fallback : String(v);

// ---------------------------------------------------------------------------
// The collector
// ---------------------------------------------------------------------------

interface FragLog {
  sn: number | string;
  bytes: number;
  ms: number;
  duration: number;
  at: number;
}

interface FrameSample {
  at: number;
  total: number;
  dropped: number;
}

type ChromeVideo = HTMLVideoElement & {
  webkitDecodedFrameCount?: number;
  webkitDroppedFrameCount?: number;
  webkitVideoDecodedByteCount?: number;
  webkitAudioDecodedByteCount?: number;
};

const KEEP_FRAGS = 12;
const KEEP_ERRORS = 8;

export class PlaybackStats {
  private hls: Hls | null = null;
  private hlsEvents: (typeof Hls)["Events"] | null = null;
  private hlsVersion: string | null = null;
  private frags: FragLog[] = [];
  private totalBytes = 0;
  private errors: string[] = [];
  private frames: FrameSample[] = [];
  private stalls = 0;
  private waitingSince: number | null = null;
  private stalledMs = 0;
  private recoveries = 0;
  private status: SessionStatus | null = null;
  private statusAt = 0;
  private readonly started = performance.now();

  constructor(private readonly video: HTMLVideoElement) {
    video.addEventListener("waiting", this.onWaiting);
    video.addEventListener("playing", this.onPlaying);
  }

  dispose() {
    this.video.removeEventListener("waiting", this.onWaiting);
    this.video.removeEventListener("playing", this.onPlaying);
    this.detachHls();
  }

  private onWaiting = () => {
    if (this.waitingSince === null && !this.video.seeking) {
      this.stalls++;
      this.waitingSince = performance.now();
    }
  };

  private onPlaying = () => {
    if (this.waitingSince !== null) this.stalledMs += performance.now() - this.waitingSince;
    this.waitingSince = null;
  };

  /** Follow an hls.js instance (call again after a restart). */
  attachHls(hls: Hls, HlsClass: typeof Hls) {
    this.detachHls();
    this.hls = hls;
    this.hlsEvents = HlsClass.Events;
    this.hlsVersion = HlsClass.version;
    hls.on(HlsClass.Events.FRAG_LOADED, this.onFrag);
    hls.on(HlsClass.Events.ERROR, this.onHlsError);
  }

  private detachHls() {
    if (this.hls && this.hlsEvents) {
      this.hls.off(this.hlsEvents.FRAG_LOADED, this.onFrag);
      this.hls.off(this.hlsEvents.ERROR, this.onHlsError);
    }
    this.hls = null;
  }

  private onFrag = (_e: unknown, data: { frag: { sn: number | string; duration: number; stats: FragStats } }) => {
    const st = data.frag.stats;
    const bytes = st.loaded || st.total || 0;
    const ms = Math.max(1, st.loading.end - st.loading.start);
    this.totalBytes += bytes;
    this.frags.push({ sn: data.frag.sn, bytes, ms, duration: data.frag.duration, at: performance.now() });
    if (this.frags.length > KEEP_FRAGS) this.frags.shift();
  };

  private onHlsError = (
    _e: unknown,
    data: { type: string; details: string; fatal: boolean; response?: { code?: number } },
  ) => {
    this.note(`${data.fatal ? "fatal " : ""}${data.details}${data.response?.code ? ` (${data.response.code})` : ""}`);
  };

  note(message: string) {
    const time = formatClock(this.video.currentTime);
    this.errors.push(`${new Date().toLocaleTimeString()} @${time} ${message}`);
    if (this.errors.length > KEEP_ERRORS) this.errors.shift();
  }

  noteRecovery() {
    this.recoveries++;
  }

  setSessionStatus(status: SessionStatus | null) {
    this.status = status;
    this.statusAt = performance.now();
  }

  /** Frames shown per second over the last ~2 s, measured. */
  private renderFps(): { fps: number | null; dropped: number; total: number } {
    const q = this.video.getVideoPlaybackQuality?.();
    if (!q) return { fps: null, dropped: 0, total: 0 };
    const now = performance.now();
    this.frames.push({ at: now, total: q.totalVideoFrames, dropped: q.droppedVideoFrames });
    while (this.frames.length > 2 && now - this.frames[0].at > 2000) this.frames.shift();
    const first = this.frames[0];
    const span = (now - first.at) / 1000;
    const fps =
      span > 0.5 && !this.video.paused
        ? (q.totalVideoFrames - q.droppedVideoFrames - (first.total - first.dropped)) / span
        : null;
    return { fps, dropped: q.droppedVideoFrames, total: q.totalVideoFrames };
  }

  /** Download speed over the recent fragments. */
  private throughput(): number | null {
    const recent = this.frags.slice(-4);
    const bytes = recent.reduce((n, f) => n + f.bytes, 0);
    const ms = recent.reduce((n, f) => n + f.ms, 0);
    return ms > 0 ? (bytes * 8 * 1000) / ms : null;
  }

  snapshot(ctx: InfoContext): PlayerInfo {
    const v = this.video as ChromeVideo;
    const { playback, file } = ctx;
    const info = file?.info ?? null;
    const src = info?.video ?? null;
    const plan = playback?.plan;
    const audio = info?.audio.find((a) => a.index === (ctx.audioIndex ?? plan?.audio_index)) ?? info?.audio[0] ?? null;
    const videoOut = plan?.video;
    const audioOut = plan?.audio;
    const transcodingVideo = videoOut?.kind === "encode";
    const transcodingAudio = audioOut?.kind === "encode";
    const frames = this.renderFps();
    const ahead = bufferedAhead(v.buffered, v.currentTime);
    const level = this.hls && this.hls.currentLevel >= 0 ? this.hls.levels[this.hls.currentLevel] : null;
    const method = playback ? methodLabel(playback.method) : "–";
    const mem = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory;
    const throughput = this.throughput();

    // ---- basic ---------------------------------------------------------
    const videoRows: [string, string][] = [
      [i18n.t("player.info.codec"), transcodingVideo ? `${codecName(src?.codec)} → H.264` : codecName(src?.codec)],
      [
        i18n.t("player.info.resolution"),
        transcodingVideo && videoOut.kind === "encode"
          ? `${src ? `${src.width}×${src.height}` : "–"} → ${videoOut.width}×${videoOut.height}`
          : src
            ? `${src.width}×${src.height}`
            : v.videoWidth
              ? `${v.videoWidth}×${v.videoHeight}`
              : "–",
      ],
      [i18n.t("player.info.fps"), src?.frame_rate ? src.frame_rate.toFixed(3).replace(/\.?0+$/, "") : "–"],
      [
        i18n.t("player.info.dynamic_range"),
        `${hdrLabel(src?.hdr, src?.dv_profile, src?.dv_compat_id)}${src ? ` · ${src.bit_depth}-bit` : ""}`,
      ],
      [i18n.t("player.info.aspect"), aspectRatio(v.videoWidth || src?.width || 0, v.videoHeight || src?.height || 0)],
    ];
    if (src?.profile) videoRows.splice(1, 0, [i18n.t("player.info.profile"), src.profile]);

    const audioRows: [string, string][] = [
      [
        i18n.t("player.info.codec"),
        transcodingAudio && audioOut.kind === "encode"
          ? `${codecName(audio?.codec)} → ${codecName(audioOut.codec)}`
          : codecName(audio?.codec),
      ],
      [
        i18n.t("player.info.channels"),
        transcodingAudio && audioOut.kind === "encode"
          ? `${audio?.channel_layout ?? audio?.channels ?? "–"} → ${audioOut.channels}ch`
          : or(audio?.channel_layout ?? (audio ? `${audio.channels}ch` : null)),
      ],
      [i18n.t("player.info.sample_rate"), audio?.sample_rate ? `${(audio.sample_rate / 1000).toFixed(1)} kHz` : "–"],
      [i18n.t("player.info.bitrate"), formatBitrate(audio?.bitrate)],
      [i18n.t("player.info.language"), or(audio?.language?.toUpperCase() ?? audio?.title)],
    ];

    const colorRows: [string, string][] = [
      [i18n.t("player.info.pixel_format"), or(src?.pix_fmt)],
      [i18n.t("player.info.matrix"), or(src?.color_space)],
      [i18n.t("player.info.primaries"), or(src?.color_primaries)],
      [i18n.t("player.info.transfer"), or(src?.color_transfer)],
      [
        i18n.t("player.info.range"),
        or(src?.color_range === "tv" ? "limited (tv)" : src?.color_range === "pc" ? "full (pc)" : src?.color_range),
      ],
    ];

    const perfRows: [string, string][] = [
      [
        i18n.t("player.info.render_fps"),
        frames.fps === null
          ? v.paused
            ? i18n.t("player.info.paused")
            : i18n.t("player.info.measuring")
          : frames.fps.toFixed(2),
      ],
      [
        i18n.t("player.info.dropped_frames"),
        frames.total ? `${frames.dropped} of ${frames.total}` : String(frames.dropped),
      ],
      [i18n.t("player.info.stalls"), this.stalls ? `${this.stalls} (${(this.stalledMs / 1000).toFixed(1)} s)` : "0"],
      [i18n.t("player.info.speed"), `${v.playbackRate}×`],
    ];

    const bufferRows: [string, string][] = [
      [i18n.t("player.info.buffered_ahead"), `${ahead.toFixed(1)} s`],
      [i18n.t("player.info.download_speed"), formatBitrate(throughput)],
      [
        i18n.t("player.info.stream_bitrate"),
        formatBitrate(
          level?.bitrate ?? (transcodingVideo && videoOut.kind === "encode" ? videoOut.bitrate : info?.bitrate),
        ),
      ],
      [i18n.t("player.info.data_loaded"), this.hls ? formatBytes(this.totalBytes) : "–"],
    ];

    const appRows: [string, string][] = [
      [i18n.t("player.info.player"), this.hls ? `Kinora web · hls.js ${this.hlsVersion}` : "Kinora web · native"],
      [i18n.t("player.info.play_method"), method],
      ...(playback?.method === "transcode"
        ? ([[i18n.t("player.info.transcoder"), playback.hardware]] as [string, string][])
        : []),
      [i18n.t("player.info.memory"), mem ? formatBytes(mem.usedJSHeapSize) : "–"],
      [i18n.t("player.subtitles"), or(ctx.subtitleLabel, i18n.t("player.subtitles_off"))],
    ];

    const basic: InfoSection[] = [
      { title: i18n.t("player.info.video"), icon: "video", rows: videoRows },
      { title: i18n.t("player.info.audio"), icon: "audio", rows: audioRows },
      { title: i18n.t("player.info.color"), icon: "color", rows: colorRows },
      { title: i18n.t("player.info.performance"), icon: "performance", rows: perfRows },
      { title: i18n.t("player.info.buffer"), icon: "buffer", rows: bufferRows },
      { title: i18n.t("player.info.app"), icon: "app", rows: appRows },
    ];

    // ---- trace ---------------------------------------------------------
    const q = v.getVideoPlaybackQuality?.();
    const element: [string, string][] = [
      [i18n.t("player.trace.current_time"), v.currentTime.toFixed(3)],
      [i18n.t("player.trace.duration"), Number.isFinite(v.duration) ? v.duration.toFixed(3) : String(v.duration)],
      [i18n.t("player.trace.ready_state"), readyStateName(v.readyState)],
      [i18n.t("player.trace.network_state"), networkStateName(v.networkState)],
      [i18n.t("player.trace.paused_ended_seeking"), `${yesNo(v.paused)} / ${yesNo(v.ended)} / ${yesNo(v.seeking)}`],
      [i18n.t("player.trace.buffered"), formatRanges(v.buffered)],
      [i18n.t("player.trace.seekable"), formatRanges(v.seekable)],
      [i18n.t("player.trace.decoded_size"), v.videoWidth ? `${v.videoWidth}×${v.videoHeight}` : "–"],
      [i18n.t("player.trace.volume"), `${Math.round(v.volume * 100)}%${v.muted ? i18n.t("player.trace.muted") : ""}`],
      [
        i18n.t("player.trace.frames"),
        q ? `${q.totalVideoFrames} / ${q.droppedVideoFrames} / ${q.corruptedVideoFrames ?? 0}` : "–",
      ],
      ...(v.webkitDecodedFrameCount !== undefined
        ? ([
            [i18n.t("player.trace.decoded_frames_webkit"), String(v.webkitDecodedFrameCount)],
            [i18n.t("player.trace.video_bytes_decoded"), formatBytes(v.webkitVideoDecodedByteCount ?? 0)],
            [i18n.t("player.trace.audio_bytes_decoded"), formatBytes(v.webkitAudioDecodedByteCount ?? 0)],
          ] as [string, string][])
        : []),
      [i18n.t("player.trace.current_src"), v.currentSrc ? v.currentSrc.slice(0, 80) : "–"],
      [i18n.t("player.trace.media_error"), v.error ? `${v.error.code} ${v.error.message}` : i18n.t("common.none")],
    ];

    const session: [string, string][] = [
      [i18n.t("player.trace.method"), playback ? `${playback.method} (${method})` : "–"],
      [i18n.t("player.trace.session_id"), or(playback?.session_id, i18n.t("player.trace.none_direct_play"))],
      [i18n.t("player.trace.url"), or(playback?.url)],
      [i18n.t("player.trace.video_plan"), plan?.video ? JSON.stringify(plan.video) : i18n.t("common.none")],
      [i18n.t("player.trace.audio_plan"), plan?.audio ? JSON.stringify(plan.audio) : i18n.t("common.none")],
      [i18n.t("player.trace.audio_stream_index"), or(plan?.audio_index)],
      [i18n.t("player.trace.tracks_picked_from"), describeAutoTracks(playback)],
      [i18n.t("player.trace.reasons"), plan?.reasons.length ? plan.reasons.join("; ") : i18n.t("common.none")],
      [i18n.t("player.trace.hardware"), or(playback?.hardware)],
      [i18n.t("player.trace.recoveries"), String(this.recoveries)],
      [i18n.t("player.trace.open_for"), formatClock((performance.now() - this.started) / 1000)],
    ];

    const st = this.status;
    const transcoder: [string, string][] = st
      ? [
          [i18n.t("player.trace.ffmpeg_running"), yesNo(st.running)],
          [i18n.t("player.trace.encoder"), st.hw],
          [
            i18n.t("player.trace.started_at_segment"),
            `${st.started_at_segment} (${formatClock(st.started_at_segment * st.segment_seconds)})`,
          ],
          [
            i18n.t("player.trace.last_segment_requested"),
            `${st.last_requested_segment} (${formatClock(st.last_requested_segment * st.segment_seconds)})`,
          ],
          [
            i18n.t("player.trace.transcoded_through"),
            st.transcoded_through === null
              ? i18n.t("player.trace.nothing_yet")
              : `${st.transcoded_through} (${formatClock((st.transcoded_through + 1) * st.segment_seconds)}, ${i18n.t("player.trace.ahead_of_playback", { count: Math.round((st.transcoded_through + 1) * st.segment_seconds - v.currentTime) })})`,
          ],
          [i18n.t("player.trace.session_age"), formatClock(st.running_seconds)],
          [i18n.t("player.trace.last_ffmpeg_line"), or(st.last_ffmpeg_line, i18n.t("common.none"))],
          [
            i18n.t("player.trace.status_updated"),
            i18n.t("player.trace.s_ago", { count: Math.round((performance.now() - this.statusAt) / 1000) }),
          ],
        ]
      : [["status", playback?.session_id ? i18n.t("player.trace.waiting_update") : i18n.t("player.trace.no_session")]];

    const hlsRows: [string, string][] = this.hls
      ? [
          [i18n.t("player.trace.version"), or(this.hlsVersion)],
          [
            i18n.t("player.trace.level"),
            level ? `${level.width}×${level.height} · ${formatBitrate(level.bitrate)}` : "–",
          ],
          [
            i18n.t("player.trace.codecs"),
            level ? [level.videoCodec, level.audioCodec].filter(Boolean).join(", ") || "–" : "–",
          ],
          [i18n.t("player.trace.bandwidth_estimate"), formatBitrate(this.hls.bandwidthEstimate)],
          [
            i18n.t("player.trace.target_buffer"),
            `${this.hls.config.maxBufferLength} s (back ${this.hls.config.backBufferLength} s)`,
          ],
          [
            i18n.t("player.trace.fragments_loaded"),
            `${this.frags.length ? this.frags[this.frags.length - 1].sn : "–"} (last) · ${formatBytes(this.totalBytes)} total`,
          ],
        ]
      : [["hls.js", i18n.t("player.trace.not_used")]];

    const fragRows: [string, string][] = this.frags
      .slice()
      .reverse()
      .map((f) => [
        `seg ${f.sn}`,
        `${formatBytes(f.bytes)} ${i18n.t("player.trace.frag_in", { ms: Math.round(f.ms) })} · ${formatBitrate((f.bytes * 8 * 1000) / f.ms)} · ${f.duration.toFixed(1)} s · ${i18n.t("player.trace.s_ago", { count: Math.round((performance.now() - f.at) / 1000) })}`,
      ]);

    const fileRows: [string, string][] = file
      ? [
          [i18n.t("player.trace.path"), file.path],
          [i18n.t("player.trace.size"), formatBytes(file.size)],
          [i18n.t("player.trace.container"), or(info?.container)],
          [i18n.t("player.trace.overall_bitrate"), formatBitrate(info?.bitrate)],
          [
            i18n.t("player.trace.video_stream"),
            src
              ? `#${src.index ?? "?"} ${src.codec} ${src.profile ?? ""} ${formatBitrate(src.bitrate)}`.trim()
              : i18n.t("common.none"),
          ],
          ...(info?.audio ?? []).map(
            (a) =>
              [
                `audio #${a.index}`,
                [
                  a.codec,
                  a.profile,
                  a.channel_layout ?? `${a.channels}ch`,
                  a.language,
                  a.title,
                  a.is_default ? i18n.t("player.info.default") : null,
                ]
                  .filter(Boolean)
                  .join(" · "),
              ] as [string, string],
          ),
          ...(info?.subtitles ?? []).map(
            (s) =>
              [
                `subtitle #${s.index}`,
                [s.codec, s.language, s.title, s.is_text ? i18n.t("player.info.text") : i18n.t("player.info.image")]
                  .filter(Boolean)
                  .join(" · "),
              ] as [string, string],
          ),
        ]
      : [["file", "–"]];

    const nav = navigator as Navigator & {
      connection?: { effectiveType?: string; downlink?: number };
      deviceMemory?: number;
    };
    const canPlay = (t: string) => v.canPlayType(t) || "no";
    const mseCan = (t: string) => (typeof MediaSource !== "undefined" && MediaSource.isTypeSupported(t) ? "yes" : "no");
    const deviceRows: [string, string][] = [
      [i18n.t("player.trace.viewport"), `${window.innerWidth}×${window.innerHeight} @${window.devicePixelRatio}x`],
      [i18n.t("player.trace.screen"), `${screen.width}×${screen.height}`],
      [
        i18n.t("player.trace.hevc"),
        `${mseCan('video/mp4; codecs="hvc1.1.6.L120.90"')} / ${canPlay('video/mp4; codecs="hvc1.1.6.L120.90"')}`,
      ],
      [i18n.t("player.trace.av1"), mseCan('video/mp4; codecs="av01.0.08M.08"')],
      [i18n.t("player.trace.eac3"), mseCan('audio/mp4; codecs="ec-3"')],
      [i18n.t("player.trace.cpu_threads"), or(navigator.hardwareConcurrency)],
      [i18n.t("player.trace.device_memory"), nav.deviceMemory ? `${nav.deviceMemory} GB` : "–"],
      [
        i18n.t("player.trace.connection"),
        nav.connection?.effectiveType
          ? `${nav.connection.effectiveType} · ${nav.connection.downlink ?? "?"} Mbps`
          : "–",
      ],
      [i18n.t("player.trace.user_agent"), navigator.userAgent],
    ];

    const trace: InfoSection[] = [
      { title: i18n.t("player.trace.session"), icon: "server", rows: session },
      { title: i18n.t("player.info.transcoder"), icon: "server", rows: transcoder },
      { title: i18n.t("player.trace.video_element"), icon: "video", rows: element },
      { title: "hls.js", icon: "network", rows: hlsRows },
      {
        title: i18n.t("player.trace.recent_segments"),
        icon: "list",
        rows: fragRows.length ? fragRows : [["–", i18n.t("player.trace.none_yet")]],
      },
      {
        title: i18n.t("player.trace.errors"),
        icon: "list",
        rows: this.errors.length ? this.errors.map((e, i) => [`#${i + 1}`, e]) : [["–", i18n.t("common.none")]],
      },
      { title: i18n.t("player.trace.source_file"), icon: "list", rows: fileRows },
      { title: i18n.t("player.trace.device"), icon: "app", rows: deviceRows },
    ];

    return { basic, trace };
  }
}

interface FragStats {
  loaded: number;
  total: number;
  loading: { start: number; end: number };
}

const METHOD_KEY: Record<PlaybackStart["method"], string> = {
  direct_play: "player.method_direct_play",
  remux: "player.method_remux",
  transcode: "player.method_transcode",
};

const methodLabel = (m: PlaybackStart["method"]) => i18n.t(METHOD_KEY[m] as "player.method_direct_play");

/** Plain text of every section, for "Copy" (bug reports). */
export function infoToText(info: PlayerInfo): string {
  return [...info.basic, ...info.trace]
    .map((s) => `## ${s.title}\n${s.rows.map(([k, v]) => `${k}: ${v}`).join("\n")}`)
    .join("\n\n");
}

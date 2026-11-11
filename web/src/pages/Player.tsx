import Hls from "hls.js";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { MediaPlayer, type PlayerChoice } from "@/components/MediaPlayer";
import { i18n } from "@/i18n";
import { api } from "@/lib/api";
import { browserCaps } from "@/lib/caps";
import { PlaybackStats, bufferedAhead } from "@/lib/playback-stats";
import { NUDGE_S, StallWatchdog, startGapTarget } from "@/lib/stall-watchdog";
import { episodeCode } from "@/lib/format";
import { readSubtitlePreference, saveSubtitlePreference, startSubtitle } from "@/lib/subtitles";
import { useApi } from "@/lib/session";
import type { ItemDetail, PlaybackStart, SessionStatus } from "@/lib/types";

/** How often the player tells the server it's still here (paused counts). */
const KEEPALIVE_MS = 10_000;
/** How often the player reports its position and stats for the admin dashboard. */
const HEARTBEAT_MS = 5_000;
/** Give up recovering a lost stream after this many tries in a minute. */
const MAX_RECOVERIES = 3;
/** Buffer ahead when the server doesn't say (older servers). */
const DEFAULT_BUFFER_S = 30;
/** Played video kept behind the playhead for quick seeks back. */
const BACK_BUFFER_S = 30;

const METHOD_KEY: Record<
  PlaybackStart["method"],
  "player.method_direct_play" | "player.method_remux" | "player.method_transcode"
> = {
  direct_play: "player.method_direct_play",
  remux: "player.method_remux",
  transcode: "player.method_transcode",
};

const methodLabel = (m: PlaybackStart["method"]) => i18n.t(METHOD_KEY[m]);

/** Fire-and-forget request that survives page unload. */
function beacon(method: string, url: string, body: unknown) {
  fetch(url, {
    method,
    keepalive: true,
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  }).catch(() => {});
}

/** Remounted per item, so going to another episode starts fresh (its own tracks, no leftover audio pick). */
export function PlayerPage() {
  const { id } = useParams();
  return <PlayerView key={id} id={id} />;
}

function PlayerView({ id }: { id: string | undefined }) {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const { data: item } = useApi<ItemDetail>(`api/items/${id}`, [id]);

  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const sessionRef = useRef<string | null>(null);
  /** This playback on the admin dashboard. */
  const playRef = useRef<string | null>(null);
  const durationRef = useRef(0);

  const [playback, setPlayback] = useState<PlaybackStart | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [audioIndex, setAudioIndex] = useState<number | null>(null);
  const [forceTranscode, setForceTranscode] = useState(false);
  const [subIndex, setSubIndex] = useState<number | null>(null);
  // Bumped to start a fresh stream at the current position (the old one was cleaned up).
  const [restartKey, setRestartKey] = useState(0);
  const startAt = useRef(Number(params.get("t") ?? 0));
  const stayPaused = useRef(false);
  /** Where the viewer is (or is seeking to) and whether they want it playing, kept while
   *  the media pipeline is healthy: a broken one reports 0 and paused. */
  const wanted = useRef({ time: 0, playing: true });
  const recoveries = useRef<number[]>([]);
  const statsRef = useRef<PlaybackStats | null>(null);

  const file = item?.files[0];
  // The subtitle track is picked once per video. Restarts (an audio change, a lost stream) keep it.
  const subPicked = useRef(false);

  /** Tell the server what the viewer picked, so other episodes (and devices) start with it. */
  const rememberTracks = useCallback(
    (audio: number | null, sub: number | null) => {
      if (!file) return;
      api.put(`api/playback/${file.id}/tracks`, { audio_index: audio, subtitle_index: sub }).catch(() => {});
    },
    [file],
  );

  const saveProgress = useCallback(() => {
    const v = videoRef.current;
    if (!v || !id || !durationRef.current) return;
    beacon("POST", `api/items/${id}/progress`, {
      position_ms: Math.floor(v.currentTime * 1000),
      duration_ms: durationRef.current,
    });
  }, [id]);

  const stopSession = useCallback(() => {
    const stop = { item_id: id, position_ms: Math.floor((videoRef.current?.currentTime ?? 0) * 1000) };
    if (sessionRef.current) {
      // Also takes it off the dashboard.
      beacon("DELETE", `api/hls/${sessionRef.current}`, stop);
      sessionRef.current = null;
    } else if (playRef.current) {
      beacon("DELETE", `api/now-playing/${playRef.current}`, stop);
    }
    playRef.current = null;
    hlsRef.current?.destroy();
    hlsRef.current = null;
  }, [id]);

  // Live numbers for the info panel.
  useEffect(() => {
    const stats = new PlaybackStats(videoRef.current!);
    statsRef.current = stats;
    return () => {
      stats.dispose();
      statsRef.current = null;
    };
  }, []);

  // The server cleans up streams nobody asked about for a while. If that
  // happened (a long pause, a laptop asleep), quietly start a new one where we were.
  // Also used when the browser's media pipeline breaks or wedges (`sessionGone`
  // false): then the old stream is stopped on the server first.
  const recover = useCallback((why: string, sessionGone = true) => {
    const v = videoRef.current;
    const now = Date.now();
    recoveries.current = recoveries.current.filter((t) => now - t < 60_000);
    if (recoveries.current.length >= MAX_RECOVERIES) {
      setError(i18n.t("player.playback_error", { reason: why }));
      return;
    }
    recoveries.current.push(now);
    statsRef.current?.note(`stream lost (${why}), starting a new one`);
    statsRef.current?.noteRecovery();
    if (v) {
      startAt.current = wanted.current.time || v.currentTime || startAt.current;
      stayPaused.current = !wanted.current.playing;
    }
    if (sessionGone) sessionRef.current = null; // already gone on the server
    setRestartKey((k) => k + 1);
  }, []);

  // Start (or restart, when the audio track changes) playback.
  useEffect(() => {
    if (!file) return;
    let cancelled = false;
    const video = videoRef.current!;
    setError(null);
    setPlayback(null);

    api
      .post<PlaybackStart>(`api/playback/${file.id}`, {
        caps: browserCaps(),
        audio_index: audioIndex,
        force_transcode: forceTranscode,
        client: { name: "Kinora web" },
        start_ms: Math.floor(startAt.current * 1000),
      })
      .then((p) => {
        if (cancelled) return;
        stopSession();
        setPlayback(p);
        if (!subPicked.current) {
          subPicked.current = true;
          setSubIndex(startSubtitle(p, readSubtitlePreference()));
        }
        durationRef.current = p.duration_ms;
        sessionRef.current = p.session_id;
        playRef.current = p.play_id;
        const start = startAt.current;

        if (p.method === "direct_play") {
          video.src = p.url;
          video.addEventListener("loadedmetadata", () => start > 0 && (video.currentTime = start), { once: true });
        } else if (Hls.isSupported()) {
          const hls = new Hls({
            startPosition: start,
            maxBufferLength: p.buffer_seconds ?? DEFAULT_BUFFER_S,
            backBufferLength: BACK_BUFFER_S,
          });
          hlsRef.current = hls;
          statsRef.current?.attachHls(hls, Hls);
          // Copied video often starts a fraction of a second after 0. Start
          // there, rather than letting hls.js jump the gap mid-play.
          hls.once(Hls.Events.FRAG_BUFFERED, () => {
            const to = startGapTarget(video.buffered, video.currentTime);
            if (to === null) return;
            statsRef.current?.note(`stream starts at ${to.toFixed(3)} s, starting there`);
            video.currentTime = to;
          });
          hls.loadSource(p.url);
          hls.attachMedia(video);
          hls.on(Hls.Events.ERROR, (_e, data) => {
            if (!data.fatal) return;
            // 404: the server already cleaned this stream up.
            if (data.type === Hls.ErrorTypes.NETWORK_ERROR && data.response?.code === 404) recover(data.details);
            // A broken media pipeline (decode error, MediaSource reset): start a fresh stream here.
            else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) recover(data.details, false);
            else setError(i18n.t("player.playback_error", { reason: data.details }));
          });
        } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
          video.src = p.url; // Safari / iOS native HLS
          video.addEventListener("loadedmetadata", () => start > 0 && (video.currentTime = start), { once: true });
        } else {
          setError(i18n.t("player.no_hls"));
          return;
        }
        if (stayPaused.current) {
          stayPaused.current = false;
          return;
        }
        video.play().catch(() => {
          // Autoplay blocked: the user presses play.
        });
      })
      .catch((e: Error) => !cancelled && setError(e.message));

    return () => {
      cancelled = true;
    };
  }, [file, audioIndex, forceTranscode, stopSession, restartKey, recover]);

  // Remember the viewer's position and play/pause intent for `recover`.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const healthy = () => !v.error && v.readyState > 0;
    const onTime = () => {
      if (healthy() && v.currentTime > 0) wanted.current.time = v.currentTime;
    };
    const onSeeking = () => {
      wanted.current.time = v.currentTime;
    };
    const onPlay = () => {
      wanted.current.playing = true;
    };
    const onPause = () => {
      if (healthy()) wanted.current.playing = false;
    };
    v.addEventListener("timeupdate", onTime);
    v.addEventListener("seeking", onSeeking);
    v.addEventListener("play", onPlay);
    v.addEventListener("pause", onPause);
    return () => {
      v.removeEventListener("timeupdate", onTime);
      v.removeEventListener("seeking", onSeeking);
      v.removeEventListener("play", onPlay);
      v.removeEventListener("pause", onPause);
    };
  }, []);

  // Get a wedged HLS player moving again (see stall-watchdog.ts): nudge, then start a fresh stream.
  useEffect(() => {
    if (!playback || playback.method === "direct_play") return;
    const v = videoRef.current;
    if (!v) return;
    const dog = new StallWatchdog();
    const timer = window.setInterval(() => {
      const action = dog.tick(
        {
          currentTime: v.currentTime,
          paused: v.paused,
          seeking: v.seeking,
          ended: v.ended,
          bufferedAhead: bufferedAhead(v.buffered, v.currentTime),
        },
        Date.now(),
      );
      if (action === "nudge") {
        statsRef.current?.note(`stuck with buffer at ${v.currentTime.toFixed(3)}, nudging`);
        v.currentTime += NUDGE_S;
      } else if (action === "recover") {
        // A fresh stream, not hls.recoverMediaError(): re-attaching the same
        // buffer made Chrome fail to decode open-GOP video in testing.
        recover("stuck with a full buffer", false);
      }
    }, 1000);
    return () => clearInterval(timer);
  }, [playback, recover]);

  // Keep the server's stream alive while the player is open, paused or not,
  // and pick up the transcoder's status for the info panel.
  useEffect(() => {
    const session = playback?.session_id;
    if (!session) {
      statsRef.current?.setSessionStatus(null);
      return;
    }
    let stopped = false;
    const ping = () =>
      api
        .post<SessionStatus>(`api/hls/${session}/keepalive`)
        .then((st) => !stopped && statsRef.current?.setSessionStatus(st))
        .catch((e: Error & { status?: number }) => {
          if (stopped) return;
          if (e.status === 404 && sessionRef.current === session) recover("session expired");
          else statsRef.current?.note(`keepalive failed: ${e.message}`);
        });
    ping();
    const timer = window.setInterval(ping, KEEPALIVE_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [playback?.session_id, recover]);

  // Tell the admin dashboard where we are, whether we're paused, and how playback is going.
  // Sent every few seconds and right away when the player pauses, plays or stalls.
  useEffect(() => {
    const playId = playback?.play_id;
    const v = videoRef.current;
    if (!playId || !v) return;
    let gone = false;
    const send = () => {
      if (gone || playRef.current !== playId) return;
      const state = v.paused ? "paused" : v.readyState < 3 || v.seeking ? "buffering" : "playing";
      api
        .post(`api/now-playing/${playId}/heartbeat`, {
          position_ms: Math.floor(v.currentTime * 1000),
          state,
          stats: statsRef.current?.report(),
        })
        .catch((e: Error & { status?: number }) => {
          // 404: the server forgot this playback (asleep too long). Only the dashboard misses it.
          if (e.status === 404) gone = true;
        });
    };
    const events = ["play", "pause", "waiting", "playing", "seeked"] as const;
    for (const e of events) v.addEventListener(e, send);
    const timer = window.setInterval(send, HEARTBEAT_MS);
    return () => {
      clearInterval(timer);
      for (const e of events) v.removeEventListener(e, send);
    };
  }, [playback?.play_id]);

  // Load exactly the chosen subtitle track (or none). Tracks are matched by the id we give each <track>.
  // "hidden" means the browser loads the cues but doesn't draw them: MediaPlayer draws them above
  // its controls. iPhone's native fullscreen hides our overlay, so there the browser draws them.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    let nativeFullscreen = false;
    const apply = () => {
      for (const t of Array.from(v.textTracks)) {
        t.mode = t.id === `sub-${subIndex}` ? (nativeFullscreen ? "showing" : "hidden") : "disabled";
      }
    };
    const begin = () => {
      nativeFullscreen = true;
      apply();
    };
    const end = () => {
      nativeFullscreen = false;
      apply();
    };
    apply();
    v.textTracks.addEventListener("addtrack", apply);
    v.addEventListener("webkitbeginfullscreen", begin);
    v.addEventListener("webkitendfullscreen", end);
    return () => {
      v.textTracks.removeEventListener("addtrack", apply);
      v.removeEventListener("webkitbeginfullscreen", begin);
      v.removeEventListener("webkitendfullscreen", end);
    };
  }, [playback, subIndex]);

  // Direct play can still fail (e.g. a codec the browser claimed but can't decode): fall back to transcoding.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const onError = () => {
      if (playback?.method === "direct_play" && !forceTranscode) {
        startAt.current = v.currentTime || startAt.current;
        setForceTranscode(true);
      } else if (v.error) {
        setError(i18n.t("player.playback_error_code", { code: v.error.code }));
      }
    };
    v.addEventListener("error", onError);
    return () => v.removeEventListener("error", onError);
  }, [playback, forceTranscode]);

  // Save progress every 10 s, on pause, and when leaving.
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (videoRef.current && !videoRef.current.paused) saveProgress();
    }, 10_000);
    const onHide = () => {
      saveProgress();
      // A closed tab never unmounts: take direct play off the dashboard now.
      // (HLS sessions are cleaned up by the server when the segments stop.)
      if (!sessionRef.current && playRef.current) beacon("DELETE", `api/now-playing/${playRef.current}`, {});
    };
    window.addEventListener("pagehide", onHide);
    return () => {
      clearInterval(timer);
      window.removeEventListener("pagehide", onHide);
      saveProgress();
      stopSession();
    };
  }, [saveProgress, stopSession]);

  const onEnded = () => {
    saveProgress();
    const next = item?.parent_id ? `/item/${item.parent_id}` : `/item/${id}`;
    navigate(next);
  };

  const title =
    item?.kind === "episode"
      ? `${item.series_title ?? ""} · ${episodeCode(item.season_number, item.episode_number)} ${item.title}`
      : (item?.title ?? "");

  const audio: PlayerChoice | undefined =
    file?.info && file.info.audio.length > 1
      ? {
          options: file.info.audio.map((a) => ({
            value: String(a.index),
            label: [a.language?.toUpperCase(), a.title, a.codec, `${a.channels}ch`].filter(Boolean).join(" · "),
          })),
          value: String(audioIndex ?? playback?.plan.audio_index ?? ""),
          onChange: (value) => {
            startAt.current = videoRef.current?.currentTime ?? 0;
            setAudioIndex(Number(value));
            rememberTracks(Number(value), subIndex);
          },
        }
      : undefined;

  const subtitles: PlayerChoice | undefined =
    playback && playback.subtitles.length > 0
      ? {
          options: [
            { value: "off", label: i18n.t("player.subtitles_off") },
            ...playback.subtitles.map((s) => ({
              value: String(s.index),
              label:
                ([
                  s.language?.toUpperCase(),
                  s.label !== s.language ? s.label : null,
                  s.is_forced ? i18n.t("player.subtitle_forced") : null,
                ]
                  .filter(Boolean)
                  .join(" · ") || i18n.t("player.track", { count: s.index })) +
                (s.supported ? "" : i18n.t("player.subtitle_unsupported")),
              disabled: !s.supported,
            })),
          ],
          value: subIndex === null ? "off" : String(subIndex),
          onChange: (value) => {
            const chosen = value === "off" ? null : Number(value);
            setSubIndex(chosen);
            const track = playback.subtitles.find((t) => t.index === chosen);
            saveSubtitlePreference(chosen === null ? "off" : (track?.language ?? "off"));
            rememberTracks(audioIndex ?? playback.plan.audio_index, chosen);
          },
        }
      : undefined;

  const subtitleLabel = subtitles?.options.find((o) => o.value === subtitles.value)?.label ?? null;
  const getInfo = () =>
    statsRef.current?.snapshot({
      playback,
      file,
      audioIndex,
      subtitleLabel: subIndex === null ? null : subtitleLabel,
    }) ?? null;

  return (
    <MediaPlayer
      videoRef={videoRef}
      title={title}
      durationHint={playback ? playback.duration_ms / 1000 : undefined}
      audio={audio}
      subtitles={subtitles}
      methodLabel={playback ? methodLabel(playback.method) : null}
      getInfo={getInfo}
      loading={!playback && !error}
      error={error}
      onBack={() => navigate(-1)}
      onPause={saveProgress}
      onEnded={onEnded}
    >
      {playback?.subtitles
        .filter((s) => s.supported)
        .map((s) => (
          <track
            key={s.index}
            id={`sub-${s.index}`}
            kind="subtitles"
            src={s.url}
            label={s.label}
            srcLang={s.language ?? undefined}
          />
        ))}
    </MediaPlayer>
  );
}

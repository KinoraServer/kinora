import Hls from "hls.js";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { MediaPlayer, type PlayerChoice } from "@/components/MediaPlayer";
import { api } from "@/lib/api";
import { browserCaps } from "@/lib/caps";
import { episodeCode } from "@/lib/format";
import { pickSubtitle, readSubtitlePreference, saveSubtitlePreference } from "@/lib/subtitles";
import { useApi } from "@/lib/session";
import type { ItemDetail, PlaybackStart } from "@/lib/types";

const METHOD_LABEL: Record<PlaybackStart["method"], string> = {
  direct_play: "Direct play",
  remux: "Direct stream",
  transcode: "Transcoding",
};

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

export function PlayerPage() {
  const { id } = useParams();
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const { data: item } = useApi<ItemDetail>(`api/items/${id}`, [id]);

  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const sessionRef = useRef<string | null>(null);
  const durationRef = useRef(0);

  const [playback, setPlayback] = useState<PlaybackStart | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [audioIndex, setAudioIndex] = useState<number | null>(null);
  const [forceTranscode, setForceTranscode] = useState(false);
  const [subIndex, setSubIndex] = useState<number | null>(null);
  const startAt = useRef(Number(params.get("t") ?? 0));

  const file = item?.files[0];

  const saveProgress = useCallback(() => {
    const v = videoRef.current;
    if (!v || !id || !durationRef.current) return;
    beacon("POST", `api/items/${id}/progress`, {
      position_ms: Math.floor(v.currentTime * 1000),
      duration_ms: durationRef.current,
    });
  }, [id]);

  const stopSession = useCallback(() => {
    if (sessionRef.current) {
      beacon("DELETE", `api/hls/${sessionRef.current}`, {
        item_id: id,
        position_ms: Math.floor((videoRef.current?.currentTime ?? 0) * 1000),
      });
      sessionRef.current = null;
    }
    hlsRef.current?.destroy();
    hlsRef.current = null;
  }, [id]);

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
      })
      .then((p) => {
        if (cancelled) return;
        stopSession();
        setPlayback(p);
        setSubIndex(pickSubtitle(p.subtitles, readSubtitlePreference()));
        durationRef.current = p.duration_ms;
        sessionRef.current = p.session_id;
        const start = startAt.current;

        if (p.method === "direct_play") {
          video.src = p.url;
          video.addEventListener("loadedmetadata", () => start > 0 && (video.currentTime = start), { once: true });
        } else if (Hls.isSupported()) {
          const hls = new Hls({ startPosition: start, maxBufferLength: 30, backBufferLength: 30 });
          hlsRef.current = hls;
          hls.loadSource(p.url);
          hls.attachMedia(video);
          hls.on(Hls.Events.ERROR, (_e, data) => {
            if (data.fatal) setError(`Playback error: ${data.details}`);
          });
        } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
          video.src = p.url; // Safari / iOS native HLS
          video.addEventListener("loadedmetadata", () => start > 0 && (video.currentTime = start), { once: true });
        } else {
          setError("This browser can't play HLS streams.");
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
  }, [file, audioIndex, forceTranscode, stopSession]);

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
        setError(`Playback error (${v.error.code})`);
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
          },
        }
      : undefined;

  const subtitles: PlayerChoice | undefined =
    playback && playback.subtitles.length > 0
      ? {
          options: [
            { value: "off", label: "Off" },
            ...playback.subtitles.map((t) => ({
              value: String(t.index),
              label:
                ([t.language?.toUpperCase(), t.label !== t.language ? t.label : null, t.is_forced ? "forced" : null]
                  .filter(Boolean)
                  .join(" · ") || `Track ${t.index}`) + (t.supported ? "" : " (image subtitles, not supported yet)"),
              disabled: !t.supported,
            })),
          ],
          value: subIndex === null ? "off" : String(subIndex),
          onChange: (value) => {
            const chosen = value === "off" ? null : Number(value);
            setSubIndex(chosen);
            const track = playback.subtitles.find((t) => t.index === chosen);
            saveSubtitlePreference(chosen === null ? "off" : (track?.language ?? "off"));
          },
        }
      : undefined;

  const info = playback && (
    <>
      {playback.method === "transcode" && <p className="text-white/70">Using {playback.hardware}</p>}
      {playback.plan.video?.kind === "encode" && (
        <p className="text-white/80">
          Video → H.264 {playback.plan.video.height}p at {(playback.plan.video.bitrate / 1_000_000).toFixed(1)} Mbps
          {playback.plan.video.tonemap && " (HDR tone mapped to SDR)"}
        </p>
      )}
      {playback.plan.audio?.kind === "encode" && (
        <p className="text-white/80">Audio → AAC {playback.plan.audio.channels}ch</p>
      )}
      {playback.plan.reasons.length > 0 && (
        <ul className="list-disc pl-4 text-white/70">
          {playback.plan.reasons.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
      )}
    </>
  );

  return (
    <MediaPlayer
      videoRef={videoRef}
      title={title}
      durationHint={playback ? playback.duration_ms / 1000 : undefined}
      audio={audio}
      subtitles={subtitles}
      methodLabel={playback ? METHOD_LABEL[playback.method] : null}
      infoContent={info}
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

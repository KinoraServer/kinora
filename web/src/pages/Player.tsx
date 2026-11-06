import Hls from "hls.js";
import { ArrowLeft, Info } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { Badge, Spinner } from "@/components/ui/card";
import { api } from "@/lib/api";
import { browserCaps } from "@/lib/caps";
import { episodeCode } from "@/lib/format";
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
  const [showInfo, setShowInfo] = useState(false);
  const [chromeVisible, setChromeVisible] = useState(true);
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

  // Auto-hide the top bar while playing.
  useEffect(() => {
    let t: number | undefined;
    const show = () => {
      setChromeVisible(true);
      clearTimeout(t);
      t = window.setTimeout(() => !videoRef.current?.paused && setChromeVisible(false), 3000);
    };
    window.addEventListener("mousemove", show);
    window.addEventListener("touchstart", show);
    show();
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousemove", show);
      window.removeEventListener("touchstart", show);
    };
  }, []);

  const onEnded = () => {
    saveProgress();
    const next = item?.parent_id ? `/item/${item.parent_id}` : `/item/${id}`;
    navigate(next);
  };

  const title =
    item?.kind === "episode"
      ? `${item.series_title ?? ""} · ${episodeCode(item.season_number, item.episode_number)} ${item.title}`
      : (item?.title ?? "");

  return (
    <div className="fixed inset-0 z-50 bg-black text-white">
      <video
        ref={videoRef}
        className="h-full w-full"
        controls
        playsInline
        onPause={saveProgress}
        onEnded={onEnded}
      >
        {playback?.subtitles.map((s) => (
          <track key={s.index} kind="subtitles" src={s.url} label={s.label} srcLang={s.language ?? undefined} default={s.is_default} />
        ))}
      </video>

      <div
        className={`pointer-events-none absolute inset-x-0 top-0 bg-gradient-to-b from-black/80 to-transparent p-4 transition-opacity ${chromeVisible ? "opacity-100" : "opacity-0"}`}
      >
        <div className="pointer-events-auto flex items-center gap-3">
          <button
            onClick={() => navigate(-1)}
            className="flex h-10 w-10 cursor-pointer items-center justify-center rounded-full bg-white/10 hover:bg-white/20"
            aria-label="Back"
          >
            <ArrowLeft className="h-5 w-5" />
          </button>
          <p className="min-w-0 flex-1 truncate font-medium">{title}</p>
          {file?.info && file.info.audio.length > 1 && (
            <select
              aria-label="Audio track"
              className="h-9 cursor-pointer rounded-md bg-white/10 px-2 text-sm"
              value={audioIndex ?? playback?.plan.audio_index ?? ""}
              onChange={(e) => {
                startAt.current = videoRef.current?.currentTime ?? 0;
                setAudioIndex(Number(e.target.value));
              }}
            >
              {file.info.audio.map((a) => (
                <option key={a.index} value={a.index} className="text-black">
                  {[a.language?.toUpperCase(), a.title, a.codec, `${a.channels}ch`].filter(Boolean).join(" · ")}
                </option>
              ))}
            </select>
          )}
          {playback && (
            <button
              onClick={() => setShowInfo((s) => !s)}
              className="flex cursor-pointer items-center gap-1.5 rounded-md bg-white/10 px-2 py-1.5 text-sm hover:bg-white/20"
            >
              <Info className="h-4 w-4" /> {METHOD_LABEL[playback.method]}
            </button>
          )}
        </div>
        {showInfo && playback && (
          <div className="pointer-events-auto mt-3 ml-auto max-w-sm space-y-2 rounded-lg bg-black/80 p-4 text-sm">
            <p>
              <Badge className="bg-white/15 text-white">{METHOD_LABEL[playback.method]}</Badge>{" "}
              {playback.method === "transcode" && <span className="text-white/70">using {playback.hardware}</span>}
            </p>
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
          </div>
        )}
      </div>

      {!playback && !error && (
        <div className="absolute inset-0 flex items-center justify-center">
          <Spinner className="h-10 w-10" />
        </div>
      )}
      {error && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-lg font-medium">Couldn't play this</p>
          <p className="text-white/70">{error}</p>
        </div>
      )}
    </div>
  );
}

// The video player. Everything lives in this one file on purpose:
//
//   1. usePlayerController  - ALL player behaviour (play, seek, volume, speed,
//                             fullscreen, shortcuts, auto-hide, ...). One place.
//   2. Shared controls      - small building blocks (buttons, seek bar, menus)
//                             that every layout is assembled from.
//   3. Layouts              - Minimal, Cinematic, Floating, Pro. They only
//                             arrange the shared controls, so every layout has
//                             exactly the same features.
//   4. Custom skins         - admin-installed markup + CSS, sanitised here and
//                             wired to the controller with data-kn-* attributes.
//   5. MediaPlayer          - picks the layout and renders it over the video.
//
// The page that owns the stream (pages/Player.tsx) only starts playback and
// hands us the <video> element's ref plus the audio/subtitle options.

import {
  ArrowLeft,
  AudioLines,
  Captions,
  Gauge,
  Info,
  Maximize,
  Minimize,
  Palette,
  Pause,
  PictureInPicture2,
  Play,
  RotateCcw,
  RotateCw,
  Volume2,
  VolumeX,
} from "lucide-react";
import {
  type CSSProperties,
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Spinner } from "@/components/ui/card";
import { formatClock } from "@/lib/format";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";
import type { PlayerSkin } from "@/lib/types";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface PlayerOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface PlayerChoice {
  options: PlayerOption[];
  value: string;
  onChange: (value: string) => void;
}

export interface MediaPlayerProps {
  videoRef: RefObject<HTMLVideoElement | null>;
  title: string;
  /** Duration from the server, used until the browser knows it. */
  durationHint?: number;
  audio?: PlayerChoice;
  subtitles?: PlayerChoice;
  /** "Direct play", "Transcoding", ... */
  methodLabel?: string | null;
  /** Extra details shown in the info panel (the page decides what to say). */
  infoContent?: ReactNode;
  loading?: boolean;
  error?: string | null;
  onBack: () => void;
  onPause?: () => void;
  onEnded?: () => void;
  /** <track> elements for the subtitles. */
  children?: ReactNode;
}

// ---------------------------------------------------------------------------
// 1. Controller: all behaviour, no markup
// ---------------------------------------------------------------------------

export const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];
const HIDE_AFTER_MS = 3000;

export interface PlayerState {
  playing: boolean;
  buffering: boolean;
  currentTime: number;
  duration: number;
  /** End of the buffered range that contains the playhead, in seconds. */
  bufferedEnd: number;
  volume: number;
  muted: boolean;
  rate: number;
  fullscreen: boolean;
  pip: boolean;
  controlsVisible: boolean;
  infoOpen: boolean;
  openMenu: string | null;
}

export interface PlayerActions {
  toggle: () => void;
  seek: (seconds: number) => void;
  skip: (delta: number) => void;
  setVolume: (volume: number) => void;
  toggleMute: () => void;
  setRate: (rate: number) => void;
  toggleFullscreen: () => void;
  togglePip: () => void;
  toggleInfo: () => void;
  setOpenMenu: (id: string | null) => void;
  setScrubbing: (scrubbing: boolean) => void;
  /** Show the controls and restart the auto-hide timer. */
  poke: () => void;
}

function bufferedEndAt(v: HTMLVideoElement): number {
  const t = v.currentTime;
  for (let i = 0; i < v.buffered.length; i++) {
    if (v.buffered.start(i) <= t + 0.5 && v.buffered.end(i) >= t) return v.buffered.end(i);
  }
  return t;
}

export const canPip = () => typeof document !== "undefined" && !!document.pictureInPictureEnabled;

function usePlayerController(
  videoRef: RefObject<HTMLVideoElement | null>,
  rootRef: RefObject<HTMLElement | null>,
  durationHint: number,
): { state: PlayerState; actions: PlayerActions } {
  const [snap, setSnap] = useState({
    playing: false,
    buffering: true,
    currentTime: 0,
    duration: durationHint,
    bufferedEnd: 0,
    volume: 1,
    muted: false,
    rate: 1,
    pip: false,
  });
  const [fullscreen, setFullscreen] = useState(false);
  const [active, setActive] = useState(true);
  const [scrubbing, setScrubbing] = useState(false);
  const [infoOpen, setInfoOpen] = useState(false);
  const [openMenu, setOpenMenu] = useState<string | null>(null);
  const hideTimer = useRef<number | undefined>(undefined);

  // Mirror the <video> element's state.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const read = () =>
      setSnap({
        playing: !v.paused && !v.ended,
        buffering: !v.paused && v.readyState < 3,
        currentTime: v.currentTime,
        duration: Number.isFinite(v.duration) && v.duration > 0 ? v.duration : durationHint,
        bufferedEnd: bufferedEndAt(v),
        volume: v.volume,
        muted: v.muted,
        rate: v.playbackRate,
        pip: document.pictureInPictureElement === v,
      });
    const events = [
      "play",
      "pause",
      "ended",
      "timeupdate",
      "durationchange",
      "progress",
      "waiting",
      "playing",
      "canplay",
      "seeking",
      "seeked",
      "volumechange",
      "ratechange",
      "loadedmetadata",
      "emptied",
      "enterpictureinpicture",
      "leavepictureinpicture",
    ];
    for (const e of events) v.addEventListener(e, read);
    read();
    return () => {
      for (const e of events) v.removeEventListener(e, read);
    };
  }, [videoRef, durationHint]);

  useEffect(() => {
    const onChange = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  const poke = useCallback(() => {
    setActive(true);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setActive(false), HIDE_AFTER_MS);
  }, []);

  useEffect(() => {
    const root = rootRef.current;
    poke();
    root?.addEventListener("pointermove", poke);
    root?.addEventListener("pointerdown", poke);
    return () => {
      window.clearTimeout(hideTimer.current);
      root?.removeEventListener("pointermove", poke);
      root?.removeEventListener("pointerdown", poke);
    };
  }, [rootRef, poke]);

  // Close a menu when the viewer clicks anywhere else.
  useEffect(() => {
    if (!openMenu) return;
    const close = (e: PointerEvent) => {
      if (!(e.target as HTMLElement | null)?.closest?.("[data-menu]")) setOpenMenu(null);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [openMenu]);

  const actions = useMemo<PlayerActions>(() => {
    const video = () => videoRef.current;
    const seek = (seconds: number) => {
      const v = video();
      if (!v) return;
      const max = Number.isFinite(v.duration) && v.duration > 0 ? v.duration : durationHint;
      v.currentTime = Math.min(Math.max(0, seconds), max || seconds);
    };
    return {
      toggle: () => {
        const v = video();
        if (!v) return;
        if (v.paused) v.play().catch(() => {});
        else v.pause();
      },
      seek,
      skip: (delta) => {
        const v = video();
        if (v) seek(v.currentTime + delta);
      },
      setVolume: (volume) => {
        const v = video();
        if (!v) return;
        v.volume = Math.min(1, Math.max(0, volume));
        v.muted = v.volume === 0;
      },
      toggleMute: () => {
        const v = video();
        if (v) v.muted = !v.muted;
      },
      setRate: (rate) => {
        const v = video();
        if (v) v.playbackRate = rate;
      },
      toggleFullscreen: () => {
        const v = video() as (HTMLVideoElement & { webkitEnterFullscreen?: () => void }) | null;
        if (document.fullscreenElement) {
          document.exitFullscreen().catch(() => {});
        } else if (rootRef.current?.requestFullscreen) {
          rootRef.current.requestFullscreen().catch(() => {});
        } else {
          v?.webkitEnterFullscreen?.(); // iPhone Safari only fullscreens the video itself
        }
      },
      togglePip: () => {
        const v = video();
        if (!v || !canPip()) return;
        if (document.pictureInPictureElement) document.exitPictureInPicture().catch(() => {});
        else v.requestPictureInPicture().catch(() => {});
      },
      toggleInfo: () => setInfoOpen((o) => !o),
      setOpenMenu,
      setScrubbing,
      poke,
    };
  }, [videoRef, rootRef, durationHint, poke]);

  // Keyboard shortcuts (ignored while typing in a field or using a menu).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const v = videoRef.current;
      if (!v) return;
      const handled = (fn: () => void) => {
        e.preventDefault();
        fn();
        poke();
      };
      switch (e.key) {
        case " ":
        case "k":
          return handled(actions.toggle);
        case "ArrowLeft":
          return handled(() => actions.skip(-5));
        case "ArrowRight":
          return handled(() => actions.skip(5));
        case "j":
          return handled(() => actions.skip(-10));
        case "l":
          return handled(() => actions.skip(10));
        case "ArrowUp":
          return handled(() => actions.setVolume(v.volume + 0.1));
        case "ArrowDown":
          return handled(() => actions.setVolume(v.volume - 0.1));
        case "m":
          return handled(actions.toggleMute);
        case "f":
          return handled(actions.toggleFullscreen);
        case "i":
          return handled(actions.toggleInfo);
        case "Escape":
          return setOpenMenu(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [videoRef, actions, poke]);

  const state: PlayerState = {
    ...snap,
    fullscreen,
    infoOpen,
    openMenu,
    controlsVisible: active || !snap.playing || scrubbing || openMenu !== null,
  };
  return { state, actions };
}

// ---------------------------------------------------------------------------
// Player styles (layouts)
// ---------------------------------------------------------------------------

export interface PlayerStyleInfo {
  id: string;
  name: string;
  description: string;
}

export const BUILTIN_STYLES: PlayerStyleInfo[] = [
  { id: "minimal", name: "Minimal", description: "A slim bar along the bottom." },
  { id: "cinematic", name: "Cinematic", description: "Big centred controls and large type." },
  { id: "floating", name: "Floating", description: "A rounded glass bar floating over the video." },
  { id: "pro", name: "Pro", description: "Dense controls with labels and extra readouts." },
];

export const DEFAULT_STYLE = "minimal";
/** The viewer's choice when they want whatever the theme says. */
export const THEME_STYLE = "theme";
const STYLE_KEY = "kinora.player-style";

/**
 * Which layout to show. A viewer's own pick wins; "theme" (the default) means
 * the theme's `player.layout`; anything unknown (a skin that was removed)
 * falls back to the default layout.
 */
export function resolvePlayerStyle(
  choice: string | null,
  themeLayout: string | null | undefined,
  skinIds: string[],
): string {
  const wanted = choice && choice !== THEME_STYLE ? choice : (themeLayout ?? DEFAULT_STYLE);
  const known = BUILTIN_STYLES.some((s) => s.id === wanted) || skinIds.includes(wanted);
  return known ? wanted : DEFAULT_STYLE;
}

function readStyleChoice(): string {
  try {
    return localStorage.getItem(STYLE_KEY) ?? THEME_STYLE;
  } catch {
    return THEME_STYLE;
  }
}

function usePlayerStyle(skins: PlayerSkin[]) {
  const { themes, current } = useTheme();
  const [choice, setChoice] = useState(readStyleChoice);
  const themeLayout = themes.find((t) => t.id === current)?.player?.layout;
  const resolved = resolvePlayerStyle(
    choice,
    themeLayout,
    skins.map((s) => s.id),
  );
  const options: PlayerOption[] = [
    {
      value: THEME_STYLE,
      label: `Theme default (${BUILTIN_STYLES.find((s) => s.id === themeLayout)?.name ?? skins.find((s) => s.id === themeLayout)?.name ?? "Minimal"})`,
    },
    ...BUILTIN_STYLES.map((s) => ({ value: s.id, label: s.name })),
    ...skins.map((s) => ({ value: s.id, label: `${s.name} (custom)` })),
  ];
  const select = useCallback((value: string) => {
    setChoice(value);
    try {
      localStorage.setItem(STYLE_KEY, value);
    } catch {
      // private mode: the pick just won't persist
    }
  }, []);
  // The menu shows the viewer's choice (which may be "theme"), not the resolved layout.
  const shownChoice = options.some((o) => o.value === choice) ? choice : THEME_STYLE;
  return { resolved, options, choice: shownChoice, select };
}

// ---------------------------------------------------------------------------
// 2. Shared controls: every layout is built from these
// ---------------------------------------------------------------------------

interface Ctx {
  s: PlayerState;
  a: PlayerActions;
  props: MediaPlayerProps;
  styleChoice: PlayerChoice;
}

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

const radiusVar: CSSProperties = { borderRadius: "var(--kn-player-button-radius)" };

/** Translucent card colour using the theme's controls-opacity knob. */
const glass = (extra?: CSSProperties): CSSProperties => ({
  background: "color-mix(in oklab, var(--kn-card) calc(var(--kn-player-controls-opacity) * 100%), transparent)",
  backdropFilter: "blur(var(--kn-player-bar-blur))",
  WebkitBackdropFilter: "blur(var(--kn-player-bar-blur))",
  ...extra,
});

function IconBtn({
  label,
  onClick,
  children,
  className,
  active,
  disabled,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  className?: string;
  active?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      style={radiusVar}
      className={cn(
        "pointer-events-auto flex shrink-0 cursor-pointer items-center justify-center gap-1.5 text-white transition-colors hover:bg-white/20 focus-visible:outline-2 focus-visible:outline-ring",
        active && "bg-white/20",
        className ?? "h-10 w-10",
      )}
    >
      {children}
    </button>
  );
}

function PlayPause({ c, className, iconClass = "h-5 w-5" }: { c: Ctx; className?: string; iconClass?: string }) {
  return (
    <IconBtn label={c.s.playing ? "Pause" : "Play"} onClick={c.a.toggle} className={className}>
      {c.s.playing ? (
        <Pause className={iconClass} fill="currentColor" />
      ) : (
        <Play className={iconClass} fill="currentColor" />
      )}
    </IconBtn>
  );
}

function SkipButtons({ c, className, iconClass = "h-5 w-5" }: { c: Ctx; className?: string; iconClass?: string }) {
  return (
    <>
      <IconBtn label="Back 10 seconds" onClick={() => c.a.skip(-10)} className={className}>
        <RotateCcw className={iconClass} />
      </IconBtn>
      <IconBtn label="Forward 10 seconds" onClick={() => c.a.skip(10)} className={className}>
        <RotateCw className={iconClass} />
      </IconBtn>
    </>
  );
}

function BackButton({ c, className }: { c: Ctx; className?: string }) {
  return (
    <IconBtn label="Back" onClick={c.props.onBack} className={className}>
      <ArrowLeft className="h-5 w-5" />
    </IconBtn>
  );
}

function TimeLabel({ c, className }: { c: Ctx; className?: string }) {
  return (
    <span className={cn("shrink-0 text-sm tabular-nums text-white/90", className)}>
      {formatClock(c.s.currentTime)} / {formatClock(c.s.duration)}
    </span>
  );
}

function Seek({ c, className }: { c: Ctx; className?: string }) {
  const { s, a } = c;
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const [scrub, setScrub] = useState<number | null>(null);
  const duration = s.duration;
  const fraction = (clientX: number) => {
    const r = ref.current?.getBoundingClientRect();
    return r && r.width > 0 ? clamp((clientX - r.left) / r.width, 0, 1) : 0;
  };
  const shown = scrub ?? (duration > 0 ? clamp(s.currentTime / duration, 0, 1) : 0);
  const buffered = duration > 0 ? clamp(s.bufferedEnd / duration, 0, 1) : 0;
  const finish = () => {
    if (scrub !== null) {
      a.seek(scrub * duration);
      setScrub(null);
      a.setScrubbing(false);
    }
  };
  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={0}
      aria-label="Seek"
      aria-valuemin={0}
      aria-valuemax={Math.floor(duration)}
      aria-valuenow={Math.floor(s.currentTime)}
      aria-valuetext={`${formatClock(s.currentTime)} of ${formatClock(duration)}`}
      className={cn(
        "group/seek pointer-events-auto relative flex h-5 w-full cursor-pointer touch-none items-center",
        className,
      )}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        a.setScrubbing(true);
        setScrub(fraction(e.clientX));
      }}
      onPointerMove={(e) => {
        const f = fraction(e.clientX);
        setHover(f);
        if (scrub !== null) setScrub(f);
      }}
      onPointerUp={finish}
      onPointerCancel={finish}
      onPointerLeave={() => setHover(null)}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft") a.skip(-5);
        else if (e.key === "ArrowRight") a.skip(5);
        else return;
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div
        className="relative w-full overflow-visible rounded-full bg-white/25 transition-[height]"
        style={{ height: "var(--kn-player-seek-height)" }}
      >
        <div className="absolute inset-y-0 left-0 rounded-full bg-white/30" style={{ width: `${buffered * 100}%` }} />
        <div className="absolute inset-y-0 left-0 rounded-full bg-primary" style={{ width: `${shown * 100}%` }} />
        <div
          className="absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 scale-0 rounded-full bg-primary shadow transition-transform group-hover/seek:scale-100 group-focus-visible/seek:scale-100"
          style={{
            left: `${shown * 100}%`,
            transform: scrub !== null ? "translate(-50%, -50%) scale(1.2)" : undefined,
          }}
        />
      </div>
      {hover !== null && duration > 0 && (
        <div
          className="pointer-events-none absolute bottom-full mb-2 -translate-x-1/2 rounded-md bg-black/85 px-2 py-1 text-xs tabular-nums text-white"
          style={{ left: `${hover * 100}%` }}
        >
          {formatClock(hover * duration)}
        </div>
      )}
    </div>
  );
}

function Volume({ c, slider = true }: { c: Ctx; slider?: boolean }) {
  const { s, a } = c;
  const ref = useRef<HTMLDivElement>(null);
  const set = (clientX: number) => {
    const r = ref.current?.getBoundingClientRect();
    if (r && r.width > 0) a.setVolume((clientX - r.left) / r.width);
  };
  const level = s.muted ? 0 : s.volume;
  return (
    <div className="pointer-events-auto flex items-center">
      <IconBtn label={s.muted ? "Unmute" : "Mute"} onClick={a.toggleMute}>
        {s.muted || s.volume === 0 ? <VolumeX className="h-5 w-5" /> : <Volume2 className="h-5 w-5" />}
      </IconBtn>
      {slider && (
        <div
          ref={ref}
          role="slider"
          tabIndex={0}
          aria-label="Volume"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(level * 100)}
          className="relative hidden h-5 w-20 cursor-pointer touch-none items-center sm:flex"
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            set(e.clientX);
          }}
          onPointerMove={(e) => e.buttons === 1 && set(e.clientX)}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft") a.setVolume(s.volume - 0.1);
            else if (e.key === "ArrowRight") a.setVolume(s.volume + 0.1);
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <div className="relative h-1 w-full rounded-full bg-white/25">
            <div className="absolute inset-y-0 left-0 rounded-full bg-white" style={{ width: `${level * 100}%` }} />
          </div>
        </div>
      )}
    </div>
  );
}

/** A button that opens a small list above it. */
function MenuButton({
  c,
  id,
  label,
  icon,
  title,
  choice,
  text,
}: {
  c: Ctx;
  id: string;
  label: string;
  icon: ReactNode;
  title: string;
  choice: PlayerChoice;
  /** Optional visible text next to the icon (used by the Pro layout). */
  text?: string;
}) {
  const open = c.s.openMenu === id;
  return (
    <div data-menu className="pointer-events-auto relative">
      <IconBtn
        label={label}
        active={open}
        onClick={() => c.a.setOpenMenu(open ? null : id)}
        className={text ? "h-9 px-2.5 text-sm" : "h-10 w-10"}
      >
        {icon}
        {text && <span className="max-w-24 truncate">{text}</span>}
      </IconBtn>
      {open && (
        <div
          role="menu"
          className="absolute right-0 bottom-full z-10 mb-2 max-h-72 min-w-44 overflow-y-auto rounded-lg border border-border bg-card p-1 text-sm text-card-foreground shadow-xl"
        >
          <p className="px-3 py-1.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">{title}</p>
          {choice.options.map((o) => (
            <button
              key={o.value}
              type="button"
              role="menuitemradio"
              aria-checked={o.value === choice.value}
              disabled={o.disabled}
              onClick={() => {
                choice.onChange(o.value);
                c.a.setOpenMenu(null);
              }}
              className={cn(
                "flex w-full cursor-pointer items-center gap-2 rounded-md px-3 py-1.5 text-left hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50",
                o.value === choice.value && "font-medium text-primary",
              )}
            >
              <span className="w-3">{o.value === choice.value ? "✓" : ""}</span>
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function speedChoice(c: Ctx): PlayerChoice {
  return {
    options: SPEEDS.map((r) => ({ value: String(r), label: r === 1 ? "Normal" : `${r}×` })),
    value: String(c.s.rate),
    onChange: (v) => c.a.setRate(Number(v)),
  };
}

/** Audio, subtitles, speed, style, info, picture-in-picture, fullscreen: the same set in every layout. */
function RightControls({ c, labelled }: { c: Ctx; labelled?: boolean }) {
  const { props, s, a } = c;
  const audioLabel = props.audio?.options.find((o) => o.value === props.audio?.value)?.label;
  const subLabel = props.subtitles?.options.find((o) => o.value === props.subtitles?.value)?.label;
  return (
    <>
      {props.audio && props.audio.options.length > 1 && (
        <MenuButton
          c={c}
          id="audio"
          label="Audio track"
          title="Audio"
          icon={<AudioLines className="h-5 w-5" />}
          choice={props.audio}
          text={labelled ? (audioLabel?.split(" · ")[0] ?? "Audio") : undefined}
        />
      )}
      {props.subtitles && props.subtitles.options.length > 1 && (
        <MenuButton
          c={c}
          id="subtitles"
          label="Subtitles"
          title="Subtitles"
          icon={<Captions className="h-5 w-5" />}
          choice={props.subtitles}
          text={labelled ? (subLabel?.split(" · ")[0] ?? "Subtitles") : undefined}
        />
      )}
      <MenuButton
        c={c}
        id="speed"
        label="Playback speed"
        title="Speed"
        icon={<Gauge className="h-5 w-5" />}
        choice={speedChoice(c)}
        text={labelled ? `${s.rate}×` : undefined}
      />
      <MenuButton
        c={c}
        id="style"
        label="Player style"
        title="Player style"
        icon={<Palette className="h-5 w-5" />}
        choice={c.styleChoice}
      />
      <IconBtn label="Playback info" onClick={a.toggleInfo} active={s.infoOpen}>
        <Info className="h-5 w-5" />
      </IconBtn>
      {canPip() && (
        <IconBtn label="Picture in picture" onClick={a.togglePip} active={s.pip}>
          <PictureInPicture2 className="h-5 w-5" />
        </IconBtn>
      )}
      <IconBtn label={s.fullscreen ? "Exit fullscreen" : "Fullscreen"} onClick={a.toggleFullscreen}>
        {s.fullscreen ? <Minimize className="h-5 w-5" /> : <Maximize className="h-5 w-5" />}
      </IconBtn>
    </>
  );
}

/** Playback details, identical in every layout. */
function InfoPanel({ c }: { c: Ctx }) {
  const v = c.props.videoRef.current;
  const dropped = v && "getVideoPlaybackQuality" in v ? v.getVideoPlaybackQuality().droppedVideoFrames : 0;
  return (
    <div
      className="pointer-events-auto absolute top-20 right-4 z-10 w-72 max-w-[calc(100%-2rem)] space-y-2 rounded-lg bg-black/80 p-4 text-sm text-white"
      style={{ backdropFilter: "blur(var(--kn-player-bar-blur))" }}
    >
      {c.props.methodLabel && <p className="font-medium">{c.props.methodLabel}</p>}
      {c.props.infoContent}
      <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-white/70">
        <dt>Resolution</dt>
        <dd className="text-white">{v?.videoWidth ? `${v.videoWidth}×${v.videoHeight}` : "–"}</dd>
        <dt>Buffered ahead</dt>
        <dd className="text-white">{Math.max(0, Math.round(c.s.bufferedEnd - c.s.currentTime))} s</dd>
        <dt>Dropped frames</dt>
        <dd className="text-white">{dropped}</dd>
      </dl>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 3. Layouts
// ---------------------------------------------------------------------------

const topGradient = "bg-gradient-to-b from-black/80 to-transparent";
const bottomGradient = "bg-gradient-to-t from-black/85 via-black/40 to-transparent";

function MinimalLayout({ c }: { c: Ctx }) {
  return (
    <>
      <div className={cn("absolute inset-x-0 top-0 flex items-center gap-3 p-4", topGradient)}>
        <BackButton c={c} className="h-10 w-10 bg-white/10" />
        <p className="min-w-0 flex-1 truncate font-medium text-white">{c.props.title}</p>
      </div>
      <div className={cn("absolute inset-x-0 bottom-0 px-4 pt-16 pb-3", bottomGradient)}>
        <Seek c={c} />
        <div className="mt-1 flex items-center gap-1">
          <PlayPause c={c} />
          <SkipButtons c={c} />
          <Volume c={c} />
          <TimeLabel c={c} className="ml-2" />
          <span className="flex-1" />
          <RightControls c={c} />
        </div>
      </div>
    </>
  );
}

function CinematicLayout({ c }: { c: Ctx }) {
  return (
    <>
      <div className={cn("absolute inset-x-0 top-0 flex items-start gap-4 p-6 sm:p-8", topGradient)}>
        <BackButton c={c} className="h-12 w-12 bg-white/10" />
        <p className="min-w-0 flex-1 truncate text-2xl font-semibold tracking-tight text-white sm:text-3xl">
          {c.props.title}
        </p>
      </div>
      <div className="absolute inset-0 flex items-center justify-center gap-6 sm:gap-12">
        <IconBtn label="Back 10 seconds" onClick={() => c.a.skip(-10)} className="h-14 w-14 bg-black/30">
          <RotateCcw className="h-7 w-7" />
        </IconBtn>
        <button
          type="button"
          aria-label={c.s.playing ? "Pause" : "Play"}
          onClick={c.a.toggle}
          className="pointer-events-auto flex h-20 w-20 cursor-pointer items-center justify-center rounded-full bg-primary text-primary-foreground shadow-2xl transition-transform hover:scale-105 sm:h-24 sm:w-24"
        >
          {c.s.playing ? (
            <Pause className="h-9 w-9" fill="currentColor" />
          ) : (
            <Play className="ml-1 h-9 w-9" fill="currentColor" />
          )}
        </button>
        <IconBtn label="Forward 10 seconds" onClick={() => c.a.skip(10)} className="h-14 w-14 bg-black/30">
          <RotateCw className="h-7 w-7" />
        </IconBtn>
      </div>
      <div className={cn("absolute inset-x-0 bottom-0 px-6 pt-24 pb-6 sm:px-8", bottomGradient)}>
        <div className="flex items-center gap-4">
          <span className="w-14 text-right text-sm tabular-nums text-white/90">{formatClock(c.s.currentTime)}</span>
          <Seek c={c} />
          <span className="w-14 text-sm tabular-nums text-white/90">{formatClock(c.s.duration)}</span>
        </div>
        <div className="mt-2 flex items-center justify-end gap-1">
          <Volume c={c} />
          <span className="flex-1" />
          <RightControls c={c} />
        </div>
      </div>
    </>
  );
}

function FloatingLayout({ c }: { c: Ctx }) {
  return (
    <>
      <div
        className="absolute top-4 left-4 flex max-w-[calc(100%-2rem)] items-center gap-2 p-1.5 pr-5"
        style={glass({ borderRadius: "var(--kn-player-button-radius)" })}
      >
        <BackButton c={c} className="h-9 w-9" />
        <p className="min-w-0 truncate text-sm font-medium text-white">{c.props.title}</p>
      </div>
      <div
        className="absolute inset-x-3 bottom-4 mx-auto flex max-w-4xl flex-col gap-1 rounded-3xl px-4 py-2.5 sm:inset-x-6"
        style={glass()}
      >
        <Seek c={c} />
        <div className="flex items-center gap-1">
          <PlayPause c={c} className="h-10 w-10 bg-primary text-primary-foreground hover:bg-primary/80" />
          <SkipButtons c={c} />
          <Volume c={c} />
          <TimeLabel c={c} className="ml-2" />
          <span className="flex-1" />
          <RightControls c={c} />
        </div>
      </div>
    </>
  );
}

function ProLayout({ c }: { c: Ctx }) {
  const remaining = Math.max(0, c.s.duration - c.s.currentTime) / c.s.rate;
  const endsAt = new Date(Date.now() + remaining * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return (
    <>
      <div
        className="absolute inset-x-0 top-0 flex items-center gap-3 border-b border-border px-3 py-2"
        style={glass({ borderRadius: 0 })}
      >
        <BackButton c={c} className="h-9 w-9" />
        <p className="min-w-0 flex-1 truncate text-sm font-medium text-white">{c.props.title}</p>
        {c.props.methodLabel && (
          <span className="rounded bg-primary/20 px-2 py-0.5 text-xs font-medium text-primary">
            {c.props.methodLabel}
          </span>
        )}
      </div>
      <div
        className="absolute inset-x-0 bottom-0 border-t border-border px-3 pt-1 pb-2"
        style={glass({ borderRadius: 0 })}
      >
        <Seek c={c} />
        <div className="flex items-center gap-1">
          <PlayPause c={c} className="h-9 w-9" />
          <SkipButtons c={c} className="h-9 w-9" />
          <Volume c={c} />
          <TimeLabel c={c} className="ml-2 font-mono" />
          <span className="ml-3 hidden text-xs text-white/60 md:inline">
            -{formatClock(remaining)} · ends {endsAt}
          </span>
          <span className="flex-1" />
          <RightControls c={c} labelled />
        </div>
      </div>
    </>
  );
}

const LAYOUTS: Record<string, (props: { c: Ctx }) => ReactNode> = {
  minimal: MinimalLayout,
  cinematic: CinematicLayout,
  floating: FloatingLayout,
  pro: ProLayout,
};

// ---------------------------------------------------------------------------
// 4. Custom skins: sanitising, scoping, and wiring markup to the controller
// ---------------------------------------------------------------------------
//
// A skin is HTML + CSS written by whoever runs the server. We still treat it
// as untrusted: no scripts, no event handler attributes, no external loads
// (images/fonts/stylesheets could leak who is watching what). What's left can
// restyle and rearrange the player freely, but can't run code.
// docs/player-skins.md lists the data-kn-* attributes a skin can use.

const FORBIDDEN_TAGS = new Set([
  "script", "iframe", "frame", "frameset", "object", "embed", "link", "style", "meta", "base", "form",
  "input", "textarea", "video", "audio", "source", "track", "canvas", "foreignobject", "template", "slot",
]); // prettier-ignore
const URL_ATTRS = new Set([
  "src",
  "href",
  "xlink:href",
  "srcset",
  "action",
  "formaction",
  "poster",
  "data",
  "background",
]);

const isSafeUrl = (value: string) => /^\s*(#|data:image\/(png|jpe?g|gif|webp|svg\+xml|avif);)/i.test(value);
const hasUrlCall = (value: string) => /url\s*\(/i.test(value);

/** Strip anything that could run code or load something from elsewhere. */
export function sanitizeSkinHtml(html: string): string {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const walk = (el: Element) => {
    for (const child of Array.from(el.children)) {
      if (FORBIDDEN_TAGS.has(child.tagName.toLowerCase())) {
        child.remove();
        continue;
      }
      for (const attr of Array.from(child.attributes)) {
        const name = attr.name.toLowerCase();
        if (name.startsWith("on")) child.removeAttribute(attr.name);
        else if (URL_ATTRS.has(name) && !isSafeUrl(attr.value)) child.removeAttribute(attr.name);
        else if (name === "style" && (hasUrlCall(attr.value) || /expression|@import|behavior/i.test(attr.value)))
          child.removeAttribute(attr.name);
      }
      walk(child);
    }
  };
  walk(doc.body);
  return doc.body.innerHTML;
}

/** Remove `@keyframes` blocks (they can't be nested) and return them separately. */
function extractKeyframes(css: string): { rest: string; keyframes: string } {
  let rest = "";
  let keyframes = "";
  let i = 0;
  const re = /@(?:-webkit-)?keyframes\b/g;
  for (;;) {
    re.lastIndex = i;
    const m = re.exec(css);
    if (!m) {
      rest += css.slice(i);
      break;
    }
    rest += css.slice(i, m.index);
    const open = css.indexOf("{", m.index);
    if (open === -1) break;
    let depth = 0;
    let j = open;
    for (; j < css.length; j++) {
      if (css[j] === "{") depth++;
      else if (css[j] === "}" && --depth === 0) break;
    }
    keyframes += css.slice(m.index, j + 1);
    i = j + 1;
  }
  return { rest, keyframes };
}

/**
 * Make a skin's CSS safe and local: only `url(data:image/...)` is allowed,
 * and everything is nested under `[data-skin="id"]` so it can't touch the
 * rest of the app. Declarations written at the top level style the skin's root.
 */
export function scopeSkinCss(skinId: string, css: string): string {
  const cleaned = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/@import[^;]*;?/gi, "")
    .replace(/@font-face\s*\{[^}]*\}/gi, "")
    .replace(/(expression\s*\(|-moz-binding|behavior\s*:)[^;}]*/gi, "")
    .replace(/<\/?[a-z][^>]*>/gi, "")
    .replace(/url\(\s*(['"]?)(?!data:image\/)[^)]*\)/gi, "none");
  const { rest, keyframes } = extractKeyframes(cleaned);
  const scope = `[data-skin="${skinId.replace(/[^a-z0-9-]/gi, "")}"]`;
  return `${keyframes}\n${scope}{${rest}}`;
}

/** Elements in a skin that must receive clicks even though the skin layer lets the video through. */
const SKIN_BASE_CSS = `
[data-skin]{position:absolute;inset:0;pointer-events:none;}
[data-skin] [data-kn-action],[data-skin] [data-kn-seek],[data-skin] [data-kn-volume],
[data-skin] [data-kn-select],[data-skin] [data-kn-interactive]{pointer-events:auto;cursor:pointer;touch-action:none;}
[data-skin] [data-kn-select]{touch-action:auto;}
[data-skin] [data-kn-seek] > *,[data-skin] [data-kn-volume] > *{pointer-events:none;}
`;

function fillSelect(sel: HTMLSelectElement, choice: PlayerChoice | undefined) {
  if (!choice) {
    sel.hidden = true;
    return;
  }
  sel.hidden = choice.options.length < 2 && sel.dataset.knSelect !== "speed";
  const signature = choice.options.map((o) => `${o.value}|${o.label}|${o.disabled ? 1 : 0}`).join("\n");
  if (sel.dataset.signature !== signature) {
    sel.replaceChildren(
      ...choice.options.map((o) => {
        const opt = document.createElement("option");
        opt.value = o.value;
        opt.textContent = o.label; // textContent: labels come from file metadata
        opt.disabled = !!o.disabled;
        return opt;
      }),
    );
    sel.dataset.signature = signature;
  }
  sel.value = choice.value;
}

function SkinLayout({ c, skin }: { c: Ctx; skin: PlayerSkin }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const html = useMemo(() => sanitizeSkinHtml(skin.html), [skin.html]);
  const css = useMemo(() => scopeSkinCss(skin.id, skin.css), [skin.id, skin.css]);
  const latest = useRef(c);
  latest.current = c;

  // Clicks, dragging and selects, via delegation so the markup stays plain HTML.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const fractionIn = (el: Element, clientX: number) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 ? clamp((clientX - r.left) / r.width, 0, 1) : 0;
    };
    let dragging: { el: HTMLElement; kind: "seek" | "volume" } | null = null;
    const apply = (clientX: number, final: boolean) => {
      if (!dragging) return;
      const { a, s } = latest.current;
      const f = fractionIn(dragging.el, clientX);
      if (dragging.kind === "volume") a.setVolume(f);
      else if (final) a.seek(f * s.duration);
    };
    const onClick = (e: MouseEvent) => {
      const el = (e.target as HTMLElement).closest<HTMLElement>("[data-kn-action]");
      if (!el) return;
      const { a, props } = latest.current;
      const actions: Record<string, () => void> = {
        play: a.toggle,
        "skip-back": () => a.skip(-10),
        "skip-forward": () => a.skip(10),
        mute: a.toggleMute,
        fullscreen: a.toggleFullscreen,
        pip: a.togglePip,
        info: a.toggleInfo,
        back: props.onBack,
      };
      actions[el.dataset.knAction ?? ""]?.();
    };
    const onDown = (e: PointerEvent) => {
      const el = (e.target as HTMLElement).closest<HTMLElement>("[data-kn-seek],[data-kn-volume]");
      if (!el) return;
      dragging = { el, kind: "knSeek" in el.dataset ? "seek" : "volume" };
      el.setPointerCapture(e.pointerId);
      if (dragging.kind === "seek") latest.current.a.setScrubbing(true);
      apply(e.clientX, false);
    };
    const onMove = (e: PointerEvent) => dragging && apply(e.clientX, false);
    const onUp = (e: PointerEvent) => {
      if (!dragging) return;
      apply(e.clientX, true);
      if (dragging.kind === "seek") latest.current.a.setScrubbing(false);
      dragging = null;
    };
    const onChange = (e: Event) => {
      const sel = (e.target as HTMLElement).closest<HTMLSelectElement>("select[data-kn-select]");
      if (!sel) return;
      const { props, styleChoice } = latest.current;
      const speed: PlayerChoice = speedChoice(latest.current);
      const map: Record<string, PlayerChoice | undefined> = {
        audio: props.audio,
        subtitles: props.subtitles,
        speed,
        style: styleChoice,
      };
      map[sel.dataset.knSelect ?? ""]?.onChange(sel.value);
    };
    root.addEventListener("click", onClick);
    root.addEventListener("pointerdown", onDown);
    root.addEventListener("pointermove", onMove);
    root.addEventListener("pointerup", onUp);
    root.addEventListener("pointercancel", onUp);
    root.addEventListener("change", onChange);
    return () => {
      root.removeEventListener("click", onClick);
      root.removeEventListener("pointerdown", onDown);
      root.removeEventListener("pointermove", onMove);
      root.removeEventListener("pointerup", onUp);
      root.removeEventListener("pointercancel", onUp);
      root.removeEventListener("change", onChange);
    };
  }, []);

  // Push player state into the markup after every change.
  const { s, props } = c;
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const v = props.videoRef.current;
    const duration = s.duration;
    const remaining = Math.max(0, duration - s.currentTime);
    const texts: Record<string, string> = {
      title: props.title,
      time: formatClock(s.currentTime),
      duration: formatClock(duration),
      remaining: formatClock(remaining),
      method: props.methodLabel ?? "",
      resolution: v?.videoWidth ? `${v.videoWidth}×${v.videoHeight}` : "–",
      buffer: `${Math.max(0, Math.round(s.bufferedEnd - s.currentTime))}s`,
    };
    for (const el of root.querySelectorAll<HTMLElement>("[data-kn-text]")) {
      el.textContent = texts[el.dataset.knText ?? ""] ?? "";
    }
    const percent = (n: number) => `${clamp(n, 0, 1) * 100}%`;
    for (const bar of root.querySelectorAll<HTMLElement>("[data-kn-seek]")) {
      const played = duration > 0 ? s.currentTime / duration : 0;
      const buffered = duration > 0 ? s.bufferedEnd / duration : 0;
      const fill = bar.querySelector<HTMLElement>("[data-kn-fill]");
      const buf = bar.querySelector<HTMLElement>("[data-kn-buffer]");
      const thumb = bar.querySelector<HTMLElement>("[data-kn-thumb]");
      if (fill) fill.style.width = percent(played);
      if (buf) buf.style.width = percent(buffered);
      if (thumb) thumb.style.left = percent(played);
    }
    for (const bar of root.querySelectorAll<HTMLElement>("[data-kn-volume]")) {
      const fill = bar.querySelector<HTMLElement>("[data-kn-fill]");
      if (fill) fill.style.width = percent(s.muted ? 0 : s.volume);
    }
    for (const sel of root.querySelectorAll<HTMLSelectElement>("select[data-kn-select]")) {
      const kind = sel.dataset.knSelect;
      fillSelect(
        sel,
        kind === "audio"
          ? props.audio
          : kind === "subtitles"
            ? props.subtitles
            : kind === "style"
              ? c.styleChoice
              : kind === "speed"
                ? speedChoice(c)
                : undefined,
      );
    }
  });

  return (
    <>
      <style>{SKIN_BASE_CSS + css}</style>
      <div
        ref={rootRef}
        data-skin={skin.id}
        data-playing={s.playing}
        data-buffering={s.buffering}
        data-muted={s.muted}
        data-fullscreen={s.fullscreen}
        data-controls={s.controlsVisible ? "visible" : "hidden"}
        data-info={s.infoOpen}
        // Sanitised above: no scripts, handlers or external URLs survive.
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </>
  );
}

// ---------------------------------------------------------------------------
// 5. The component
// ---------------------------------------------------------------------------

export function MediaPlayer(props: MediaPlayerProps) {
  const { videoRef, children, loading, error } = props;
  const rootRef = useRef<HTMLDivElement>(null);
  const { state: s, actions: a } = usePlayerController(videoRef, rootRef, props.durationHint ?? 0);
  const { data: skinData } = useApi<PlayerSkin[]>("api/player-skins");
  const skins = useMemo(() => skinData ?? [], [skinData]);
  const style = usePlayerStyle(skins);

  const c: Ctx = {
    s,
    a,
    props,
    styleChoice: { options: style.options, value: style.choice, onChange: style.select },
  };
  const skin = skins.find((k) => k.id === style.resolved);
  const Layout = LAYOUTS[style.resolved] ?? MinimalLayout;
  const hidden = !s.controlsVisible;

  return (
    <div
      ref={rootRef}
      data-player-style={style.resolved}
      className={cn("fixed inset-0 z-50 overflow-hidden bg-black text-white", hidden && s.playing && "cursor-none")}
    >
      <video
        ref={videoRef}
        className="h-full w-full"
        playsInline
        onClick={() => (s.controlsVisible ? a.toggle() : a.poke())}
        onDoubleClick={a.toggleFullscreen}
        onPause={props.onPause}
        onEnded={props.onEnded}
      >
        {children}
      </video>

      {skin ? (
        <SkinLayout c={c} skin={skin} />
      ) : (
        <div
          // `inert` keeps hidden controls from catching clicks or focus.
          inert={hidden}
          className={cn(
            "pointer-events-none absolute inset-0 transition-opacity duration-300",
            hidden ? "opacity-0" : "opacity-100",
          )}
        >
          <Layout c={c} />
        </div>
      )}

      {s.infoOpen && !skin && <InfoPanel c={c} />}

      {(loading || (s.buffering && !error)) && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <Spinner className="h-10 w-10" />
        </div>
      )}
      {error && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/70 p-6 text-center">
          <p className="text-lg font-medium">Couldn't play this</p>
          <p className="text-white/70">{error}</p>
          <button
            type="button"
            onClick={props.onBack}
            className="mt-2 cursor-pointer rounded-md bg-white/10 px-4 py-2 text-sm hover:bg-white/20"
          >
            Go back
          </button>
        </div>
      )}
    </div>
  );
}

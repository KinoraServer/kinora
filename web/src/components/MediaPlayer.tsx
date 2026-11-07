// The video player. Everything lives in this one file on purpose:
//
//   1. usePlayerController  - ALL player behaviour (play, seek, volume, speed,
//                             fullscreen, shortcuts, auto-hide, ...). One place.
//   2. Shared controls      - small building blocks (buttons, seek bar, menus)
//                             that every layout is assembled from.
//   3. Layouts              - Minimal, Cinematic, Floating, Pro. They only
//                             arrange the shared controls, so every layout has
//                             exactly the same features.
//      Subtitles     - drawn by us (not the browser) so they can sit above
//                             the controls and be moved up or down from a small panel.
//   4. Custom skins         - admin-installed markup + CSS, sanitised here and
//                             wired to the controller with data-kn-* attributes.
//   5. MediaPlayer          - picks the layout and renders it over the video.
//
// The page that owns the stream (pages/Player.tsx) only starts playback and
// hands us the <video> element's ref plus the audio/subtitle options.

import {
  Activity,
  AppWindow,
  ArrowLeft,
  AudioLines,
  Captions,
  ChevronDown,
  ChevronUp,
  Check,
  Copy,
  Film,
  Gauge,
  HardDrive,
  Info,
  ListTree,
  Maximize,
  Minimize,
  MoveVertical,
  Network,
  Palette,
  Pause,
  PictureInPicture2,
  Play,
  RotateCcw,
  RotateCw,
  Server,
  Volume2,
  VolumeX,
  X,
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
import { useHotkeys } from "@tanstack/react-hotkeys";
import {
  describeSubtitleOffset,
  NO_OFFSET,
  nudgeSubtitleOffset,
  readSubtitleOffset,
  SUBTITLE_STEP,
  type SubtitleOffset,
  saveSubtitleOffset,
} from "@/lib/subtitles";
import { Spinner } from "@/components/ui/card";
import { formatClock } from "@/lib/format";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";
import { infoToText, type InfoIcon, type InfoSection, type PlayerInfo } from "@/lib/playback-stats";
import { type PlayerStyleSetting, resolvePlayerStyle } from "@/lib/player-styles";
import { useSkin } from "@/skin-engine/SkinProvider";
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
  /**
   * Live stats for the info panel, called about once a second while it's open
   * (see lib/playback-stats.ts). Without it the panel shows the basics it can
   * read from the <video> element.
   */
  getInfo?: () => PlayerInfo | null;
  loading?: boolean;
  error?: string | null;
  onBack: () => void;
  onPause?: () => void;
  onEnded?: () => void;
  /**
   * <track> elements for the subtitles. Set the chosen track's mode to
   * "hidden" (not "showing"): the player draws its cues itself.
   */
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
  /** How far the viewer moved the subtitles up or down (see lib/subtitles.ts). */
  subtitleOffset: SubtitleOffset;
}

/** `openMenu` id of the subtitle position panel. */
export const SUBTITLE_PANEL = "subtitle-position";

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
  /** Move the subtitles up (+) or down (-) by whole percent steps. Saved on this device. */
  nudgeSubtitles: (dy: number) => void;
  resetSubtitles: () => void;
  toggleSubtitlePanel: () => void;
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
  const [subtitleOffset, setSubtitleOffset] = useState(readSubtitleOffset);
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
      const keep = `[data-menu],[data-kn-action="${SUBTITLE_PANEL}"]`;
      if (!(e.target as HTMLElement | null)?.closest?.(keep)) setOpenMenu(null);
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
    const moveSubtitles = (next: (o: SubtitleOffset) => SubtitleOffset) =>
      setSubtitleOffset((o) => {
        const n = next(o);
        saveSubtitleOffset(n);
        return n;
      });
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
      nudgeSubtitles: (dy) => moveSubtitles((o) => nudgeSubtitleOffset(o, dy)),
      resetSubtitles: () => moveSubtitles(() => NO_OFFSET),
      toggleSubtitlePanel: () => setOpenMenu((m) => (m === SUBTITLE_PANEL ? null : SUBTITLE_PANEL)),
      poke,
    };
  }, [videoRef, rootRef, durationHint, poke]);

  // Keyboard shortcuts. TanStack Hotkeys skips keys a focused control needs
  // (typing in a field, Space on a button, arrows in a menu or slider) and
  // only matches exact modifiers, so Ctrl+K and friends are left alone.
  const run = (fn: () => void) => () => {
    fn();
    poke();
  };
  const volumeBy = (delta: number) => () => actions.setVolume((videoRef.current?.volume ?? 1) + delta);
  // While the subtitle panel is open, Up/Down (and a TV remote's D-pad) move the
  // subtitles instead of changing the volume. Left/Right still seek.
  const positioning = openMenu === SUBTITLE_PANEL;
  const vertical = (normal: () => void, dy: number) =>
    run(() => (positioning ? actions.nudgeSubtitles(dy * SUBTITLE_STEP) : normal()));
  useHotkeys([
    { hotkey: "Space", callback: run(actions.toggle) },
    { hotkey: "K", callback: run(actions.toggle) },
    { hotkey: "ArrowLeft", callback: run(() => actions.skip(-5)) },
    { hotkey: "ArrowRight", callback: run(() => actions.skip(5)) },
    { hotkey: "J", callback: run(() => actions.skip(-10)) },
    { hotkey: "L", callback: run(() => actions.skip(10)) },
    { hotkey: "ArrowUp", callback: vertical(volumeBy(0.1), 1) },
    { hotkey: "ArrowDown", callback: vertical(volumeBy(-0.1), -1) },
    { hotkey: "M", callback: run(actions.toggleMute) },
    { hotkey: "F", callback: run(actions.toggleFullscreen) },
    { hotkey: "I", callback: run(actions.toggleInfo) },
    {
      hotkey: "Escape",
      callback: () => (openMenu !== null ? setOpenMenu(null) : setInfoOpen(false)),
      options: { enabled: openMenu !== null || infoOpen },
    },
  ]);

  const state: PlayerState = {
    ...snap,
    fullscreen,
    infoOpen,
    openMenu,
    subtitleOffset,
    controlsVisible: active || !snap.playing || scrubbing || openMenu !== null,
  };
  return { state, actions };
}

// ---------------------------------------------------------------------------
// Player styles (layouts)
// ---------------------------------------------------------------------------

/**
 * The layout this server uses for every client: an admin picks it on the
 * Modules page. `ready` stays false until the setting and the skins have
 * loaded, so the player doesn't flash one layout and then switch.
 */
function usePlayerStyle() {
  const { themes, current } = useTheme();
  const setting = useApi<PlayerStyleSetting>("api/player/style");
  const skinList = useApi<PlayerSkin[]>("api/player-skins");
  const skins = useMemo(() => skinList.data ?? [], [skinList.data]);
  const themeLayout = themes.find((t) => t.id === current)?.player?.layout;
  const appSkin = useSkin().skin;
  const style = resolvePlayerStyle(
    setting.data?.style,
    themeLayout,
    skins.map((s) => s.id),
    appSkin?.bundle.player?.style,
  );
  return { style, skin: skins.find((k) => k.id === style), ready: !setting.loading && !skinList.loading };
}

// ---------------------------------------------------------------------------
// 2. Shared controls: every layout is built from these
// ---------------------------------------------------------------------------

interface Ctx {
  s: PlayerState;
  a: PlayerActions;
  props: MediaPlayerProps;
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
  // Arrow keys while the seek bar itself has focus (they stop here, so the
  // player-wide arrow shortcuts don't also fire).
  useHotkeys(
    [
      { hotkey: "ArrowLeft", callback: () => a.skip(-5) },
      { hotkey: "ArrowRight", callback: () => a.skip(5) },
    ],
    { target: ref },
  );
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
      data-kn-bar
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
  useHotkeys(
    [
      { hotkey: "ArrowLeft", callback: () => a.setVolume(s.volume - 0.1) },
      { hotkey: "ArrowRight", callback: () => a.setVolume(s.volume + 0.1) },
    ],
    { target: ref },
  );
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
        // With visible text (Pro layout) the name must include it, so voice control ("click English") works.
        label={text ? `${label}: ${text}` : label}
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

/** Audio, subtitles, speed, info, picture-in-picture, fullscreen: the same set in every layout. */
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
      {props.subtitles && props.subtitles.options.length > 1 && (
        <div data-menu className="pointer-events-auto">
          <IconBtn
            label="Subtitle position"
            onClick={a.toggleSubtitlePanel}
            active={s.openMenu === SUBTITLE_PANEL}
            className={labelled ? "h-9 w-9" : "h-10 w-10"}
          >
            <MoveVertical className="h-5 w-5" />
          </IconBtn>
        </div>
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

const INFO_ICONS: Record<InfoIcon, typeof Film> = {
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

/** What the panel can say with no help from the page. */
function fallbackInfo(v: HTMLVideoElement | null, s: PlayerState): PlayerInfo {
  const q = v?.getVideoPlaybackQuality?.();
  return {
    basic: [
      {
        title: "Playback",
        icon: "video",
        rows: [
          ["Resolution", v?.videoWidth ? `${v.videoWidth}×${v.videoHeight}` : "–"],
          ["Buffered ahead", `${Math.max(0, Math.round(s.bufferedEnd - s.currentTime))} s`],
          ["Dropped frames", String(q?.droppedVideoFrames ?? 0)],
        ],
      },
    ],
    trace: [],
  };
}

function InfoSectionView({ section, dense }: { section: InfoSection; dense?: boolean }) {
  const Icon = INFO_ICONS[section.icon];
  return (
    <section className="min-w-0">
      <h3 className="mb-1.5 flex items-center gap-2 font-semibold text-white">
        <Icon className="h-4 w-4 text-primary" /> {section.title}
      </h3>
      {dense ? (
        <dl className="grid grid-cols-[minmax(8rem,max-content)_1fr] gap-x-4 gap-y-0.5 font-mono text-xs">
          {section.rows.map(([k, v], i) => (
            <div key={`${k}-${i}`} className="contents">
              <dt className="text-white/55">{k}</dt>
              <dd className="min-w-0 [overflow-wrap:anywhere] text-white/90">{v}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <dl className="space-y-0.5">
          {section.rows.map(([k, v], i) => (
            <div key={`${k}-${i}`} className="flex gap-1.5">
              <dt className="shrink-0 text-white/60">{k}:</dt>
              <dd className="min-w-0 truncate font-medium text-white" title={v}>
                {v}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}

/**
 * Playback details, the same in every layout. "Overview" is the short list
 * most people want; "Trace" is everything we know, for chasing a problem.
 */
function InfoPanel({ c }: { c: Ctx }) {
  const [tab, setTab] = useState<"basic" | "trace">("basic");
  const [copied, setCopied] = useState(false);
  const latest = useRef(c);
  latest.current = c;
  const read = useCallback(() => {
    const { props, s } = latest.current;
    return props.getInfo?.() ?? fallbackInfo(props.videoRef.current, s);
  }, []);
  const [info, setInfo] = useState<PlayerInfo>(read);
  useEffect(() => {
    setInfo(read());
    const timer = window.setInterval(() => setInfo(read()), 1000);
    return () => clearInterval(timer);
  }, [read]);

  const copy = () => {
    navigator.clipboard
      ?.writeText(infoToText(read()))
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };
  const tabBtn = (id: "basic" | "trace", label: string) => (
    <button
      type="button"
      role="tab"
      aria-selected={tab === id}
      onClick={() => setTab(id)}
      className={cn(
        "cursor-pointer rounded-md px-3 py-1 text-xs font-medium transition-colors",
        tab === id ? "bg-white/20 text-white" : "text-white/60 hover:text-white",
      )}
    >
      {label}
    </button>
  );

  return (
    <div
      role="dialog"
      aria-label="Playback info"
      className="pointer-events-auto absolute top-20 left-4 z-10 flex max-h-[calc(100%-12rem)] w-[min(60rem,calc(100%-2rem))] flex-col rounded-xl bg-black/80 text-sm text-white shadow-2xl"
      style={{ backdropFilter: "blur(var(--kn-player-bar-blur))" }}
    >
      <div className="flex items-center gap-2 border-b border-white/10 px-4 py-2">
        {info.trace.length > 0 && (
          <div role="tablist" className="flex gap-1 rounded-lg bg-white/5 p-0.5">
            {tabBtn("basic", "Overview")}
            {tabBtn("trace", "Trace")}
          </div>
        )}
        <span className="flex-1" />
        {tab === "trace" && (
          <button
            type="button"
            onClick={copy}
            className="flex cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-xs text-white/70 hover:bg-white/10 hover:text-white"
          >
            {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            {copied ? "Copied" : "Copy all"}
          </button>
        )}
        <button
          type="button"
          aria-label="Close playback info"
          onClick={c.a.toggleInfo}
          className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full hover:bg-white/15"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
      {/* Focusable so a keyboard or TV remote can scroll it when it doesn't fit. */}
      <div
        tabIndex={0}
        className="overflow-y-auto overscroll-contain p-4 focus-visible:outline-2 focus-visible:outline-ring"
      >
        {tab === "basic" || info.trace.length === 0 ? (
          <div className="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-4">
            {info.basic.map((sec) => (
              <InfoSectionView key={sec.title} section={sec} />
            ))}
          </div>
        ) : (
          <div className="space-y-5">
            {info.trace.map((sec) => (
              <InfoSectionView key={sec.title} section={sec} dense />
            ))}
          </div>
        )}
      </div>
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
        data-kn-bar
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
        data-kn-bar
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
// Subtitles: drawn above the controls, moved up or down with a small panel
// ---------------------------------------------------------------------------

/** Space between the subtitles (or the panel) and the top of the controls bar, in px. */
const BAR_GAP = 12;
/** Default distance from the bottom edge when the controls are hidden, in percent. */
const SUBTITLE_BASE = 6;

/** The cues showing right now, from whichever track Player.tsx set to "hidden". */
function useActiveCues(videoRef: RefObject<HTMLVideoElement | null>): TextTrackCue[] {
  const [cues, setCues] = useState<TextTrackCue[]>([]);
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const list = v.textTracks;
    let bound: TextTrack[] = [];
    const read = () => {
      const out: TextTrackCue[] = [];
      for (const t of Array.from(list)) {
        if (t.mode === "hidden" && t.activeCues) out.push(...Array.from(t.activeCues));
      }
      setCues(out);
    };
    const rebind = () => {
      for (const t of bound) t.removeEventListener("cuechange", read);
      bound = Array.from(list);
      for (const t of bound) t.addEventListener("cuechange", read);
      read();
    };
    rebind();
    list.addEventListener("addtrack", rebind);
    list.addEventListener("removetrack", rebind);
    list.addEventListener("change", read);
    return () => {
      for (const t of bound) t.removeEventListener("cuechange", read);
      list.removeEventListener("addtrack", rebind);
      list.removeEventListener("removetrack", rebind);
      list.removeEventListener("change", read);
    };
  }, [videoRef]);
  return cues;
}

/**
 * How far the top of the controls bar is from the bottom of the player, in
 * px. Layouts (and skins) mark their bar with `data-kn-bar`; a skin without
 * one gets 15% of the player height.
 */
function useBarHeight(rootRef: RefObject<HTMLElement | null>, deps: unknown[]): number {
  const [height, setHeight] = useState(0);
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const measure = () => {
      const r = root.getBoundingClientRect();
      const bars = Array.from(root.querySelectorAll<HTMLElement>("[data-kn-bar]"));
      let top = 0;
      for (const b of bars) {
        const br = b.getBoundingClientRect();
        if (br.height > 0) top = Math.max(top, r.bottom - br.top);
      }
      setHeight(Math.round(bars.length ? top : r.height * 0.15));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(root);
    for (const b of root.querySelectorAll("[data-kn-bar]")) ro.observe(b);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rootRef, ...deps]);
  return height;
}

/** One cue, keeping WebVTT's own formatting (italics, bold) but nothing else. */
function CueText({ cue }: { cue: TextTrackCue }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // getCueAsHTML only builds text and a few inline tags (b, i, u, ruby, ...), never scripts.
    if (typeof VTTCue !== "undefined" && cue instanceof VTTCue) el.replaceChildren(cue.getCueAsHTML());
    else el.textContent = (cue as TextTrackCue & { text?: string }).text ?? "";
  }, [cue]);
  return <span ref={ref} className="whitespace-pre-line" />;
}

function SubtitleOverlay({ c, cues, barHeight }: { c: Ctx; cues: TextTrackCue[]; barHeight: number }) {
  const { subtitleOffset: o, controlsVisible, openMenu } = c.s;
  const positioning = openMenu === SUBTITLE_PANEL;
  if (cues.length === 0 && !positioning) return null;
  const lift = controlsVisible ? barHeight + BAR_GAP : 0;
  return (
    <div
      aria-live="off"
      // Spans the width minus a 5% margin each side and stays centred, so a long
      // line wraps onto the next line instead of running off the screen.
      className="pointer-events-none absolute inset-x-[5%] z-[5] flex flex-col items-center gap-1 text-center transition-[bottom] duration-300"
      style={{
        // Near the bottom edge, or just above the bar while the controls show, then the viewer's nudge.
        bottom: `calc(max(${SUBTITLE_BASE}%, ${lift}px) + ${o}%)`,
        fontSize: "clamp(1rem, 0.6rem + 1.9vw, 2.75rem)",
        lineHeight: 1.3,
      }}
    >
      {cues.length > 0 ? (
        cues.map((cue, i) => (
          <p
            // Cues have no stable id; their start time + position in the list is unique enough.
            key={`${cue.startTime}-${i}`}
            className="max-w-full rounded-md bg-black/60 px-[0.4em] py-[0.1em] [overflow-wrap:anywhere] text-white [box-decoration-break:clone] [text-shadow:0_1px_3px_rgb(0_0_0/0.9)]"
          >
            <CueText cue={cue} />
          </p>
        ))
      ) : (
        <p className="rounded-md border border-dashed border-white/60 bg-black/60 px-[0.4em] py-[0.1em] text-white/90">
          Subtitles appear here
        </p>
      )}
    </div>
  );
}

/** Up/down buttons: nudge the subtitles a step at a time, or put them back. */
function SubtitlePanel({ c, barHeight }: { c: Ctx; barHeight: number }) {
  const { s, a } = c;
  const o = s.subtitleOffset;
  const btn =
    "flex h-11 w-11 cursor-pointer items-center justify-center rounded-full bg-white/10 text-white transition-colors hover:bg-white/25 focus-visible:outline-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-40";
  const step = (dy: number) => () => {
    a.nudgeSubtitles(dy * SUBTITLE_STEP);
    a.poke();
  };
  return (
    <div
      data-menu
      role="group"
      aria-label="Subtitle position"
      className="pointer-events-auto absolute right-4 z-20 w-56 rounded-xl border border-white/10 bg-black/85 p-4 text-sm text-white shadow-2xl"
      style={{ bottom: barHeight + BAR_GAP, backdropFilter: "blur(var(--kn-player-bar-blur))" }}
    >
      <div className="mb-3 flex items-center justify-between">
        <p className="font-medium">Subtitle position</p>
        <button
          type="button"
          aria-label="Close"
          onClick={() => a.setOpenMenu(null)}
          className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full hover:bg-white/15"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
      <div className="mx-auto flex w-max flex-col items-center gap-1.5">
        <button type="button" aria-label="Move subtitles up" className={btn} onClick={step(1)}>
          <ChevronUp className="h-5 w-5" />
        </button>
        <button
          type="button"
          aria-label="Reset subtitle position"
          title="Reset"
          className={cn(btn, "bg-primary/25 hover:bg-primary/40")}
          onClick={a.resetSubtitles}
          disabled={o === 0}
        >
          <RotateCcw className="h-4 w-4" />
        </button>
        <button type="button" aria-label="Move subtitles down" className={btn} onClick={step(-1)}>
          <ChevronDown className="h-5 w-5" />
        </button>
      </div>
      <p className="mt-3 text-center tabular-nums text-white/80" aria-live="polite">
        {describeSubtitleOffset(o)}
      </p>
      <p className="mt-1 text-center text-xs text-white/50">
        The up and down arrow keys move them too. Saved on this device.
      </p>
    </div>
  );
}

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
        [SUBTITLE_PANEL]: a.toggleSubtitlePanel,
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
      const { props } = latest.current;
      const speed: PlayerChoice = speedChoice(latest.current);
      const map: Record<string, PlayerChoice | undefined> = {
        audio: props.audio,
        subtitles: props.subtitles,
        speed,
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
  const { style, skin, ready } = usePlayerStyle();
  const cues = useActiveCues(videoRef);
  const barHeight = useBarHeight(rootRef, [ready, style, skin]);

  const c: Ctx = { s, a, props };
  const Layout = LAYOUTS[style] ?? MinimalLayout;
  const hidden = !s.controlsVisible;

  return (
    <div
      ref={rootRef}
      data-player-style={style}
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

      {!ready ? null : skin ? (
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

      {ready && <SubtitleOverlay c={c} cues={cues} barHeight={barHeight} />}
      {ready && s.openMenu === SUBTITLE_PANEL && <SubtitlePanel c={c} barHeight={barHeight} />}
      {ready && s.infoOpen && !skin && <InfoPanel c={c} />}

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

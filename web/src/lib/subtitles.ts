import type { PlaybackStart, SubtitleTrack } from "./types";

const STORAGE_KEY = "kinora.subtitles";

/** What the viewer last chose: a language code, or "off". */
export function readSubtitlePreference(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function saveSubtitlePreference(value: string) {
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    // private mode: choice just won't persist
  }
}

/**
 * Which subtitle track to show when playback starts.
 * 1. If the viewer chose "off" last time, show none.
 * 2. If they chose a language, use the first supported track in it.
 * 3. Otherwise show a track the file marks as default or forced, if any.
 * Returns the stream index, or null for no subtitles.
 */
export function pickSubtitle(tracks: SubtitleTrack[], preference: string | null): number | null {
  const usable = tracks.filter((t) => t.supported);
  if (preference === "off") return null;
  if (preference) {
    const match = usable.find((t) => t.language === preference);
    if (match) return match.index;
  }
  return (usable.find((t) => t.is_default) ?? usable.find((t) => t.is_forced))?.index ?? null;
}

/**
 * The subtitle track to start with: what the server remembered for this show
 * (module "auto-tracks"), unless it can't be shown here, then this device's
 * last choice via `pickSubtitle`.
 */
export function startSubtitle(p: Pick<PlaybackStart, "subtitles" | "auto_tracks">, preference: string | null) {
  const auto = p.auto_tracks?.subtitle_index;
  if (auto === null) return null;
  if (auto !== undefined && p.subtitles.some((t) => t.index === auto && t.supported)) return auto;
  return pickSubtitle(p.subtitles, preference);
}

/** Where the remembered tracks came from, for the info panel. */
export function describeAutoTracks(p: Pick<PlaybackStart, "auto_tracks"> | null): string {
  const from = p?.auto_tracks?.from;
  if (!from) return "no remembered choice";
  return `your last choice for this ${from === "series" ? "show" : from}`;
}

// ---------------------------------------------------------------------------
// How high subtitles sit on screen
// ---------------------------------------------------------------------------

const OFFSET_KEY = "kinora.subtitle-offset";

/**
 * The viewer's nudge up (+) or down (-) from the default spot, in percent of
 * the player height. Percent keeps the same spot on a phone and a TV.
 * Subtitles always stay centred horizontally.
 */
export type SubtitleOffset = number;

export const NO_OFFSET: SubtitleOffset = 0;
/** How far one press of the up/down buttons moves the subtitles, in percent. */
export const SUBTITLE_STEP = 1;
/** Keeps the text on screen: from just under the default spot to near the top. */
export const SUBTITLE_LIMITS = { min: -5, max: 80 } as const;

export function clampSubtitleOffset(y: number): SubtitleOffset {
  const v = Number.isFinite(y) ? Math.round(y) : 0;
  return Math.min(SUBTITLE_LIMITS.max, Math.max(SUBTITLE_LIMITS.min, v)) || 0; // `|| 0` turns -0 into 0
}

export function nudgeSubtitleOffset(y: SubtitleOffset, dy: number): SubtitleOffset {
  return clampSubtitleOffset(y + dy);
}

/** Saved per device: the right spot depends on the screen. */
export function readSubtitleOffset(): SubtitleOffset {
  try {
    const raw = JSON.parse(localStorage.getItem(OFFSET_KEY) ?? "null") as unknown;
    // Older builds saved {x, y}; only the height is kept now.
    const y = typeof raw === "object" && raw !== null ? (raw as { y?: unknown }).y : raw;
    return y === null || y === undefined ? NO_OFFSET : clampSubtitleOffset(Number(y));
  } catch {
    return NO_OFFSET;
  }
}

export function saveSubtitleOffset(y: SubtitleOffset) {
  try {
    localStorage.setItem(OFFSET_KEY, JSON.stringify(y));
  } catch {
    // private mode: the position just won't persist
  }
}

/** "Up 3%", "Down 1%" or "Default position". */
export function describeSubtitleOffset(y: SubtitleOffset): string {
  return y === 0 ? "Default position" : `${y > 0 ? "Up" : "Down"} ${Math.abs(y)}%`;
}

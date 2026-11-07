import type { SubtitleTrack } from "./types";

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

// ---------------------------------------------------------------------------
// Where subtitles sit on screen
// ---------------------------------------------------------------------------

const OFFSET_KEY = "kinora.subtitle-offset";

/**
 * The viewer's nudge away from the default spot (bottom centre), in percent
 * of the player: `x` is width (+ is right), `y` is height (+ is up). Percent
 * keeps the same spot on a phone and a TV.
 */
export interface SubtitleOffset {
  x: number;
  y: number;
}

export const NO_OFFSET: SubtitleOffset = { x: 0, y: 0 };
/** How far one press of the D-pad moves the subtitles, in percent. */
export const SUBTITLE_STEP = 1;
/** Keeps the text on screen: ±40% sideways, from just under the default spot to near the top. */
export const SUBTITLE_LIMITS = { x: 40, yMin: -5, yMax: 80 } as const;

export function clampSubtitleOffset(o: SubtitleOffset): SubtitleOffset {
  const fix = (n: number, lo: number, hi: number) => {
    const v = Number.isFinite(n) ? Math.round(n) : 0;
    return Math.min(hi, Math.max(lo, v)) || 0; // `|| 0` turns -0 into 0
  };
  return {
    x: fix(o.x, -SUBTITLE_LIMITS.x, SUBTITLE_LIMITS.x),
    y: fix(o.y, SUBTITLE_LIMITS.yMin, SUBTITLE_LIMITS.yMax),
  };
}

export function nudgeSubtitleOffset(o: SubtitleOffset, dx: number, dy: number): SubtitleOffset {
  return clampSubtitleOffset({ x: o.x + dx, y: o.y + dy });
}

/** Saved per device: the right spot depends on the screen. */
export function readSubtitleOffset(): SubtitleOffset {
  try {
    const raw = JSON.parse(localStorage.getItem(OFFSET_KEY) ?? "null") as Partial<SubtitleOffset> | null;
    return raw ? clampSubtitleOffset({ x: Number(raw.x), y: Number(raw.y) }) : NO_OFFSET;
  } catch {
    return NO_OFFSET;
  }
}

export function saveSubtitleOffset(o: SubtitleOffset) {
  try {
    localStorage.setItem(OFFSET_KEY, JSON.stringify(o));
  } catch {
    // private mode: the position just won't persist
  }
}

/** "Up 3% · Right 2%", or "Default position". */
export function describeSubtitleOffset(o: SubtitleOffset): string {
  const parts = [
    o.y !== 0 ? `${o.y > 0 ? "Up" : "Down"} ${Math.abs(o.y)}%` : null,
    o.x !== 0 ? `${o.x > 0 ? "Right" : "Left"} ${Math.abs(o.x)}%` : null,
  ].filter(Boolean);
  return parts.length ? parts.join(" · ") : "Default position";
}

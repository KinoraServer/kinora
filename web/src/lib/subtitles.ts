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

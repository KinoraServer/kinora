import { i18n } from "@/i18n";

export function formatDuration(ms: number | null | undefined): string {
  if (!ms || ms <= 0) return "";
  const totalMinutes = Math.round(ms / 60000);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  if (h === 0) return `${m}m`;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

export function formatClock(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) seconds = 0;
  const s = Math.floor(seconds % 60);
  const m = Math.floor((seconds / 60) % 60);
  const h = Math.floor(seconds / 3600);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function formatDate(ms: number | null | undefined): string {
  if (!ms) return i18n.t("common.never");
  return new Date(ms).toLocaleString(i18n.language);
}

export function episodeCode(season: number | null, episode: number | null): string {
  if (season == null || episode == null) return "";
  return `S${season.toString().padStart(2, "0")}E${episode.toString().padStart(2, "0")}`;
}

export function progressPercent(position: number | null | undefined, duration: number | null | undefined): number {
  if (!position || !duration) return 0;
  return Math.min(100, Math.max(0, (position / duration) * 100));
}

/** "2hr 56min", "2hr", "45min" (rounded down to the minute). */
function hoursMinutes(ms: number): string {
  const totalMinutes = Math.floor(ms / 60000);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  if (h === 0) return i18n.t("time.min", { m });
  return m === 0 ? i18n.t("time.hr", { h }) : i18n.t("time.hr_min", { h, m });
}

/**
 * Where someone is in a video, for the dashboard: "45:07 of 58:21" for videos
 * under an hour, "2hr 56min of 3hr 10min" (or "45min of 2hr 10min") for longer ones.
 */
export function formatPlaybackMarker(positionMs: number | null | undefined, durationMs: number): string {
  const long = durationMs >= 3_600_000;
  const fmt = (ms: number) => (long ? hoursMinutes(ms) : formatClock(ms / 1000));
  const position =
    positionMs == null || positionMs < 0 ? "–" : fmt(durationMs > 0 ? Math.min(positionMs, durationMs) : positionMs);
  return i18n.t("time.position_of", { position, duration: durationMs > 0 ? fmt(durationMs) : "–" });
}

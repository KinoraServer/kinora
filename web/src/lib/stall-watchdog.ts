// Gets a player moving again when it wedges with plenty of video buffered.
//
// Seen at the start of transcoded streams: the first segment begins a little
// after 0 (copied video keeps its B-frame delay), hls.js seeks over that start
// gap, and Chrome then sits at ~0.27 s with 30+ s buffered, readyState 2
// ("have current data"), and never plays. hls.js only nudges the playhead
// when it sees a hole in the buffer, and here there is none, so the spinner
// stayed until Chrome gave up on its own almost a minute later
// (mediaSourceRequiresReset). This watchdog spots that state within seconds:
// first it nudges the playhead, then the player starts a fresh stream.

/** What the watchdog looks at, once a second. */
export interface PlayheadSample {
  currentTime: number;
  paused: boolean;
  seeking: boolean;
  ended: boolean;
  /** Seconds buffered ahead of currentTime in the range the playhead is in. */
  bufferedAhead: number;
}

export type WatchdogAction = "none" | "nudge" | "recover";

/** Seconds stuck before the first nudge. */
export const NUDGE_AFTER_S = 3;
/** Seconds stuck (in total) before the player starts a fresh stream. */
export const RECOVER_AFTER_S = 6;
/** This much must be buffered ahead for "stuck" to mean wedged, not starved. */
export const MIN_BUFFER_AHEAD_S = 3;
/** How far a nudge moves the playhead. */
export const NUDGE_S = 0.1;
/** Fresh streams allowed per minute, so a file that never plays isn't hammered. */
export const MAX_RECOVERIES_PER_MINUTE = 2;

export class StallWatchdog {
  private lastTime: number | null = null;
  private stuckFor = 0;
  private nudged = false;
  private recoveries: number[] = [];

  /** Call once a second. `now` is in ms. Returns what the caller should do. */
  tick(s: PlayheadSample, now: number): WatchdogAction {
    const moved = this.lastTime === null || s.currentTime !== this.lastTime;
    this.lastTime = s.currentTime;
    const shouldPlay = !s.paused && !s.seeking && !s.ended;
    if (moved || !shouldPlay || s.bufferedAhead < MIN_BUFFER_AHEAD_S) {
      this.stuckFor = 0;
      this.nudged = false;
      return "none";
    }
    this.stuckFor++;
    if (!this.nudged && this.stuckFor >= NUDGE_AFTER_S) {
      this.nudged = true;
      // The nudge itself moves the playhead: only movement past it counts.
      this.lastTime = s.currentTime + NUDGE_S;
      return "nudge";
    }
    if (this.stuckFor >= RECOVER_AFTER_S) {
      this.recoveries = this.recoveries.filter((t) => now - t < 60_000);
      if (this.recoveries.length >= MAX_RECOVERIES_PER_MINUTE) return "none";
      this.recoveries.push(now);
      this.stuckFor = 0;
      this.nudged = false;
      return "recover";
    }
    return "none";
  }
}

/**
 * Where to start when the stream's first media begins just after the start
 * position: the buffered start, if it's a small gap at the very beginning.
 * Seeking there before playback starts avoids hls.js skipping the gap
 * mid-play, which is what tends to wedge Chrome.
 */
export function startGapTarget(buffered: TimeRanges, currentTime: number): number | null {
  if (buffered.length === 0) return null;
  const start = buffered.start(0);
  const gap = start - currentTime;
  return gap > 0 && gap < 2 && currentTime < 1 ? start : null;
}

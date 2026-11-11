import { describe, expect, it } from "vitest";
import "@/i18n";
import { episodeCode, formatBytes, formatClock, formatDuration, formatPlaybackMarker, progressPercent } from "./format";

describe("format", () => {
  it("durations", () => {
    expect(formatDuration(0)).toBe("");
    expect(formatDuration(45 * 60000)).toBe("45m");
    expect(formatDuration(120 * 60000)).toBe("2h");
    expect(formatDuration(148 * 60000)).toBe("2h 28m");
  });
  it("clock", () => {
    expect(formatClock(65)).toBe("1:05");
    expect(formatClock(3725)).toBe("1:02:05");
    expect(formatClock(NaN)).toBe("0:00");
  });
  it("bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
  });
  it("episode codes and progress", () => {
    expect(episodeCode(1, 2)).toBe("S01E02");
    expect(episodeCode(null, 2)).toBe("");
    expect(progressPercent(50, 200)).toBe(25);
    expect(progressPercent(300, 200)).toBe(100);
  });
  it("playback marker: clock under an hour, hours and minutes over", () => {
    expect(formatPlaybackMarker(45 * 60_000 + 7_000, 58 * 60_000 + 21_000)).toBe("45:07 of 58:21");
    expect(formatPlaybackMarker(0, 22 * 60_000)).toBe("0:00 of 22:00");
    expect(formatPlaybackMarker((2 * 60 + 56) * 60_000 + 30_000, (3 * 60 + 10) * 60_000)).toBe(
      "2hr 56min of 3hr 10min",
    );
    expect(formatPlaybackMarker(45 * 60_000, (2 * 60 + 10) * 60_000)).toBe("45min of 2hr 10min");
    expect(formatPlaybackMarker(2 * 3_600_000, 3 * 3_600_000)).toBe("2hr of 3hr");
    expect(formatPlaybackMarker(null, 3_600_000)).toBe("– of 1hr");
    expect(formatPlaybackMarker(99 * 60_000, 60 * 60_000)).toBe("1hr of 1hr");
  });
});

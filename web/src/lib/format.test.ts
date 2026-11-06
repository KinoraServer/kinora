import { describe, expect, it } from "vitest";
import { episodeCode, formatBytes, formatClock, formatDuration, progressPercent } from "./format";

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
});

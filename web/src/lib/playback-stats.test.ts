import { describe, expect, it } from "vitest";
import {
  aspectRatio,
  bufferedAhead,
  codecName,
  formatBitrate,
  formatRanges,
  infoToText,
  networkStateName,
  readyStateName,
} from "./playback-stats";

const ranges = (...r: [number, number][]): TimeRanges => ({
  length: r.length,
  start: (i: number) => r[i][0],
  end: (i: number) => r[i][1],
});

describe("playback stats formatting", () => {
  it("names codecs", () => {
    expect(codecName("h264")).toBe("H.264");
    expect(codecName("eac3")).toBe("E-AC3");
    expect(codecName("prores")).toBe("PRORES");
    expect(codecName(null)).toBe("–");
  });

  it("formats bitrates", () => {
    expect(formatBitrate(128_000)).toBe("128 kbps");
    expect(formatBitrate(4_500_000)).toBe("4.5 Mbps");
    expect(formatBitrate(45_000_000)).toBe("45 Mbps");
    expect(formatBitrate(0)).toBe("–");
    expect(formatBitrate(null)).toBe("–");
  });

  it("works out aspect ratios", () => {
    expect(aspectRatio(1920, 1080)).toBe("16:9");
    expect(aspectRatio(1920, 804)).toBe("2.39:1");
    expect(aspectRatio(1440, 1080)).toBe("4:3");
    expect(aspectRatio(1000, 300)).toBe("3.33:1");
    expect(aspectRatio(0, 0)).toBe("–");
  });

  it("names element states", () => {
    expect(readyStateName(4)).toBe("4 HAVE_ENOUGH_DATA");
    expect(networkStateName(2)).toBe("2 NETWORK_LOADING");
  });

  it("reads buffered ranges", () => {
    const r = ranges([0, 12.4], [30, 61.25]);
    expect(formatRanges(r)).toBe("0.0–12.4, 30.0–61.3");
    expect(formatRanges(ranges())).toBe("none");
    expect(bufferedAhead(r, 40)).toBeCloseTo(21.25);
    expect(bufferedAhead(r, 20)).toBe(0);
  });

  it("turns everything into copyable text", () => {
    const text = infoToText({
      basic: [{ title: "Video", icon: "video", rows: [["Codec", "H.264"]] }],
      trace: [{ title: "Session", icon: "server", rows: [["method", "remux"]] }],
    });
    expect(text).toBe("## Video\nCodec: H.264\n\n## Session\nmethod: remux");
  });
});

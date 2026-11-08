import { describe, expect, it } from "vitest";
import "@/i18n";
import { liveStreams, liveStreamsNow } from "@/stories/fixtures";
import {
  livePosition,
  startedAgo,
  streamBadges,
  streamBandwidth,
  streamInfoSections,
  streamReason,
  streamSubtitle,
  streamTitle,
  streamTotals,
  streamTraceSections,
} from "./streams";

const [transcode, directPlay, remux, paused] = liveStreams;
const rows = (sections: { title: string; rows: [string, string][] }[], title: string) =>
  Object.fromEntries(sections.find((s) => s.title === title)!.rows);

describe("streams", () => {
  it("titles: movies show the year, episodes the show and episode", () => {
    expect(streamTitle(transcode)).toBe("Dune: Part Two");
    expect(streamSubtitle(transcode)).toBe("(2024)");
    expect(streamTitle(directPlay)).toBe("Severance");
    expect(streamSubtitle(directPlay)).toBe("S2 E5 · Trojan's Horse");
  });

  it("badges and reasons follow the method", () => {
    expect(streamBadges(transcode)).toEqual(["NVIDIA NVENC", "4K → 1080p"]);
    expect(streamBadges(directPlay)).toEqual(["HEVC · 4K HDR10"]);
    expect(streamReason(directPlay)).toBe("Original file, nothing converted");
    expect(streamReason(remux)).toBe("Direct Stream: MKV isn't supported by this browser");
    expect(streamReason(transcode)).toMatch(/^Transcoding: Dolby Vision/);
  });

  it("totals count every method and add up the bandwidth", () => {
    expect(streamTotals(liveStreams)).toEqual({
      total: 4,
      direct_play: 2,
      remux: 1,
      transcode: 1,
      bandwidth: 18_200_000 + 42_000_000 + 9_800_000,
    });
  });

  it("the marker moves on between refreshes only while playing", () => {
    expect(livePosition(transcode, 1000, 4000)).toBe(5_950_000);
    expect(livePosition(paused, 1000, 4000)).toBe(10_590_000);
    expect(livePosition(remux, 1000, 4000)).toBe(1_020_000);
    expect(livePosition({ ...paused, state: "playing" }, 0, 10_000_000)).toBe(11_400_000);
    expect(livePosition({ ...paused, position_ms: null }, 0, 1)).toBeNull();
  });

  it("started ago", () => {
    expect(startedAgo(0, 30_000)).toBe("started just now");
    expect(startedAgo(0, 12 * 60_000)).toBe("started 12 min ago");
    expect(startedAgo(0, 185 * 60_000)).toBe("started 3 hr ago");
  });

  it("bandwidth line includes the transcoder's speed", () => {
    expect(streamBandwidth(transcode)).toBe("18 Mbps · ffmpeg 2.4× realtime");
    expect(streamBandwidth(paused)).toBe("0 Mbps · paused");
  });

  it("info shows source → output", () => {
    const info = streamInfoSections(transcode, liveStreamsNow);
    const video = rows(info, "Video");
    expect(video["Source"]).toBe("HEVC · Main 10 · 3840×2160");
    expect(video["Output"]).toBe("H.264 · 1920×1080");
    expect(video["Dynamic range"]).toBe("Dolby Vision 8.1 → SDR");
    expect(rows(info, "Audio")["Output"]).toBe("AAC · 6ch");
    expect(rows(streamInfoSections(directPlay, liveStreamsNow), "Audio")["Output"]).toBe("copied");
    expect(rows(info, "Viewer")["Address"]).toBe("192.168.1.24");
  });

  it("trace has the transcoder only for HLS, and says when a client sends no stats", () => {
    const t = streamTraceSections(transcode, liveStreamsNow);
    expect(t.map((s) => s.title)).toEqual(["Session", "Transcoder", "Player", "Errors and warnings"]);
    expect(rows(t, "Transcoder")["speed / fps"]).toBe("2.4× · 58 fps");
    expect(rows(t, "Transcoder")["transcoded through"]).toBe("1018 (+126 s)");
    expect(rows(t, "Player")["dropped / total frames"]).toBe("14 / 142288");
    const d = streamTraceSections(directPlay, liveStreamsNow);
    expect(d.map((s) => s.title)).toEqual(["Session", "Player", "Errors and warnings"]);
    expect(rows(d, "Player")["player"]).toBe("This app doesn't send player stats yet.");
  });
});

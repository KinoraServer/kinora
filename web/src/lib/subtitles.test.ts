import { describe, expect, it } from "vitest";
import {
  clampSubtitleOffset,
  describeSubtitleOffset,
  nudgeSubtitleOffset,
  pickSubtitle,
  readSubtitleOffset,
  saveSubtitleOffset,
} from "./subtitles";
import type { SubtitleTrack } from "./types";

const track = (index: number, over: Partial<SubtitleTrack> = {}): SubtitleTrack => ({
  index,
  label: `t${index}`,
  language: "eng",
  url: `u${index}`,
  codec: "subrip",
  is_default: false,
  is_forced: false,
  supported: true,
  ...over,
});

describe("pickSubtitle", () => {
  it("shows nothing by default when no track is flagged", () => {
    expect(pickSubtitle([track(2), track(3)], null)).toBeNull();
  });
  it("uses a default or forced track", () => {
    expect(pickSubtitle([track(2), track(3, { is_default: true })], null)).toBe(3);
    expect(pickSubtitle([track(2), track(3, { is_forced: true })], null)).toBe(3);
  });
  it("remembers 'off' and languages", () => {
    const tracks = [track(2, { is_default: true }), track(3, { language: "jpn" })];
    expect(pickSubtitle(tracks, "off")).toBeNull();
    expect(pickSubtitle(tracks, "jpn")).toBe(3);
    expect(pickSubtitle(tracks, "fra")).toBe(2); // no French track: fall back to the file's default
  });
  it("never picks image subtitles", () => {
    expect(pickSubtitle([track(2, { supported: false, is_default: true })], null)).toBeNull();
  });
});

describe("subtitle offset", () => {
  it("moves one step per press and stays on screen", () => {
    expect(nudgeSubtitleOffset({ x: 0, y: 0 }, 0, 1)).toEqual({ x: 0, y: 1 });
    expect(nudgeSubtitleOffset({ x: 40, y: 80 }, 1, 1)).toEqual({ x: 40, y: 80 });
    expect(nudgeSubtitleOffset({ x: -40, y: -5 }, -1, -1)).toEqual({ x: -40, y: -5 });
  });

  it("cleans up bad values", () => {
    expect(clampSubtitleOffset({ x: Number.NaN, y: 2.6 })).toEqual({ x: 0, y: 3 });
    expect(Object.is(clampSubtitleOffset({ x: -0.2, y: 0 }).x, 0)).toBe(true);
  });

  it("describes the position", () => {
    expect(describeSubtitleOffset({ x: 0, y: 0 })).toBe("Default position");
    expect(describeSubtitleOffset({ x: -2, y: 3 })).toBe("Up 3% · Left 2%");
    expect(describeSubtitleOffset({ x: 4, y: -1 })).toBe("Down 1% · Right 4%");
  });

  it("round-trips through storage and ignores junk", () => {
    saveSubtitleOffset({ x: 5, y: 10 });
    expect(readSubtitleOffset()).toEqual({ x: 5, y: 10 });
    localStorage.setItem("kinora.subtitle-offset", "{nope");
    expect(readSubtitleOffset()).toEqual({ x: 0, y: 0 });
    localStorage.setItem("kinora.subtitle-offset", JSON.stringify({ x: 999, y: "7" }));
    expect(readSubtitleOffset()).toEqual({ x: 40, y: 7 });
  });
});

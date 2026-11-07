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
    expect(nudgeSubtitleOffset(0, 1)).toBe(1);
    expect(nudgeSubtitleOffset(80, 1)).toBe(80);
    expect(nudgeSubtitleOffset(-5, -1)).toBe(-5);
  });

  it("cleans up bad values", () => {
    expect(clampSubtitleOffset(Number.NaN)).toBe(0);
    expect(clampSubtitleOffset(2.6)).toBe(3);
    expect(Object.is(clampSubtitleOffset(-0.2), 0)).toBe(true);
  });

  it("describes the position", () => {
    expect(describeSubtitleOffset(0)).toBe("Default position");
    expect(describeSubtitleOffset(3)).toBe("Up 3%");
    expect(describeSubtitleOffset(-1)).toBe("Down 1%");
  });

  it("round-trips through storage and ignores junk", () => {
    saveSubtitleOffset(10);
    expect(readSubtitleOffset()).toBe(10);
    localStorage.setItem("kinora.subtitle-offset", "{nope");
    expect(readSubtitleOffset()).toBe(0);
    localStorage.setItem("kinora.subtitle-offset", "999");
    expect(readSubtitleOffset()).toBe(80);
  });

  it("keeps the height from the old {x, y} format", () => {
    localStorage.setItem("kinora.subtitle-offset", JSON.stringify({ x: 12, y: 7 }));
    expect(readSubtitleOffset()).toBe(7);
  });
});

import { describe, expect, it } from "vitest";
import { pickSubtitle } from "./subtitles";
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

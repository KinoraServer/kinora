import { describe, expect, it } from "vitest";
import { detectCaps } from "./caps";

describe("detectCaps", () => {
  it("chrome-like browser: no HEVC, no HDR", () => {
    const supported = ['video/mp4; codecs="avc1.640028"', 'audio/mp4; codecs="mp4a.40.2"', "video/webm"];
    const caps = detectCaps((m) => supported.includes(m), false);
    expect(caps.video_codecs).toEqual(["h264"]);
    expect(caps.audio_codecs).toEqual(["aac"]);
    expect(caps.containers).toEqual(["mp4", "webm"]);
    expect(caps.hdr10).toBe(false);
  });

  it("safari-like browser on an HDR screen gets Dolby Vision", () => {
    const caps = detectCaps(() => true, true);
    expect(caps.video_codecs).toContain("hevc");
    expect(caps.hdr10 && caps.dolby_vision).toBe(true);
  });

  it("HDR needs an HDR screen", () => {
    expect(detectCaps(() => true, false).dolby_vision).toBe(false);
  });
});

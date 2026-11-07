// Renders the real <MediaPlayer> the way pages/Player.tsx does, but with a
// small local video instead of a stream from the server: a 90 minute
// gradient clip in .storybook/public/sample.webm (136 KB). Stories choose
// where playback starts and which subtitle line is on screen.

import { useEffect, useRef, useState } from "react";
import { MediaPlayer, type MediaPlayerProps } from "@/components/MediaPlayer";

export const SAMPLE_VIDEO = "/sample.webm";

export interface PlayerHarnessProps extends Omit<MediaPlayerProps, "videoRef" | "audio" | "subtitles" | "children"> {
  /** Where the video sits, in seconds. */
  startAt?: number;
  /** A subtitle line to show at `startAt`. Supports WebVTT tags like <i> and <b>. */
  subtitleText?: string;
  /** Audio tracks to offer; the first is selected. */
  audioOptions?: string[];
  /** Subtitle tracks to offer; "Off" is added in front, and the first real one is selected. */
  subtitleOptions?: string[];
  /** Leave out the video source, e.g. for the loading and error states. */
  noVideo?: boolean;
}

export function PlayerHarness({
  startAt = 0,
  subtitleText,
  audioOptions = [],
  subtitleOptions = [],
  noVideo = false,
  ...props
}: PlayerHarnessProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [audio, setAudio] = useState("0");
  const [subtitle, setSubtitle] = useState(subtitleOptions.length ? "0" : "off");

  useEffect(() => {
    const v = videoRef.current;
    if (!v || noVideo) return;
    v.muted = true; // so the browser lets the stories autoplay if someone presses play
    v.src = SAMPLE_VIDEO;
    const seek = () => {
      v.currentTime = startAt;
    };
    v.addEventListener("loadedmetadata", seek, { once: true });
    return () => {
      v.removeEventListener("loadedmetadata", seek);
      v.removeAttribute("src");
      v.load();
    };
  }, [noVideo, startAt]);

  // A "hidden" text track is what Player.tsx hands the player: the browser
  // tracks the cues and MediaPlayer draws them itself.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !subtitleText || subtitle === "off") return;
    const track = v.addTextTrack("subtitles", "Story", "en");
    track.mode = "hidden";
    const cue = new VTTCue(Math.max(0, startAt - 1), startAt + 3600, subtitleText);
    track.addCue(cue);
    return () => {
      track.removeCue(cue);
      track.mode = "disabled";
    };
  }, [subtitleText, startAt, subtitle]);

  return (
    <MediaPlayer
      {...props}
      videoRef={videoRef}
      audio={
        audioOptions.length
          ? { options: audioOptions.map((label, i) => ({ value: String(i), label })), value: audio, onChange: setAudio }
          : undefined
      }
      subtitles={
        subtitleOptions.length
          ? {
              options: [
                { value: "off", label: "Off" },
                ...subtitleOptions.map((label, i) => ({ value: String(i), label })),
              ],
              value: subtitle,
              onChange: setSubtitle,
            }
          : undefined
      }
    />
  );
}

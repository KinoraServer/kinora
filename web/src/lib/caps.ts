// Figure out what this browser can play, so the server can pick direct play
// whenever possible. Sent with every playback request.

export interface ClientCaps {
  containers: string[];
  video_codecs: string[];
  audio_codecs: string[];
  max_audio_channels: number;
  hdr10: boolean;
  hlg: boolean;
  dolby_vision: boolean;
  max_height?: number;
}

type Probe = (mime: string) => boolean;

function browserProbe(): Probe {
  const video = document.createElement("video");
  const mse = (window as unknown as { MediaSource?: { isTypeSupported(t: string): boolean } }).MediaSource;
  return (mime) => {
    try {
      return video.canPlayType(mime) === "probably" || (mse?.isTypeSupported(mime) ?? false);
    } catch {
      return false;
    }
  };
}

/** Pure part, testable with a fake probe. */
export function detectCaps(can: Probe, highDynamicRangeScreen: boolean): ClientCaps {
  const video: string[] = [];
  if (can('video/mp4; codecs="avc1.640028"')) video.push("h264");
  const hevc = can('video/mp4; codecs="hvc1.2.4.L153.B0"') || can('video/mp4; codecs="hev1.2.4.L153.B0"');
  if (hevc) video.push("hevc");
  if (can('video/webm; codecs="vp09.00.10.08"')) video.push("vp9");
  if (can('video/mp4; codecs="av01.0.08M.08"')) video.push("av1");

  const audio: string[] = [];
  if (can('audio/mp4; codecs="mp4a.40.2"')) audio.push("aac");
  if (can("audio/mpeg") || can('audio/mp4; codecs="mp3"')) audio.push("mp3");
  if (can('audio/webm; codecs="opus"') || can('audio/mp4; codecs="opus"')) audio.push("opus");
  if (can('audio/mp4; codecs="flac"') || can("audio/flac")) audio.push("flac");
  if (can('audio/mp4; codecs="ac-3"')) audio.push("ac3");
  if (can('audio/mp4; codecs="ec-3"')) audio.push("eac3");

  const containers = ["mp4"];
  if (can('video/webm; codecs="vp9"') || can("video/webm")) containers.push("webm");

  return {
    containers,
    video_codecs: video,
    audio_codecs: audio,
    max_audio_channels: 6,
    hdr10: highDynamicRangeScreen && hevc,
    hlg: highDynamicRangeScreen && hevc,
    dolby_vision:
      highDynamicRangeScreen && (can('video/mp4; codecs="dvh1.05.06"') || can('video/mp4; codecs="dvh1.08.06"')),
  };
}

export function browserCaps(): ClientCaps {
  const hdr = window.matchMedia?.("(dynamic-range: high)").matches ?? false;
  return detectCaps(browserProbe(), hdr);
}

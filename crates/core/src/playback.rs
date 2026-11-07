//! The playback decision engine: direct play, remux, or transcode?
//!
//! This is a pure function of (file, client capabilities, limits). Every rule
//! about codecs, HDR and Dolby Vision lives here so it can be tested with a
//! table of cases instead of real files and real devices.

use serde::{Deserialize, Serialize};

use crate::media::{AudioStream, HdrFormat, MediaInfo, VideoStream};

/// What the client says it can play. The web app builds this from
/// `MediaSource.isTypeSupported` / `canPlayType`; TV apps will send their own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientCaps {
    /// Containers the client can play as a plain file: "mp4", "webm", "mkv".
    pub containers: Vec<String>,
    /// ffmpeg codec names: "h264", "hevc", "av1", "vp9".
    pub video_codecs: Vec<String>,
    /// ffmpeg codec names: "aac", "mp3", "opus", "flac", "ac3", "eac3".
    pub audio_codecs: Vec<String>,
    #[serde(default = "default_channels")]
    pub max_audio_channels: u32,
    #[serde(default)]
    pub hdr10: bool,
    #[serde(default)]
    pub hlg: bool,
    #[serde(default)]
    pub dolby_vision: bool,
    /// Maximum 10-bit support for H.264/HEVC decode is assumed when the codec is listed.
    #[serde(default)]
    pub max_height: Option<u32>,
}

fn default_channels() -> u32 {
    2
}

/// Server/user side limits. `max_bitrate` is where network-aware adaptive
/// streaming plugs in later.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub max_bitrate: Option<u64>,
    pub preferred_audio_language: Option<String>,
    /// Pick a specific audio stream (ffprobe index).
    pub audio_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Serve the original file untouched (zero CPU).
    DirectPlay,
    /// Copy the streams into HLS without re-encoding (very cheap).
    Remux,
    /// Re-encode at least one stream.
    Transcode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VideoAction {
    Copy,
    Encode {
        /// Always "h264" for now: the one codec every client decodes.
        codec: String,
        width: u32,
        height: u32,
        bitrate: u64,
        /// Convert HDR / Dolby Vision to SDR.
        tonemap: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioAction {
    Copy,
    Encode { codec: String, channels: u32, bitrate: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackPlan {
    pub method: Method,
    pub video: Option<VideoAction>,
    pub audio_index: Option<u32>,
    pub audio: Option<AudioAction>,
    /// Human readable reasons for anything other than direct play.
    pub reasons: Vec<String>,
}

pub fn decide(media: &MediaInfo, caps: &ClientCaps, limits: &Limits) -> PlaybackPlan {
    let mut reasons = Vec::new();

    let audio_stream = pick_audio(&media.audio, limits);
    let audio_ok = match audio_stream {
        None => true,
        Some(a) => audio_compatible(a, caps, &mut reasons),
    };

    let video_ok = match &media.video {
        None => true,
        Some(v) => video_compatible(v, caps, &mut reasons),
    };

    let over_bitrate = match (limits.max_bitrate, media.bitrate) {
        (Some(max), Some(actual)) if actual > max => {
            reasons.push(format!("bitrate {} Mbps is over the {} Mbps limit", actual / 1_000_000, max / 1_000_000));
            true
        }
        _ => false,
    };

    let container_ok = caps.containers.iter().any(|c| container_matches(&media.container, c));
    if !container_ok {
        reasons.push(format!("container {} is not supported", media.container));
    }

    let encode_audio = |a: &AudioStream| {
        let channels = a.channels.min(caps.max_audio_channels).clamp(1, 6);
        AudioAction::Encode { codec: "aac".into(), channels, bitrate: if channels > 2 { 384_000 } else { 192_000 } }
    };

    let mut video_action = media.video.as_ref().map(|v| {
        if video_ok && !over_bitrate {
            VideoAction::Copy
        } else {
            encode_target(v, caps, limits, media.bitrate)
        }
    });
    let mut audio_action = audio_stream.map(|a| if audio_ok { AudioAction::Copy } else { encode_audio(a) });

    let all_copy = |v: &Option<VideoAction>, a: &Option<AudioAction>| {
        matches!(v, None | Some(VideoAction::Copy)) && matches!(a, None | Some(AudioAction::Copy))
    };

    let method = if all_copy(&video_action, &audio_action) && container_ok {
        Method::DirectPlay
    } else {
        // Anything not direct-played goes out as HLS in MPEG-TS segments.
        // Only H.264 video and a few audio codecs can be copied into that
        // reliably across players, so re-encode the rest.
        if let (Some(VideoAction::Copy), Some(v)) = (&video_action, &media.video) {
            if v.codec != "h264" {
                reasons.push(format!("{} cannot be copied into HLS for this client", v.codec));
                video_action = Some(encode_target(v, caps, limits, media.bitrate));
            }
        }
        if let (Some(AudioAction::Copy), Some(a)) = (&audio_action, audio_stream) {
            if !HLS_COPY_AUDIO.contains(&a.codec.as_str()) {
                audio_action = Some(encode_audio(a));
            }
        }
        if all_copy(&video_action, &audio_action) {
            Method::Remux
        } else {
            Method::Transcode
        }
    };

    PlaybackPlan {
        method,
        video: video_action,
        audio_index: audio_stream.map(|a| a.index),
        audio: audio_action,
        reasons,
    }
}

/// Audio codecs that can be stream-copied into MPEG-TS HLS segments.
const HLS_COPY_AUDIO: &[&str] = &["aac", "mp3", "ac3", "eac3"];

fn container_matches(probe_name: &str, client: &str) -> bool {
    match client {
        "mp4" => probe_name == "mov" || probe_name == "mp4",
        "mkv" => probe_name == "matroska",
        "webm" => probe_name == "webm",
        other => probe_name == other,
    }
}

fn pick_audio<'a>(streams: &'a [AudioStream], limits: &Limits) -> Option<&'a AudioStream> {
    if let Some(i) = limits.audio_index {
        if let Some(a) = streams.iter().find(|a| a.index == i) {
            return Some(a);
        }
    }
    if let Some(lang) = &limits.preferred_audio_language {
        if let Some(a) = streams.iter().find(|a| a.language.as_deref() == Some(lang.as_str())) {
            return Some(a);
        }
    }
    streams.iter().find(|a| a.is_default).or_else(|| streams.first())
}

fn audio_compatible(a: &AudioStream, caps: &ClientCaps, reasons: &mut Vec<String>) -> bool {
    if !caps.audio_codecs.iter().any(|c| c == &a.codec) {
        reasons.push(format!("audio codec {} is not supported", a.codec));
        return false;
    }
    if a.channels > caps.max_audio_channels {
        reasons.push(format!("{} audio channels, client supports {}", a.channels, caps.max_audio_channels));
        return false;
    }
    true
}

fn video_compatible(v: &VideoStream, caps: &ClientCaps, reasons: &mut Vec<String>) -> bool {
    let mut ok = true;
    if !caps.video_codecs.iter().any(|c| c == &v.codec) {
        reasons.push(format!("video codec {} is not supported", v.codec));
        ok = false;
    }
    if v.codec == "h264" && v.bit_depth > 8 {
        reasons.push("10-bit H.264 is not supported by most clients".into());
        ok = false;
    }
    if let Some(max) = caps.max_height {
        if v.height > max {
            reasons.push(format!("{}p is above the client's {}p limit", v.height, max));
            ok = false;
        }
    }
    if !hdr_compatible(v, caps) {
        reasons.push(match v.hdr {
            HdrFormat::DolbyVision => format!(
                "Dolby Vision profile {} has no fallback this client can display",
                v.dv_profile.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
            ),
            other => format!("client cannot display {other:?}"),
        });
        ok = false;
    }
    ok
}

/// Can the client show this HDR format correctly (or a compatible fallback)?
///
/// Dolby Vision files carry a base layer plus extra metadata:
/// - compat 1 (profile 8.1, most 7): base layer is HDR10, so an HDR10 client plays it fine.
/// - compat 4 (profile 8.4): base layer is HLG.
/// - compat 2 (profile 8.2): base layer is SDR.
/// - compat 0 (profile 5): no usable base layer; only real DV clients show correct colours.
pub fn hdr_compatible(v: &VideoStream, caps: &ClientCaps) -> bool {
    match v.hdr {
        HdrFormat::Sdr => true,
        HdrFormat::Hdr10 => caps.hdr10,
        HdrFormat::Hlg => caps.hlg,
        HdrFormat::DolbyVision => {
            caps.dolby_vision
                || match v.dv_compat_id {
                    Some(1) => caps.hdr10,
                    Some(4) => caps.hlg,
                    Some(2) => true,
                    _ => false,
                }
        }
    }
}

fn encode_target(v: &VideoStream, caps: &ClientCaps, limits: &Limits, source_bitrate: Option<u64>) -> VideoAction {
    let max_h = caps.max_height.unwrap_or(u32::MAX);
    let height = v.height.min(max_h).max(1);
    let width = if v.height == 0 {
        v.width
    } else {
        // Keep aspect ratio; encoders want even dimensions.
        ((u64::from(v.width) * u64::from(height) / u64::from(v.height)) as u32) & !1
    };
    let height = height & !1;

    let default_bitrate = match height {
        h if h >= 2000 => 25_000_000,
        h if h >= 1000 => 10_000_000,
        h if h >= 700 => 5_000_000,
        _ => 2_500_000,
    };
    let mut bitrate = default_bitrate;
    if let Some(src) = source_bitrate.filter(|b| *b > 0) {
        bitrate = bitrate.min(src);
    }
    if let Some(max) = limits.max_bitrate {
        // Leave room for audio.
        bitrate = bitrate.min(max.saturating_sub(400_000).max(500_000));
    }

    VideoAction::Encode { codec: "h264".into(), width, height, bitrate, tonemap: v.hdr != HdrFormat::Sdr }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{HdrFormat, MediaInfo, VideoStream};

    fn video(codec: &str, height: u32, hdr: HdrFormat, dv: Option<(u8, u8)>) -> VideoStream {
        VideoStream {
            index: 0,
            codec: codec.into(),
            profile: None,
            width: height * 16 / 9,
            height,
            bit_depth: if hdr == HdrFormat::Sdr { 8 } else { 10 },
            pix_fmt: None,
            hdr,
            dv_profile: dv.map(|d| d.0),
            dv_compat_id: dv.map(|d| d.1),
            bitrate: None,
            frame_rate: Some(24.0),
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            color_range: None,
        }
    }

    fn audio(codec: &str, channels: u32) -> AudioStream {
        AudioStream {
            index: 1,
            codec: codec.into(),
            channels,
            language: Some("eng".into()),
            title: None,
            is_default: true,
            bitrate: None,
            profile: None,
            sample_rate: None,
            channel_layout: None,
        }
    }

    fn file(container: &str, v: VideoStream, a: AudioStream) -> MediaInfo {
        MediaInfo {
            container: container.into(),
            duration_ms: 60_000,
            bitrate: Some(8_000_000),
            video: Some(v),
            audio: vec![a],
            subtitles: vec![],
        }
    }

    /// Typical Chrome on a desktop: no HEVC, no HDR.
    fn chrome() -> ClientCaps {
        ClientCaps {
            containers: vec!["mp4".into(), "webm".into()],
            video_codecs: vec!["h264".into(), "vp9".into(), "av1".into()],
            audio_codecs: vec!["aac".into(), "mp3".into(), "opus".into(), "flac".into()],
            max_audio_channels: 2,
            hdr10: false,
            hlg: false,
            dolby_vision: false,
            max_height: None,
        }
    }

    /// Safari on a recent Mac: HEVC, HDR10 and Dolby Vision.
    fn safari() -> ClientCaps {
        ClientCaps {
            containers: vec!["mp4".into()],
            video_codecs: vec!["h264".into(), "hevc".into()],
            audio_codecs: vec!["aac".into(), "mp3".into(), "ac3".into(), "eac3".into(), "flac".into()],
            max_audio_channels: 6,
            hdr10: true,
            hlg: true,
            dolby_vision: true,
            max_height: None,
        }
    }

    #[test]
    fn mp4_h264_aac_direct_plays() {
        let f = file("mov", video("h264", 1080, HdrFormat::Sdr, None), audio("aac", 2));
        let p = decide(&f, &chrome(), &Limits::default());
        assert_eq!(p.method, Method::DirectPlay);
        assert!(p.reasons.is_empty());
    }

    #[test]
    fn mkv_h264_aac_is_remuxed() {
        let f = file("matroska", video("h264", 1080, HdrFormat::Sdr, None), audio("aac", 2));
        let p = decide(&f, &chrome(), &Limits::default());
        assert_eq!(p.method, Method::Remux);
        assert_eq!(p.video, Some(VideoAction::Copy));
    }

    #[test]
    fn surround_audio_is_transcoded_but_video_copied() {
        let f = file("mov", video("h264", 1080, HdrFormat::Sdr, None), audio("eac3", 6));
        let p = decide(&f, &chrome(), &Limits::default());
        assert_eq!(p.method, Method::Transcode);
        assert_eq!(p.video, Some(VideoAction::Copy));
        assert_eq!(p.audio, Some(AudioAction::Encode { codec: "aac".into(), channels: 2, bitrate: 192_000 }));
    }

    #[test]
    fn hevc_on_chrome_is_transcoded_to_h264() {
        let f = file("matroska", video("hevc", 1080, HdrFormat::Sdr, None), audio("aac", 2));
        let p = decide(&f, &chrome(), &Limits::default());
        assert_eq!(p.method, Method::Transcode);
        match p.video.unwrap() {
            VideoAction::Encode { codec, height, tonemap, .. } => {
                assert_eq!(codec, "h264");
                assert_eq!(height, 1080);
                assert!(!tonemap);
            }
            other => panic!("expected encode, got {other:?}"),
        }
    }

    #[test]
    fn dolby_vision_matrix() {
        // (dv profile, compat id, client, expect direct-compatible video)
        let cases = [
            ((8, 1), safari(), true),                                                     // DV client
            ((5, 0), safari(), true),                                                     // DV client handles profile 5
            ((8, 1), ClientCaps { dolby_vision: false, ..safari() }, true),               // plays HDR10 base layer
            ((5, 0), ClientCaps { dolby_vision: false, ..safari() }, false),              // P5 needs tone mapping
            ((8, 4), ClientCaps { dolby_vision: false, hdr10: false, ..safari() }, true), // HLG base
        ];
        for ((profile, compat), caps, expect_ok) in cases {
            let v = video("hevc", 2160, HdrFormat::DolbyVision, Some((profile, compat)));
            assert_eq!(
                hdr_compatible(&v, &caps),
                expect_ok,
                "DV {profile}.{compat} with dv={} hdr10={}",
                caps.dolby_vision,
                caps.hdr10
            );
        }
    }

    #[test]
    fn dv_profile5_on_sdr_client_tonemaps() {
        let f = file("matroska", video("hevc", 2160, HdrFormat::DolbyVision, Some((5, 0))), audio("aac", 2));
        let caps = ClientCaps { video_codecs: vec!["h264".into(), "hevc".into()], ..chrome() };
        let p = decide(&f, &caps, &Limits::default());
        assert_eq!(p.method, Method::Transcode);
        assert!(matches!(p.video, Some(VideoAction::Encode { tonemap: true, .. })));
        assert!(p.reasons.iter().any(|r| r.contains("Dolby Vision profile 5")));
    }

    #[test]
    fn hdr10_on_sdr_client_tonemaps() {
        let f = file("mov", video("hevc", 2160, HdrFormat::Hdr10, None), audio("aac", 2));
        let caps = ClientCaps { video_codecs: vec!["h264".into(), "hevc".into()], ..chrome() };
        let p = decide(&f, &caps, &Limits::default());
        assert!(matches!(p.video, Some(VideoAction::Encode { tonemap: true, height: 2160, .. })));
    }

    #[test]
    fn bitrate_limit_forces_transcode_and_caps_bitrate() {
        let mut f = file("mov", video("h264", 2160, HdrFormat::Sdr, None), audio("aac", 2));
        f.bitrate = Some(60_000_000);
        let limits = Limits { max_bitrate: Some(8_000_000), ..Default::default() };
        let p = decide(&f, &chrome(), &limits);
        assert_eq!(p.method, Method::Transcode);
        match p.video.unwrap() {
            VideoAction::Encode { bitrate, .. } => assert_eq!(bitrate, 7_600_000),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn max_height_downscales_and_keeps_aspect() {
        let f = file("mov", video("h264", 2160, HdrFormat::Sdr, None), audio("aac", 2));
        let caps = ClientCaps { max_height: Some(1080), ..chrome() };
        let p = decide(&f, &caps, &Limits::default());
        match p.video.unwrap() {
            VideoAction::Encode { width, height, .. } => assert_eq!((width, height), (1920, 1080)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn picks_preferred_audio_language() {
        let mut f = file("mov", video("h264", 1080, HdrFormat::Sdr, None), audio("aac", 2));
        f.audio.push(AudioStream { index: 2, language: Some("jpn".into()), is_default: false, ..audio("aac", 2) });
        let limits = Limits { preferred_audio_language: Some("jpn".into()), ..Default::default() };
        assert_eq!(decide(&f, &chrome(), &limits).audio_index, Some(2));
        assert_eq!(decide(&f, &chrome(), &Limits::default()).audio_index, Some(1));
    }

    #[test]
    fn ten_bit_h264_is_transcoded() {
        let mut v = video("h264", 1080, HdrFormat::Sdr, None);
        v.bit_depth = 10;
        let f = file("mov", v, audio("aac", 2));
        assert_eq!(decide(&f, &chrome(), &Limits::default()).method, Method::Transcode);
    }
}

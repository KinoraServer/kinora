//! Media file description, built from `ffprobe -print_format json` output.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HdrFormat {
    #[default]
    Sdr,
    Hdr10,
    Hlg,
    /// Dolby Vision. See `dv_profile` / `dv_compat_id` on the stream.
    DolbyVision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStream {
    pub index: u32,
    /// ffmpeg codec name: h264, hevc, av1, vp9, mpeg2video, ...
    pub codec: String,
    pub profile: Option<String>,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub pix_fmt: Option<String>,
    pub hdr: HdrFormat,
    /// Dolby Vision profile (5, 7, 8, ...).
    pub dv_profile: Option<u8>,
    /// Dolby Vision base-layer compatibility: 1 = HDR10, 2 = SDR, 4 = HLG, 0 = none.
    pub dv_compat_id: Option<u8>,
    pub bitrate: Option<u64>,
    pub frame_rate: Option<f64>,
    /// ffprobe colour tags (bt709, bt2020nc, smpte2084, tv, ...). Missing in
    /// files scanned before they were recorded.
    #[serde(default)]
    pub color_space: Option<String>,
    #[serde(default)]
    pub color_primaries: Option<String>,
    #[serde(default)]
    pub color_transfer: Option<String>,
    #[serde(default)]
    pub color_range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub channels: u32,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub bitrate: Option<u64>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub channel_layout: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleStream {
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    /// SDH / closed captions. Missing in files scanned before it was recorded;
    /// [`is_hearing_impaired`] also looks at the title.
    #[serde(default)]
    pub is_hearing_impaired: bool,
    /// Text subs (srt, ass, webvtt) can be converted; image subs (PGS, VobSub) must be burned in.
    pub is_text: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    /// First entry of ffprobe's comma separated format name (e.g. "matroska", "mov").
    pub container: String,
    pub duration_ms: u64,
    pub bitrate: Option<u64>,
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioStream>,
    pub subtitles: Vec<SubtitleStream>,
}

impl MediaInfo {
    /// Short label for the UI, e.g. "4K HEVC Dolby Vision 8.1".
    pub fn video_label(&self) -> Option<String> {
        let v = self.video.as_ref()?;
        let res = match v.height {
            h if h >= 2000 || v.width >= 3800 => "4K",
            h if h >= 1000 || v.width >= 1900 => "1080p",
            h if h >= 700 || v.width >= 1260 => "720p",
            _ => "SD",
        };
        let hdr = match v.hdr {
            HdrFormat::Sdr => String::new(),
            HdrFormat::Hdr10 => " HDR10".into(),
            HdrFormat::Hlg => " HLG".into(),
            HdrFormat::DolbyVision => match (v.dv_profile, v.dv_compat_id) {
                (Some(p), Some(c)) if c > 0 => format!(" Dolby Vision {p}.{c}"),
                (Some(p), _) => format!(" Dolby Vision {p}"),
                _ => " Dolby Vision".into(),
            },
        };
        Some(format!("{res} {}{hdr}", v.codec.to_uppercase()))
    }
}

const TEXT_SUB_CODECS: &[&str] = &["subrip", "srt", "ass", "ssa", "webvtt", "mov_text", "text"];

#[derive(Debug, PartialEq, Eq)]
pub struct ProbeError(pub String);

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid ffprobe output: {}", self.0)
    }
}
impl std::error::Error for ProbeError {}

/// Parse the JSON from `ffprobe -v error -print_format json -show_format -show_streams`.
pub fn parse_ffprobe(json: &str) -> Result<MediaInfo, ProbeError> {
    let root: Value = serde_json::from_str(json).map_err(|e| ProbeError(e.to_string()))?;
    let format = root.get("format").ok_or_else(|| ProbeError("missing format".into()))?;
    let streams = root.get("streams").and_then(Value::as_array).cloned().unwrap_or_default();

    let container = format
        .get("format_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .split(',')
        .next()
        .unwrap_or("unknown")
        .to_string();
    let duration_ms = parse_f64(format.get("duration")).map(|d| (d * 1000.0).round() as u64).unwrap_or(0);
    let bitrate = parse_u64(format.get("bit_rate"));

    let mut video = None;
    let mut audio = Vec::new();
    let mut subtitles = Vec::new();

    for s in &streams {
        let index = s.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
        let codec = s.get("codec_name").and_then(Value::as_str).unwrap_or("unknown").to_string();
        let tags = s.get("tags");
        let language = tag(tags, "language").filter(|l| l != "und");
        let title = tag(tags, "title");
        let disposition = s.get("disposition");
        let is_default = flag(disposition, "default");

        match s.get("codec_type").and_then(Value::as_str) {
            Some("video") if video.is_none() && !flag(disposition, "attached_pic") => {
                video = Some(parse_video(s, index, codec));
            }
            Some("audio") => audio.push(AudioStream {
                index,
                codec,
                channels: s.get("channels").and_then(Value::as_u64).unwrap_or(2) as u32,
                language,
                title,
                is_default,
                bitrate: parse_u64(s.get("bit_rate")),
                profile: string(s.get("profile")),
                sample_rate: parse_u64(s.get("sample_rate")).map(|n| n as u32),
                channel_layout: string(s.get("channel_layout")),
            }),
            Some("subtitle") => subtitles.push(SubtitleStream {
                index,
                is_text: TEXT_SUB_CODECS.contains(&codec.as_str()),
                codec,
                language,
                title,
                is_default,
                is_forced: flag(disposition, "forced"),
                is_hearing_impaired: flag(disposition, "hearing_impaired"),
            }),
            _ => {}
        }
    }

    Ok(MediaInfo { container, duration_ms, bitrate, video, audio, subtitles })
}

fn parse_video(s: &Value, index: u32, codec: String) -> VideoStream {
    let pix_fmt = s.get("pix_fmt").and_then(Value::as_str).map(str::to_string);
    let bit_depth = s
        .get("bits_per_raw_sample")
        .and_then(|v| v.as_str().and_then(|x| x.parse().ok()).or_else(|| v.as_u64().map(|n| n as u8)))
        .or_else(|| {
            pix_fmt.as_deref().map(|p| {
                if p.contains("12") {
                    12
                } else if p.contains("10") {
                    10
                } else {
                    8
                }
            })
        })
        .unwrap_or(8);

    let mut dv_profile = None;
    let mut dv_compat_id = None;
    if let Some(side) = s.get("side_data_list").and_then(Value::as_array) {
        for sd in side {
            let kind = sd.get("side_data_type").and_then(Value::as_str).unwrap_or("");
            if kind.contains("DOVI") || kind.contains("Dolby Vision") {
                dv_profile = sd.get("dv_profile").and_then(Value::as_u64).map(|n| n as u8);
                dv_compat_id = sd.get("dv_bl_signal_compatibility_id").and_then(Value::as_u64).map(|n| n as u8);
            }
        }
    }
    let transfer = s.get("color_transfer").and_then(Value::as_str).unwrap_or("");
    let hdr = if dv_profile.is_some() {
        HdrFormat::DolbyVision
    } else if transfer == "smpte2084" {
        HdrFormat::Hdr10
    } else if transfer == "arib-std-b67" {
        HdrFormat::Hlg
    } else {
        HdrFormat::Sdr
    };

    VideoStream {
        index,
        codec,
        profile: s.get("profile").and_then(Value::as_str).map(str::to_string),
        width: s.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
        height: s.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
        bit_depth,
        pix_fmt,
        hdr,
        dv_profile,
        dv_compat_id,
        bitrate: parse_u64(s.get("bit_rate")),
        frame_rate: s.get("avg_frame_rate").and_then(Value::as_str).and_then(parse_ratio),
        color_space: string(s.get("color_space")),
        color_primaries: string(s.get("color_primaries")),
        color_transfer: string(s.get("color_transfer")).filter(|t| !t.is_empty()),
        color_range: string(s.get("color_range")),
    }
}

/// A non-empty string field ("unknown" counts as missing).
fn string(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty() && *s != "unknown").map(str::to_string)
}

fn parse_ratio(r: &str) -> Option<f64> {
    let (n, d) = r.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d > 0.0).then(|| n / d)
}

fn tag(tags: Option<&Value>, key: &str) -> Option<String> {
    let tags = tags?.as_object()?;
    tags.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).and_then(|(_, v)| v.as_str()).map(str::to_string)
}

fn flag(disposition: Option<&Value>, key: &str) -> bool {
    disposition.and_then(|d| d.get(key)).and_then(Value::as_u64).map(|n| n == 1).unwrap_or(false)
}

fn parse_f64(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_f64().or_else(|| v.as_str()?.parse().ok())
}

fn parse_u64(v: Option<&Value>) -> Option<u64> {
    let v = v?;
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}

/// SDH / closed captions: the ffprobe flag, or a title that says so.
pub fn is_hearing_impaired(s: &SubtitleStream) -> bool {
    let words = |t: &str| -> Vec<String> {
        t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
    };
    s.is_hearing_impaired
        || s.title.as_deref().map(words).is_some_and(|w| {
            w.iter().any(|w| matches!(w.as_str(), "sdh" | "cc" | "hi")) || w.windows(2).any(|p| p[0] == "hearing")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DV81: &str = r#"{
      "streams": [
        {"index":0,"codec_name":"hevc","codec_type":"video","profile":"Main 10","width":3840,"height":2160,
         "pix_fmt":"yuv420p10le","color_transfer":"smpte2084","avg_frame_rate":"24000/1001",
         "color_space":"bt2020nc","color_primaries":"bt2020","color_range":"tv",
         "disposition":{"default":1},
         "side_data_list":[{"side_data_type":"DOVI configuration record","dv_profile":8,"dv_level":6,
            "dv_bl_signal_compatibility_id":1}]},
        {"index":1,"codec_name":"eac3","codec_type":"audio","channels":6,"sample_rate":"48000",
         "channel_layout":"5.1(side)","profile":"Dolby Digital Plus + Dolby Atmos",
         "tags":{"language":"eng","title":"Atmos"},"disposition":{"default":1}},
        {"index":2,"codec_name":"aac","codec_type":"audio","channels":2,"tags":{"language":"jpn"},"disposition":{"default":0}},
        {"index":3,"codec_name":"subrip","codec_type":"subtitle","tags":{"language":"eng"},"disposition":{"default":0,"forced":1}},
        {"index":4,"codec_name":"hdmv_pgs_subtitle","codec_type":"subtitle","tags":{"language":"und"}},
        {"index":5,"codec_name":"mjpeg","codec_type":"video","disposition":{"attached_pic":1}}
      ],
      "format": {"format_name":"matroska,webm","duration":"7260.123000","bit_rate":"45000000"}
    }"#;

    #[test]
    fn parses_dolby_vision_file() {
        let info = parse_ffprobe(DV81).unwrap();
        assert_eq!(info.container, "matroska");
        assert_eq!(info.duration_ms, 7_260_123);
        let v = info.video.as_ref().unwrap();
        assert_eq!(v.codec, "hevc");
        assert_eq!(v.bit_depth, 10);
        assert_eq!(v.hdr, HdrFormat::DolbyVision);
        assert_eq!((v.dv_profile, v.dv_compat_id), (Some(8), Some(1)));
        assert!((v.frame_rate.unwrap() - 23.976).abs() < 0.01);
        assert_eq!(v.color_primaries.as_deref(), Some("bt2020"));
        assert_eq!(v.color_space.as_deref(), Some("bt2020nc"));
        assert_eq!(v.color_range.as_deref(), Some("tv"));
        assert_eq!(info.audio.len(), 2);
        assert_eq!(info.audio[0].sample_rate, Some(48_000));
        assert_eq!(info.audio[0].channel_layout.as_deref(), Some("5.1(side)"));
        assert_eq!(info.audio[1].sample_rate, None);
        assert_eq!(info.audio[0].channels, 6);
        assert_eq!(info.audio[0].language.as_deref(), Some("eng"));
        assert_eq!(info.subtitles.len(), 2);
        assert!(info.subtitles[0].is_text && info.subtitles[0].is_forced);
        assert!(!info.subtitles[1].is_text);
        assert_eq!(info.subtitles[1].language, None, "'und' means unknown");
        assert_eq!(info.video_label().unwrap(), "4K HEVC Dolby Vision 8.1");
    }

    #[test]
    fn hdr10_and_sdr() {
        let hdr = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"hevc","width":3840,"height":2160,
            "pix_fmt":"yuv420p10le","color_transfer":"smpte2084"}],"format":{"format_name":"matroska","duration":"10"}}"#;
        assert_eq!(parse_ffprobe(hdr).unwrap().video.unwrap().hdr, HdrFormat::Hdr10);

        let sdr = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
            "pix_fmt":"yuv420p"}],"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"10"}}"#;
        let info = parse_ffprobe(sdr).unwrap();
        assert_eq!(info.container, "mov");
        let v = info.video.unwrap();
        assert_eq!((v.hdr, v.bit_depth), (HdrFormat::Sdr, 8));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_ffprobe("not json").is_err());
        assert!(parse_ffprobe("{}").is_err());
    }

    #[test]
    fn hearing_impaired_from_the_flag_or_the_title() {
        let sub = |title: Option<&str>, flag: bool| SubtitleStream {
            index: 3,
            codec: "srt".into(),
            language: Some("eng".into()),
            title: title.map(Into::into),
            is_default: false,
            is_forced: false,
            is_hearing_impaired: flag,
            is_text: true,
        };
        assert!(is_hearing_impaired(&sub(None, true)));
        assert!(is_hearing_impaired(&sub(Some("English [SDH]"), false)));
        assert!(is_hearing_impaired(&sub(Some("For the hearing impaired"), false)));
        assert!(!is_hearing_impaired(&sub(Some("English"), false)));
        assert!(!is_hearing_impaired(&sub(Some("Chinese"), false)), "\"hi\" must be a whole word");
    }
}

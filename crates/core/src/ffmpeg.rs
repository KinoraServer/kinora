//! Build ffmpeg command lines.
//!
//! The output is a plain `Vec<String>`, so every combination of
//! (plan x GPU backend x tone mapper) is covered by snapshot tests without
//! ffmpeg installed. The `media` code in the server only spawns what this
//! returns.

use serde::{Deserialize, Serialize};

use crate::playback::{AudioAction, PlaybackPlan, VideoAction};

/// Which hardware video API to use. Picked at startup by probing the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HwAccel {
    /// CPU only (libx264).
    None,
    /// NVIDIA (Windows, Linux).
    Nvenc,
    /// Intel Quick Sync. `device` is the DRM render node on Linux.
    Qsv { device: Option<String> },
    /// VAAPI: Intel and AMD on Linux.
    Vaapi { device: String },
    /// Apple (macOS).
    VideoToolbox,
}

impl HwAccel {
    pub fn label(&self) -> &'static str {
        match self {
            HwAccel::None => "Software (CPU)",
            HwAccel::Nvenc => "NVIDIA NVENC",
            HwAccel::Qsv { .. } => "Intel Quick Sync",
            HwAccel::Vaapi { .. } => "VAAPI",
            HwAccel::VideoToolbox => "Apple VideoToolbox",
        }
    }
}

/// How HDR / Dolby Vision gets converted to SDR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tonemapper {
    /// libplacebo on Vulkan: best quality, understands Dolby Vision metadata
    /// (including profile 5).
    Libplacebo,
    /// zscale + tonemap on the CPU: works everywhere, HDR10/HLG only.
    Zscale,
    /// Neither available: HDR is passed through and colours will look washed out.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HwProfile {
    pub accel: HwAccel,
    pub tonemapper: Tonemapper,
    /// Index of the hardware Vulkan device libplacebo runs on. `None` lets
    /// ffmpeg pick, which can land on a software device such as llvmpipe.
    #[serde(default)]
    pub vulkan_device: Option<u32>,
}

impl Default for HwProfile {
    fn default() -> Self {
        HwProfile { accel: HwAccel::None, tonemapper: Tonemapper::None, vulkan_device: None }
    }
}

/// The next, more conservative profile to try after ffmpeg crashed with `hw`:
/// first swap GPU tone mapping (libplacebo) for CPU tone mapping (zscale),
/// then drop GPU decoding and encoding. `None` once there's nothing left to drop.
pub fn fallback_profile(hw: &HwProfile) -> Option<HwProfile> {
    if hw.tonemapper == Tonemapper::Libplacebo {
        return Some(HwProfile { tonemapper: Tonemapper::Zscale, vulkan_device: None, ..hw.clone() });
    }
    if hw.accel != HwAccel::None {
        return Some(HwProfile { accel: HwAccel::None, ..hw.clone() });
    }
    None
}

/// A Vulkan device as ffmpeg lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulkanDevice {
    pub index: u32,
    pub name: String,
    /// A CPU implementation (Mesa llvmpipe/lavapipe). Far too slow for 4K
    /// tone mapping, and it crashes ffmpeg on large frames.
    pub software: bool,
}

/// Initialise Vulkan with verbose logging so ffmpeg prints its "GPU listing".
pub fn vulkan_list_args() -> Vec<String> {
    [
        "-hide_banner",
        "-v",
        "verbose",
        "-nostdin",
        "-init_hw_device",
        "vulkan=vk",
        "-f",
        "lavfi",
        "-i",
        "nullsrc=s=16x16",
        "-frames:v",
        "1",
        "-f",
        "null",
        "-",
    ]
    .map(String::from)
    .to_vec()
}

/// Parse the "GPU listing" ffmpeg prints when it initialises Vulkan:
///
/// ```text
/// [AVHWDeviceContext @ 0x55e7] GPU listing:
/// [AVHWDeviceContext @ 0x55e7]     0: NVIDIA GeForce RTX 3060 (discrete) (0x2504)
/// [AVHWDeviceContext @ 0x55e7]     1: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)
/// ```
pub fn parse_vulkan_devices(stderr: &str) -> Vec<VulkanDevice> {
    let mut out = Vec::new();
    let mut in_listing = false;
    for line in stderr.lines() {
        let body = line.split_once("] ").map(|(_, b)| b).unwrap_or(line);
        if body.trim() == "GPU listing:" {
            in_listing = true;
            continue;
        }
        if !in_listing {
            continue;
        }
        let entry = body.trim().split_once(": ").and_then(|(i, rest)| Some((i.parse::<u32>().ok()?, rest)));
        let Some((index, rest)) = entry else { break };
        let name = rest.split(" (").next().unwrap_or(rest).to_string();
        out.push(VulkanDevice { index, name, software: rest.contains("(software)") });
    }
    out
}

/// The first hardware (non-software) Vulkan device.
pub fn pick_vulkan_device(devices: &[VulkanDevice]) -> Option<&VulkanDevice> {
    devices.iter().find(|d| !d.software)
}

fn vulkan_init(device: Option<u32>) -> String {
    match device {
        Some(i) => format!("vulkan=vk:{i}"),
        None => "vulkan=vk".into(),
    }
}

pub struct HlsJob<'a> {
    pub input: &'a str,
    pub plan: &'a PlaybackPlan,
    pub hw: &'a HwProfile,
    /// ffprobe index of the video stream, if any.
    pub video_index: Option<u32>,
    /// The source is Dolby Vision (needs software decode to keep its metadata for tone mapping).
    pub source_is_dolby_vision: bool,
    pub frame_rate: Option<f64>,
    pub segment_seconds: u32,
    pub start_segment: u32,
    /// Directory ffmpeg writes segments into.
    pub output_dir: &'a str,
}

pub fn hls_args(job: &HlsJob) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));

    push(&["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);

    let encode = match &job.plan.video {
        Some(VideoAction::Encode { width, height, bitrate, tonemap, .. }) => {
            Some((*width, *height, *bitrate, *tonemap))
        }
        _ => None,
    };

    // libplacebo and VAAPI upload both need a filter device; VAAPI wins and
    // tone mapping falls back to the CPU in that case.
    let tonemapper = match (&job.hw.accel, job.hw.tonemapper) {
        (HwAccel::Vaapi { .. }, Tonemapper::Libplacebo) => Tonemapper::Zscale,
        (_, t) => t,
    };
    let wants_tonemap = encode.map(|e| e.3).unwrap_or(false) && tonemapper != Tonemapper::None;

    // --- input side: hardware devices + decoding -----------------------------
    if encode.is_some() {
        if let HwAccel::Vaapi { device } = &job.hw.accel {
            a.extend(["-init_hw_device".into(), format!("vaapi=va:{device}")]);
            a.extend(["-filter_hw_device".into(), "va".into()]);
        }
        if wants_tonemap && tonemapper == Tonemapper::Libplacebo {
            a.extend(["-init_hw_device".into(), vulkan_init(job.hw.vulkan_device)]);
            a.extend(["-filter_hw_device", "vk"].map(String::from));
        }
        // Decode on the GPU, but let ffmpeg copy frames back to system memory
        // so the same filters work for every vendor. Dolby Vision is decoded
        // in software when tone mapping so its metadata survives.
        let hw_decode = !(wants_tonemap && job.source_is_dolby_vision);
        if hw_decode {
            match &job.hw.accel {
                HwAccel::None => {}
                HwAccel::Nvenc => a.extend(["-hwaccel", "cuda"].map(String::from)),
                HwAccel::Qsv { device: Some(d) } | HwAccel::Vaapi { device: d } => {
                    a.extend(["-hwaccel".into(), "vaapi".into(), "-hwaccel_device".into(), d.clone()])
                }
                HwAccel::Qsv { device: None } => a.extend(["-hwaccel", "d3d11va"].map(String::from)),
                HwAccel::VideoToolbox => a.extend(["-hwaccel", "videotoolbox"].map(String::from)),
            }
        }
    }

    let start_seconds = job.start_segment * job.segment_seconds;
    if start_seconds > 0 {
        a.extend(["-ss".into(), start_seconds.to_string()]);
    }
    // Keep the source timestamps so every segment lines up with the playlist,
    // no matter where ffmpeg was (re)started.
    a.extend(["-copyts", "-start_at_zero", "-avoid_negative_ts", "disabled"].map(String::from));
    a.extend(["-i".into(), job.input.to_string()]);

    // --- stream selection -----------------------------------------------------
    if let Some(v) = job.video_index {
        a.extend(["-map".into(), format!("0:{v}")]);
    }
    if let Some(i) = job.plan.audio_index {
        a.extend(["-map".into(), format!("0:{i}")]);
    }
    a.extend(["-sn", "-dn", "-map_metadata", "-1", "-map_chapters", "-1"].map(String::from));

    // --- video ----------------------------------------------------------------
    match (&job.plan.video, encode) {
        (_, Some((w, h, bitrate, tonemap))) => {
            let filter = video_filter(w, h, tonemap, tonemapper, &job.hw.accel);
            a.extend(["-vf".into(), filter]);
            a.extend(video_encoder(&job.hw.accel, bitrate));
            let seg = job.segment_seconds;
            a.extend(["-force_key_frames".into(), format!("expr:gte(t,{start_seconds}+n_forced*{seg})")]);
            if let Some(fps) = job.frame_rate.filter(|f| *f > 0.0) {
                a.extend(["-g".into(), ((fps * f64::from(seg)).round() as u32).to_string()]);
            }
        }
        (Some(VideoAction::Copy), None) => a.extend(["-c:v", "copy"].map(String::from)),
        (None, None) => a.push("-vn".into()),
        (Some(VideoAction::Encode { .. }), None) => unreachable!(),
    }

    // --- audio ----------------------------------------------------------------
    match &job.plan.audio {
        Some(AudioAction::Copy) => a.extend(["-c:a", "copy"].map(String::from)),
        Some(AudioAction::Encode { codec, channels, bitrate }) => a.extend([
            "-c:a".into(),
            codec.clone(),
            "-ac".into(),
            channels.to_string(),
            "-b:a".into(),
            bitrate.to_string(),
        ]),
        None => a.push("-an".into()),
    }

    // --- HLS output -----------------------------------------------------------
    let dir = job.output_dir.trim_end_matches('/');
    a.extend([
        "-f".into(),
        "hls".into(),
        "-hls_time".into(),
        job.segment_seconds.to_string(),
        "-hls_segment_type".into(),
        "mpegts".into(),
        "-hls_list_size".into(),
        "0".into(),
        "-hls_flags".into(),
        "temp_file".into(),
        "-start_number".into(),
        job.start_segment.to_string(),
        "-hls_segment_filename".into(),
        format!("{dir}/seg_%d.ts"),
        format!("{dir}/ffmpeg.m3u8"),
    ]);
    a
}

fn video_filter(w: u32, h: u32, tonemap: bool, tonemapper: Tonemapper, accel: &HwAccel) -> String {
    let mut f = if tonemap && tonemapper == Tonemapper::Libplacebo {
        // libplacebo scales and tone maps in one pass on the GPU.
        format!(
            "hwupload,libplacebo=w={w}:h={h}:tonemapping=bt.2390:colorspace=bt709:\
             color_primaries=bt709:color_trc=bt709:range=tv:format=yuv420p,hwdownload,format=yuv420p"
        )
    } else if tonemap && tonemapper == Tonemapper::Zscale {
        format!(
            "scale={w}:{h},zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,\
             tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p"
        )
    } else {
        format!("scale={w}:{h},format=yuv420p")
    };
    if let HwAccel::Vaapi { .. } = accel {
        f.push_str(",format=nv12,hwupload");
    }
    f
}

fn video_encoder(accel: &HwAccel, bitrate: u64) -> Vec<String> {
    let rate = |a: &mut Vec<String>| {
        a.extend([
            "-b:v".into(),
            bitrate.to_string(),
            "-maxrate".into(),
            bitrate.to_string(),
            "-bufsize".into(),
            (bitrate * 2).to_string(),
        ])
    };
    let mut a: Vec<String> = match accel {
        HwAccel::None => ["-c:v", "libx264", "-preset", "veryfast", "-profile:v", "high"].map(String::from).to_vec(),
        HwAccel::Nvenc => {
            ["-c:v", "h264_nvenc", "-preset", "p4", "-profile:v", "high", "-forced-idr", "1"].map(String::from).to_vec()
        }
        HwAccel::Qsv { .. } => {
            ["-c:v", "h264_qsv", "-preset", "veryfast", "-profile:v", "high"].map(String::from).to_vec()
        }
        HwAccel::Vaapi { .. } => ["-c:v", "h264_vaapi", "-profile:v", "high"].map(String::from).to_vec(),
        HwAccel::VideoToolbox => ["-c:v", "h264_videotoolbox", "-profile:v", "high"].map(String::from).to_vec(),
    };
    rate(&mut a);
    a
}

/// A one-second test encode that proves a hardware backend really works on
/// this machine (drivers present, device accessible, encoder licensed).
pub fn encoder_test_args(accel: &HwAccel) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"].map(String::from).to_vec();
    let mut vf = "format=yuv420p".to_string();
    if let HwAccel::Vaapi { device } = accel {
        a.extend(["-init_hw_device".into(), format!("vaapi=va:{device}"), "-filter_hw_device".into(), "va".into()]);
        vf = "format=nv12,hwupload".into();
    }
    a.extend(["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30", "-t", "1", "-vf"].map(String::from));
    a.push(vf);
    a.extend(video_encoder(accel, 4_000_000));
    a.extend(["-f", "null", "-"].map(String::from));
    a
}

/// A short test of a tone mapper on a synthetic 10-bit HDR frame.
/// `vulkan_device` is only used by libplacebo.
pub fn tonemap_test_args(tonemapper: Tonemapper, vulkan_device: Option<u32>) -> Option<Vec<String>> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"].map(String::from).to_vec();
    if tonemapper == Tonemapper::Libplacebo {
        a.extend(["-init_hw_device".into(), vulkan_init(vulkan_device)]);
        a.extend(["-filter_hw_device", "vk"].map(String::from));
    }
    a.extend(["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=10", "-t", "0.5", "-vf"].map(String::from));
    let tm = match tonemapper {
        Tonemapper::None => return None,
        _ => video_filter(640, 360, true, tonemapper, &HwAccel::None),
    };
    a.push(format!("format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc,{tm}"));
    a.extend(["-f", "null", "-"].map(String::from));
    Some(a)
}

/// Extract one subtitle stream as WebVTT (text subtitles only).
pub fn subtitle_vtt_args(input: &str, stream_index: u32) -> Vec<String> {
    [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-i",
        input,
        "-map",
        &format!("0:{stream_index}"),
        "-c:s",
        "webvtt",
        "-f",
        "webvtt",
        "pipe:1",
    ]
    .map(String::from)
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::{Method, PlaybackPlan};

    fn transcode_plan(tonemap: bool) -> PlaybackPlan {
        PlaybackPlan {
            method: Method::Transcode,
            video: Some(VideoAction::Encode {
                codec: "h264".into(),
                width: 1920,
                height: 1080,
                bitrate: 10_000_000,
                tonemap,
            }),
            audio_index: Some(1),
            audio: Some(AudioAction::Encode { codec: "aac".into(), channels: 2, bitrate: 192_000 }),
            reasons: vec![],
        }
    }

    fn remux_plan() -> PlaybackPlan {
        PlaybackPlan {
            method: Method::Remux,
            video: Some(VideoAction::Copy),
            audio_index: Some(1),
            audio: Some(AudioAction::Copy),
            reasons: vec![],
        }
    }

    fn run(plan: &PlaybackPlan, accel: HwAccel, tonemapper: Tonemapper, dv: bool, start: u32) -> String {
        let hw = HwProfile { accel, tonemapper, vulkan_device: None };
        hls_args(&HlsJob {
            input: "/media/Movie (2020)/Movie.mkv",
            plan,
            hw: &hw,
            video_index: Some(0),
            source_is_dolby_vision: dv,
            frame_rate: Some(23.976),
            segment_seconds: 6,
            start_segment: start,
            output_dir: "/tmp/t/abc",
        })
        .join(" ")
    }

    #[test]
    fn remux_copies_streams() {
        insta::assert_snapshot!(run(&remux_plan(), HwAccel::Nvenc, Tonemapper::Libplacebo, false, 0));
    }

    #[test]
    fn software_transcode_from_segment_10() {
        insta::assert_snapshot!(run(&transcode_plan(false), HwAccel::None, Tonemapper::Zscale, false, 10));
    }

    #[test]
    fn nvenc_with_libplacebo_tonemap() {
        insta::assert_snapshot!(run(&transcode_plan(true), HwAccel::Nvenc, Tonemapper::Libplacebo, false, 0));
    }

    #[test]
    fn dolby_vision_decodes_in_software_when_tonemapping() {
        let args = run(&transcode_plan(true), HwAccel::Nvenc, Tonemapper::Libplacebo, true, 0);
        assert!(!args.contains("-hwaccel cuda"), "{args}");
        assert!(args.contains("libplacebo"));
    }

    #[test]
    fn vaapi_uses_cpu_tonemap_and_uploads() {
        insta::assert_snapshot!(run(
            &transcode_plan(true),
            HwAccel::Vaapi { device: "/dev/dri/renderD128".into() },
            Tonemapper::Libplacebo,
            false,
            0
        ));
    }

    #[test]
    fn qsv_linux() {
        insta::assert_snapshot!(run(
            &transcode_plan(false),
            HwAccel::Qsv { device: Some("/dev/dri/renderD128".into()) },
            Tonemapper::Zscale,
            false,
            0
        ));
    }

    #[test]
    fn videotoolbox() {
        insta::assert_snapshot!(run(&transcode_plan(true), HwAccel::VideoToolbox, Tonemapper::Zscale, false, 3));
    }

    #[test]
    fn hardware_test_commands() {
        insta::assert_snapshot!(encoder_test_args(&HwAccel::Vaapi { device: "/dev/dri/renderD128".into() }).join(" "));
        insta::assert_snapshot!(tonemap_test_args(Tonemapper::Libplacebo, None).unwrap().join(" "));
        assert!(tonemap_test_args(Tonemapper::None, None).is_none());
        let args = tonemap_test_args(Tonemapper::Libplacebo, Some(1)).unwrap().join(" ");
        assert!(args.contains("-init_hw_device vulkan=vk:1 "), "{args}");
    }

    #[test]
    fn libplacebo_uses_the_chosen_vulkan_device() {
        let plan = transcode_plan(true);
        let hw = HwProfile { accel: HwAccel::Nvenc, tonemapper: Tonemapper::Libplacebo, vulkan_device: Some(1) };
        let args = hls_args(&HlsJob {
            input: "/m/a.mkv",
            plan: &plan,
            hw: &hw,
            video_index: Some(0),
            source_is_dolby_vision: false,
            frame_rate: None,
            segment_seconds: 6,
            start_segment: 0,
            output_dir: "/tmp/x",
        })
        .join(" ");
        assert!(args.contains("-init_hw_device vulkan=vk:1 -filter_hw_device vk"), "{args}");
    }

    // Real output from an Unraid box where the NVIDIA Vulkan driver wasn't in the container.
    const ONLY_LLVMPIPE: &str = "\
[AVHWDeviceContext @ 0x55e7ee0a2e80] GPU listing:
[AVHWDeviceContext @ 0x55e7ee0a2e80]     0: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)
[AVHWDeviceContext @ 0x55e7ee0a2e80] Device 0 selected: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)
[AVHWDeviceContext @ 0x55e7ee0a2e80] Using device extension VK_KHR_push_descriptor
";

    const NVIDIA_AND_LLVMPIPE: &str = "\
[AVHWDeviceContext @ 0x1] GPU listing:
[AVHWDeviceContext @ 0x1]     0: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)
[AVHWDeviceContext @ 0x1]     1: NVIDIA GeForce RTX 3060 (discrete) (0x2504)
[AVHWDeviceContext @ 0x1] Device 0 selected: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)
";

    #[test]
    fn vulkan_listing_with_only_software_has_no_usable_device() {
        let devices = parse_vulkan_devices(ONLY_LLVMPIPE);
        assert_eq!(devices, vec![VulkanDevice { index: 0, name: "llvmpipe".into(), software: true }]);
        assert_eq!(pick_vulkan_device(&devices), None);
        assert!(parse_vulkan_devices("no listing here").is_empty());
    }

    #[test]
    fn vulkan_listing_picks_the_gpu_over_llvmpipe() {
        let devices = parse_vulkan_devices(NVIDIA_AND_LLVMPIPE);
        assert_eq!(devices.len(), 2);
        let gpu = pick_vulkan_device(&devices).unwrap();
        assert_eq!((gpu.index, gpu.name.as_str()), (1, "NVIDIA GeForce RTX 3060"));
    }

    #[test]
    fn fallback_steps_down_to_software() {
        let hw = HwProfile { accel: HwAccel::Nvenc, tonemapper: Tonemapper::Libplacebo, vulkan_device: Some(0) };
        let step1 = fallback_profile(&hw).unwrap();
        assert_eq!(step1, HwProfile { accel: HwAccel::Nvenc, tonemapper: Tonemapper::Zscale, vulkan_device: None });
        let step2 = fallback_profile(&step1).unwrap();
        assert_eq!(step2.accel, HwAccel::None);
        assert_eq!(step2.tonemapper, Tonemapper::Zscale);
        assert_eq!(fallback_profile(&step2), None);
    }

    #[test]
    fn subtitles() {
        assert_eq!(
            subtitle_vtt_args("/m/a.mkv", 3).join(" "),
            "-hide_banner -loglevel error -nostdin -i /m/a.mkv -map 0:3 -c:s webvtt -f webvtt pipe:1"
        );
    }
}

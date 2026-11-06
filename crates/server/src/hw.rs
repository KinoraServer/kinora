//! Hardware detection: which GPU encoders and tone mappers actually work here.
//!
//! We don't trust what ffmpeg *lists*: a build can include NVENC with no
//! NVIDIA card present. Each candidate runs a one-second test encode, and only
//! backends that succeed are used.

use std::time::Duration;

use kinora_core::ffmpeg::{encoder_test_args, tonemap_test_args, HwAccel, HwProfile, Tonemapper};
use serde::Serialize;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Default)]
pub struct HardwareReport {
    /// What transcodes will use.
    pub profile: HwProfile,
    pub ffmpeg_version: Option<String>,
    pub ffmpeg_found: bool,
    pub tests: Vec<BackendTest>,
    pub detected_at: Option<i64>,
    /// Admin override ("auto" or a backend name).
    pub preference: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendTest {
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
}

async fn run_ffmpeg(ffmpeg: &str, args: &[String], timeout: Duration) -> Result<(), String> {
    let fut = Command::new(ffmpeg).args(args).kill_on_drop(true).output();
    match tokio::time::timeout(timeout, fut).await {
        Err(_) => Err("timed out".into()),
        Ok(Err(e)) => Err(e.to_string()),
        Ok(Ok(out)) if out.status.success() => Ok(()),
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").chars().take(300).collect())
        }
    }
}

fn render_nodes() -> Vec<String> {
    let mut nodes: Vec<String> = std::fs::read_dir("/dev/dri")
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path().display().to_string())
                .filter(|p| p.contains("renderD"))
                .collect()
        })
        .unwrap_or_default();
    nodes.sort();
    nodes
}

/// Candidates in preference order for this OS.
fn candidates() -> Vec<HwAccel> {
    let mut c = Vec::new();
    if cfg!(target_os = "macos") {
        c.push(HwAccel::VideoToolbox);
    } else if cfg!(target_os = "windows") {
        c.push(HwAccel::Nvenc);
        c.push(HwAccel::Qsv { device: None });
    } else {
        c.push(HwAccel::Nvenc);
        for node in render_nodes() {
            c.push(HwAccel::Qsv { device: Some(node.clone()) });
            c.push(HwAccel::Vaapi { device: node });
        }
    }
    c
}

fn backend_name(a: &HwAccel) -> String {
    match a {
        HwAccel::Qsv { device: Some(d) } | HwAccel::Vaapi { device: d } => format!("{} ({d})", a.label()),
        other => other.label().to_string(),
    }
}

fn matches_preference(a: &HwAccel, pref: &str) -> bool {
    matches!(
        (a, pref),
        (HwAccel::Nvenc, "nvenc")
            | (HwAccel::Qsv { .. }, "qsv")
            | (HwAccel::Vaapi { .. }, "vaapi")
            | (HwAccel::VideoToolbox, "videotoolbox")
    )
}

pub async fn detect(ffmpeg: &str, preference: &str) -> HardwareReport {
    let mut report = HardwareReport { preference: preference.to_string(), ..Default::default() };

    let version = Command::new(ffmpeg).arg("-version").output().await;
    match version {
        Ok(out) if out.status.success() => {
            report.ffmpeg_found = true;
            report.ffmpeg_version = String::from_utf8_lossy(&out.stdout).lines().next().map(str::to_string);
        }
        _ => {
            tracing::error!("ffmpeg not found at {ffmpeg:?}: transcoding is disabled");
            report.detected_at = Some(crate::db::now_ms());
            return report;
        }
    }

    let mut chosen = HwAccel::None;
    if preference != "none" {
        for candidate in candidates() {
            if preference != "auto" && !matches_preference(&candidate, preference) {
                continue;
            }
            let result = run_ffmpeg(ffmpeg, &encoder_test_args(&candidate), Duration::from_secs(20)).await;
            let ok = result.is_ok();
            report.tests.push(BackendTest { name: backend_name(&candidate), ok, error: result.err() });
            if ok && chosen == HwAccel::None {
                chosen = candidate;
                break; // first working backend wins; stopping early keeps startup fast
            }
        }
    }

    let mut tonemapper = Tonemapper::None;
    for t in [Tonemapper::Libplacebo, Tonemapper::Zscale] {
        let Some(args) = tonemap_test_args(t) else { continue };
        let result = run_ffmpeg(ffmpeg, &args, Duration::from_secs(20)).await;
        let ok = result.is_ok();
        let name = match t {
            Tonemapper::Libplacebo => "Tone mapping: libplacebo (Vulkan)",
            _ => "Tone mapping: zscale (CPU)",
        };
        report.tests.push(BackendTest { name: name.into(), ok, error: result.err() });
        if ok {
            tonemapper = t;
            break;
        }
    }

    tracing::info!(accel = chosen.label(), ?tonemapper, "hardware detection finished");
    report.profile = HwProfile { accel: chosen, tonemapper };
    report.detected_at = Some(crate::db::now_ms());
    report
}

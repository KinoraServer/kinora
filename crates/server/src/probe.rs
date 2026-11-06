//! Run ffprobe on a file and parse its output with `kinora_core::media`.

use std::time::Duration;

use kinora_core::media::{parse_ffprobe, MediaInfo};
use tokio::process::Command;

pub async fn probe(ffprobe: &str, path: &std::path::Path) -> anyhow::Result<MediaInfo> {
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        Command::new(ffprobe)
            .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
            .arg(path)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ffprobe timed out"))??;
    if !output.status.success() {
        anyhow::bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("unknown error")
        );
    }
    Ok(parse_ffprobe(&String::from_utf8_lossy(&output.stdout))?)
}

//! HLS playlists for transcoded playback.
//!
//! We always hand the player a complete VOD playlist with fixed-length
//! segments, even though segments are only produced on demand. That gives the
//! player a correct duration and seek bar from the start; when it asks for a
//! segment that doesn't exist yet the server (re)starts ffmpeg at that point.
//!
//! Segments are MPEG-TS (`seg_N.ts`) or, when HEVC is copied, fragmented MP4
//! (`seg_N.m4s` plus one `init.mp4` with the codec setup).

use crate::playback::SegmentFormat;

/// Length of every segment except the last.
pub const SEGMENT_SECONDS: u32 = 6;

pub fn segment_count(duration_ms: u64, segment_seconds: u32) -> u32 {
    let seg_ms = u64::from(segment_seconds) * 1000;
    duration_ms.div_ceil(seg_ms).max(1) as u32
}

/// Segment that contains `position_ms`.
pub fn segment_for_position(position_ms: u64, segment_seconds: u32) -> u32 {
    (position_ms / (u64::from(segment_seconds) * 1000)) as u32
}

/// The fMP4 init segment the player is given.
pub const INIT_NAME: &str = "init.mp4";

/// The init segment one ffmpeg run writes (one per run, so a restart never
/// rewrites a file that is being served).
pub fn run_init_name(start_segment: u32) -> String {
    format!("init_{start_segment}.mp4")
}

pub fn segment_name(index: u32, format: SegmentFormat) -> String {
    match format {
        SegmentFormat::MpegTs => format!("seg_{index}.ts"),
        SegmentFormat::Fmp4 => format!("seg_{index}.m4s"),
    }
}

/// Parse "seg_12.ts" or "seg_12.m4s" -> 12.
pub fn parse_segment_name(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("seg_")?;
    rest.strip_suffix(".ts").or_else(|| rest.strip_suffix(".m4s"))?.parse().ok()
}

pub fn vod_playlist(duration_ms: u64, segment_seconds: u32, format: SegmentFormat) -> String {
    let count = segment_count(duration_ms, segment_seconds);
    let seg_ms = u64::from(segment_seconds) * 1000;
    let mut out = String::new();
    // fMP4 segments need playlist version 7 (EXT-X-MAP outside I-frame playlists).
    let version = if format == SegmentFormat::Fmp4 { 7 } else { 3 };
    out.push_str(&format!("#EXTM3U\n#EXT-X-VERSION:{version}\n"));
    out.push_str(&format!("#EXT-X-TARGETDURATION:{segment_seconds}\n"));
    out.push_str("#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-MEDIA-SEQUENCE:0\n");
    if format == SegmentFormat::Fmp4 {
        out.push_str(&format!("#EXT-X-MAP:URI=\"{INIT_NAME}\"\n"));
    }
    for i in 0..count {
        let start = u64::from(i) * seg_ms;
        let len_ms = (duration_ms.saturating_sub(start)).min(seg_ms).max(1);
        out.push_str(&format!("#EXTINF:{:.3},\n{}\n", len_ms as f64 / 1000.0, segment_name(i, format)));
    }
    out.push_str("#EXT-X-ENDLIST\n");
    out
}

/// ffmpeg is paused once it is this many segments ahead of the player
/// (2 minutes). Copying video runs many times faster than real time, and
/// without a limit it would write the whole file out (often into RAM).
pub const MAX_SEGMENTS_AHEAD: u32 = 20;
/// A paused ffmpeg is started again when the player gets this close to the
/// last finished segment (1 minute of buffer left).
pub const RESUME_SEGMENTS_AHEAD: u32 = 10;
/// Finished segments kept behind the player, for short seeks back.
pub const KEEP_SEGMENTS_BEHIND: u32 = 10;

/// What to do with a session's ffmpeg, checked every couple of seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Throttle {
    Keep,
    /// Far enough ahead: stop ffmpeg.
    Pause,
    /// The player is catching up with a paused ffmpeg: start it at this segment.
    Resume(u32),
}

/// `produced` is the highest finished segment of the current (or last) run.
pub fn throttle(running: bool, paused: bool, last_requested: u32, produced: Option<u32>, count: u32) -> Throttle {
    let Some(produced) = produced else { return Throttle::Keep };
    if running && produced >= last_requested + MAX_SEGMENTS_AHEAD {
        return Throttle::Pause;
    }
    let next = produced + 1;
    if !running && paused && next < count && produced < last_requested + RESUME_SEGMENTS_AHEAD {
        return Throttle::Resume(next);
    }
    Throttle::Keep
}

/// Finished segments that can be deleted: well behind the player, or far
/// ahead of it (left over from before a seek back).
pub fn segments_to_delete(existing: &[u32], last_requested: u32) -> Vec<u32> {
    let keep_from = last_requested.saturating_sub(KEEP_SEGMENTS_BEHIND);
    let keep_to = last_requested + MAX_SEGMENTS_AHEAD + RESUME_SEGMENTS_AHEAD;
    existing.iter().copied().filter(|&i| i < keep_from || i > keep_to).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_positions() {
        assert_eq!(segment_count(60_000, 6), 10);
        assert_eq!(segment_count(61_000, 6), 11);
        assert_eq!(segment_count(0, 6), 1);
        assert_eq!(segment_for_position(0, 6), 0);
        assert_eq!(segment_for_position(5_999, 6), 0);
        assert_eq!(segment_for_position(6_000, 6), 1);
    }

    #[test]
    fn names() {
        assert_eq!(parse_segment_name(&segment_name(42, SegmentFormat::MpegTs)), Some(42));
        assert_eq!(parse_segment_name(&segment_name(7, SegmentFormat::Fmp4)), Some(7));
        assert_eq!(parse_segment_name("seg_x.ts"), None);
        assert_eq!(parse_segment_name("seg_3.mp4"), None);
        assert_eq!(parse_segment_name(INIT_NAME), None);
        assert_eq!(parse_segment_name("../etc/passwd"), None);
    }

    #[test]
    fn playlist_snapshot() {
        insta::assert_snapshot!(vod_playlist(14_500, 6, SegmentFormat::MpegTs), @r"
        #EXTM3U
        #EXT-X-VERSION:3
        #EXT-X-TARGETDURATION:6
        #EXT-X-PLAYLIST-TYPE:VOD
        #EXT-X-MEDIA-SEQUENCE:0
        #EXTINF:6.000,
        seg_0.ts
        #EXTINF:6.000,
        seg_1.ts
        #EXTINF:2.500,
        seg_2.ts
        #EXT-X-ENDLIST
        ");
    }

    #[test]
    fn fmp4_playlist_snapshot() {
        insta::assert_snapshot!(vod_playlist(8_000, 6, SegmentFormat::Fmp4), @r#"
        #EXTM3U
        #EXT-X-VERSION:7
        #EXT-X-TARGETDURATION:6
        #EXT-X-PLAYLIST-TYPE:VOD
        #EXT-X-MEDIA-SEQUENCE:0
        #EXT-X-MAP:URI="init.mp4"
        #EXTINF:6.000,
        seg_0.m4s
        #EXTINF:2.000,
        seg_1.m4s
        #EXT-X-ENDLIST
        "#);
    }

    #[test]
    fn throttle_pauses_far_ahead_and_resumes_before_the_buffer_runs_out() {
        // Running, not far enough ahead yet.
        assert_eq!(throttle(true, false, 100, Some(119), 1000), Throttle::Keep);
        assert_eq!(throttle(true, false, 100, Some(120), 1000), Throttle::Pause);
        // Paused, plenty left.
        assert_eq!(throttle(false, true, 105, Some(120), 1000), Throttle::Keep);
        // Paused, the player is within a minute of the end of what's ready.
        assert_eq!(throttle(false, true, 111, Some(120), 1000), Throttle::Resume(121));
        // Not paused by us (crashed, or never started): leave it to the segment handler.
        assert_eq!(throttle(false, false, 111, Some(120), 1000), Throttle::Keep);
        // Everything is done.
        assert_eq!(throttle(false, true, 995, Some(999), 1000), Throttle::Keep);
        assert_eq!(throttle(true, false, 0, None, 1000), Throttle::Keep);
    }

    #[test]
    fn deletes_segments_far_behind_and_far_ahead() {
        let existing: Vec<u32> = (0..200).collect();
        let gone = segments_to_delete(&existing, 100);
        assert_eq!(gone.first(), Some(&0));
        assert!(gone.contains(&89));
        assert!(!gone.contains(&90));
        assert!(!gone.contains(&130));
        assert!(gone.contains(&131));
        assert!(segments_to_delete(&[0, 1, 2], 3).is_empty());
    }
}

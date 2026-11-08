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
}

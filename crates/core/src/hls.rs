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

/// Where every segment starts. Encodes use fixed 6 s segments (ffmpeg
/// places a keyframe at each boundary). Copied video can only be cut at its
/// own keyframes, so its segments start on the source's keyframes, about
/// 6 s apart, taken from the file's index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    starts_ms: Vec<u64>,
    duration_ms: u64,
    keyframes: bool,
}

/// How close (ms) a piece's start must be to a segment boundary to count as
/// starting that segment: covers rounding between container time bases.
const BOUNDARY_TOLERANCE_MS: u64 = 20;

impl Timeline {
    pub fn fixed(duration_ms: u64, segment_seconds: u32) -> Self {
        let seg_ms = u64::from(segment_seconds) * 1000;
        let count = duration_ms.div_ceil(seg_ms).max(1);
        Timeline { starts_ms: (0..count).map(|i| i * seg_ms).collect(), duration_ms, keyframes: false }
    }

    /// Segments that start on keyframes, each at least `segment_seconds`
    /// long (except the last). `None` without keyframes to go on.
    pub fn from_keyframes(keyframes_ms: &[u64], duration_ms: u64, segment_seconds: u32) -> Option<Self> {
        let target = u64::from(segment_seconds) * 1000;
        let mut starts = vec![0u64];
        let mut sorted: Vec<u64> = keyframes_ms.iter().copied().filter(|k| *k < duration_ms).collect();
        sorted.sort_unstable();
        for k in sorted {
            if k >= starts[starts.len() - 1] + target {
                starts.push(k);
            }
        }
        // A single segment would mean the index is missing or useless.
        (starts.len() > 1).then_some(Timeline { starts_ms: starts, duration_ms, keyframes: true })
    }

    pub fn count(&self) -> u32 {
        self.starts_ms.len() as u32
    }

    /// Segments start on the source's keyframes (copied video).
    pub fn keyframe_aligned(&self) -> bool {
        self.keyframes
    }

    pub fn start_ms(&self, index: u32) -> u64 {
        self.starts_ms.get(index as usize).copied().unwrap_or(self.duration_ms)
    }

    pub fn end_ms(&self, index: u32) -> u64 {
        self.starts_ms.get(index as usize + 1).copied().unwrap_or(self.duration_ms).max(self.start_ms(index))
    }

    /// Segment that `time_ms` falls in (a time a few ms before a boundary counts as that boundary).
    pub fn segment_at(&self, time_ms: u64) -> u32 {
        let t = time_ms + BOUNDARY_TOLERANCE_MS;
        (self.starts_ms.partition_point(|s| *s <= t).max(1) - 1) as u32
    }

    /// The whole VOD playlist.
    pub fn playlist(&self, format: SegmentFormat) -> String {
        let longest = (0..self.count()).map(|i| self.end_ms(i) - self.start_ms(i)).max().unwrap_or(0);
        let mut out = String::new();
        // fMP4 segments need playlist version 7 (EXT-X-MAP outside I-frame playlists).
        let version = if format == SegmentFormat::Fmp4 { 7 } else { 3 };
        out.push_str(&format!("#EXTM3U\n#EXT-X-VERSION:{version}\n"));
        out.push_str(&format!("#EXT-X-TARGETDURATION:{}\n", longest.div_ceil(1000).max(1)));
        out.push_str("#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-MEDIA-SEQUENCE:0\n");
        if format == SegmentFormat::Fmp4 {
            out.push_str(&format!("#EXT-X-MAP:URI=\"{INIT_NAME}\"\n"));
        }
        for i in 0..self.count() {
            let len_ms = (self.end_ms(i) - self.start_ms(i)).max(1);
            out.push_str(&format!("#EXTINF:{:.3},\n{}\n", len_ms as f64 / 1000.0, segment_name(i, format)));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }
}

/// One piece ffmpeg wrote when cutting copied video at every keyframe:
/// `gop_<run>_<n>.m4s`, where `run` is the segment that ffmpeg run started at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub run: u32,
    pub start_ms: u64,
    pub name: String,
}

pub fn piece_pattern(run: u32) -> String {
    format!("gop_{run}_%d.m4s")
}

/// Parse "gop_12_3.m4s" -> (run 12, piece 3).
pub fn parse_piece_name(name: &str) -> Option<(u32, u32)> {
    let (run, n) = name.strip_prefix("gop_")?.strip_suffix(".m4s")?.split_once('_')?;
    Some((run.parse().ok()?, n.parse().ok()?))
}

/// Segments that one run's pieces now complete, with their pieces in order.
/// A segment is complete once the run has a piece starting in a later
/// segment, or the run reached the end of the file (`finished`).
pub fn complete_segments(timeline: &Timeline, pieces: &[Piece], run: u32, finished: bool) -> Vec<(u32, Vec<String>)> {
    let mut mine: Vec<&Piece> = pieces.iter().filter(|p| p.run == run).collect();
    mine.sort_by_key(|p| p.start_ms);
    let Some(last) = mine.last() else { return Vec::new() };
    let last_segment = timeline.segment_at(last.start_ms);
    let mut out: Vec<(u32, Vec<String>)> = Vec::new();
    for p in &mine {
        let seg = timeline.segment_at(p.start_ms);
        if seg == last_segment && !finished {
            break;
        }
        match out.last_mut() {
            Some((s, names)) if *s == seg => names.push(p.name.clone()),
            _ => out.push((seg, vec![p.name.clone()])),
        }
    }
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

/// How far a run has got: the end of the unbroken stretch of finished
/// segments from where it started. `through` is the previous answer, so
/// segments the player has left behind (and that were deleted) still count.
/// Files further on from an earlier run only count once the stretch reaches
/// them, so a restarted run isn't taken to be far ahead.
pub fn progress(existing: &[u32], started_at: u32, through: Option<u32>) -> Option<u32> {
    let have: std::collections::HashSet<u32> = existing.iter().copied().collect();
    let mut end = through;
    let mut next = through.map_or(Some(started_at), |t| t.checked_add(1));
    while let Some(n) = next.filter(|n| have.contains(n)) {
        end = Some(n);
        next = n.checked_add(1);
    }
    end
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
        assert_eq!(Timeline::fixed(60_000, 6).count(), 10);
        assert_eq!(Timeline::fixed(61_000, 6).count(), 11);
        assert_eq!(Timeline::fixed(0, 6).count(), 1);
        let t = Timeline::fixed(60_000, 6);
        assert_eq!(t.segment_at(0), 0);
        assert_eq!(t.segment_at(5_900), 0);
        assert_eq!(t.segment_at(6_000), 1);
        assert_eq!((t.start_ms(9), t.end_ms(9)), (54_000, 60_000));
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
        insta::assert_snapshot!(Timeline::fixed(14_500, 6).playlist(SegmentFormat::MpegTs), @r"
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
        insta::assert_snapshot!(Timeline::fixed(8_000, 6).playlist(SegmentFormat::Fmp4), @r#"
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

    #[test]
    fn keyframe_timeline_starts_segments_on_keyframes() {
        // Keyframes every 2.002 s (23.976 fps, GOP 48), plus a scene cut at 7.5 s.
        let mut kf: Vec<u64> = (0..30).map(|i| i * 2002).collect();
        kf.push(7_500);
        let t = Timeline::from_keyframes(&kf, 60_000, 6).unwrap();
        assert!(t.keyframe_aligned());
        assert_eq!(&t.starts_ms[..4], &[0, 6006, 12012, 18018]);
        assert_eq!(t.end_ms(t.count() - 1), 60_000);
        assert_eq!(t.segment_at(6_000), 1, "a piece starting 6 ms early still starts segment 1");
        assert_eq!(t.segment_at(7_500), 1);
        assert_eq!(t.segment_at(12_011), 2);
        assert!(Timeline::from_keyframes(&[0], 60_000, 6).is_none());
        assert!(Timeline::from_keyframes(&[], 60_000, 6).is_none());
    }

    #[test]
    fn keyframe_playlist_uses_real_lengths() {
        let t = Timeline::from_keyframes(&[0, 5005, 10010, 15015, 20020], 22_000, 6).unwrap();
        insta::assert_snapshot!(t.playlist(SegmentFormat::Fmp4), @r#"
        #EXTM3U
        #EXT-X-VERSION:7
        #EXT-X-TARGETDURATION:11
        #EXT-X-PLAYLIST-TYPE:VOD
        #EXT-X-MEDIA-SEQUENCE:0
        #EXT-X-MAP:URI="init.mp4"
        #EXTINF:10.010,
        seg_0.m4s
        #EXTINF:10.010,
        seg_1.m4s
        #EXTINF:1.980,
        seg_2.m4s
        #EXT-X-ENDLIST
        "#);
    }

    #[test]
    fn pieces_complete_a_segment_once_the_next_one_starts() {
        let t = Timeline::from_keyframes(&(0..20).map(|i| i * 2000).collect::<Vec<_>>(), 40_000, 6).unwrap();
        let piece = |run: u32, start_ms: u64| Piece { run, start_ms, name: format!("gop_{run}_{start_ms}.m4s") };
        // Run 1 started at 6 s; an old run 0 left a piece behind.
        let pieces =
            vec![piece(1, 6000), piece(1, 8000), piece(1, 10000), piece(1, 12000), piece(1, 14000), piece(0, 2000)];
        assert_eq!(
            complete_segments(&t, &pieces, 1, false),
            vec![(1, vec!["gop_1_6000.m4s".into(), "gop_1_8000.m4s".into(), "gop_1_10000.m4s".into()])]
        );
        // At the end of the file the last segment is complete too.
        let done = complete_segments(&t, &pieces, 1, true);
        assert_eq!(done.len(), 2);
        assert_eq!(done[1], (2, vec!["gop_1_12000.m4s".into(), "gop_1_14000.m4s".into()]));
        assert!(complete_segments(&t, &pieces, 7, false).is_empty());
        assert_eq!(parse_piece_name("gop_12_3.m4s"), Some((12, 3)));
        assert_eq!(parse_piece_name("seg_3.m4s"), None);
    }

    #[test]
    fn progress_stops_at_the_first_gap() {
        // A run restarted at 3 has made 3 and 4; 9.. are left from an earlier run.
        let existing = [9, 3, 10, 4, 11];
        assert_eq!(progress(&existing, 3, None), Some(4));
        assert_eq!(progress(&existing, 5, None), None);
        assert_eq!(progress(&[], 0, None), None);
        // Once it reaches the old files, they count.
        assert_eq!(progress(&[5, 6, 7, 8, 9, 10, 11], 3, Some(4)), Some(11));
    }

    #[test]
    fn progress_keeps_segments_deleted_behind_the_player() {
        assert_eq!(progress(&[7, 8], 0, Some(6)), Some(8));
        assert_eq!(progress(&[], 0, Some(6)), Some(6));
    }
}

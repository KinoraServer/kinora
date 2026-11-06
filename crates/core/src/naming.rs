//! Turn file and folder names into titles, years, seasons and episodes.
//!
//! Examples of what we handle:
//! - `Movies/Inception (2010)/Inception (2010) [1080p].mkv` -> "Inception", 2010
//! - `Movies/The.Matrix.1999.2160p.UHD.BluRay.x265.mkv` -> "The Matrix", 1999
//! - `TV/Show Name/Season 01/Show Name - S01E02 - Pilot.mkv` -> show "Show Name", S1 E2
//! - `TV/Show Name (2019)/Show.Name.1x05.720p.mkv` -> show "Show Name", S1 E5

use std::path::{Component, Path};

/// File extensions we treat as video.
pub const VIDEO_EXTENSIONS: &[&str] =
    &["mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "m2ts", "mts", "wmv", "flv", "mpg", "mpeg", "ogv"];

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Words that mark the end of the title part of a release name.
const STOP_WORDS: &[&str] = &[
    "2160p", "1080p", "1080i", "720p", "576p", "480p", "4k", "uhd", "hdr", "hdr10", "dv", "dovi", "bluray", "blu-ray",
    "bdrip", "brrip", "webrip", "web-dl", "webdl", "web", "hdtv", "dvdrip", "remux", "x264", "x265", "h264", "h265",
    "hevc", "av1", "aac", "ac3", "dts", "truehd", "atmos", "proper", "repack", "extended", "unrated", "imax", "10bit",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieName {
    pub title: String,
    pub year: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeName {
    pub show: String,
    pub show_year: Option<u16>,
    pub season: u32,
    pub episode: u32,
    /// Last episode for multi-episode files like `S01E01E02` or `S01E01-E02`.
    pub episode_end: Option<u32>,
    pub title: Option<String>,
}

/// Parse a movie from its path. Prefers the file name, falls back to the
/// parent folder when the file name has no year (e.g. `Inception (2010)/movie.mkv`).
pub fn parse_movie(path: &Path) -> MovieName {
    let stem = file_stem(path);
    let from_file = parse_title_year(&stem);
    if from_file.year.is_some() {
        return from_file;
    }
    if let Some(parent) = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
        let from_folder = parse_title_year(parent);
        if from_folder.year.is_some() {
            return from_folder;
        }
    }
    from_file
}

/// Parse a TV episode from its path relative to the library root.
/// Returns `None` if no season/episode marker is found.
pub fn parse_episode(relative_path: &Path) -> Option<EpisodeName> {
    let stem = file_stem(relative_path);
    let (season, episode, episode_end, before, after) = find_episode_marker(&stem).or_else(|| {
        // "Season 2/05 - Title.mkv" style: episode number only in file name.
        let season = relative_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .and_then(parse_season_folder)?;
        let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
        let ep: u32 = digits.parse().ok()?;
        let rest = stem[digits.len()..].to_string();
        Some((season, ep, None, String::new(), rest))
    })?;

    // The show name is the first folder of the relative path when there is
    // one (the usual "Show/Season 1/file" layout), otherwise the file name
    // part before the marker.
    let components: Vec<String> = relative_path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str().map(str::to_string),
            _ => None,
        })
        .collect();
    let show_source = if components.len() >= 2 { components[0].clone() } else { before.clone() };
    let show = parse_title_year(&show_source);
    let show = if show.title.is_empty() { parse_title_year(&before) } else { show };

    let title = clean_episode_title(&after);

    Some(EpisodeName { show: show.title, show_year: show.year, season, episode, episode_end, title })
}

/// "Season 01", "Season 1", "S01", "Specials" -> season number.
pub fn parse_season_folder(name: &str) -> Option<u32> {
    let lower = name.trim().to_ascii_lowercase();
    if lower == "specials" {
        return Some(0);
    }
    let rest =
        lower.strip_prefix("season").or_else(|| lower.strip_prefix('s'))?.trim_start_matches([' ', '.', '_', '-']);
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn file_stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string()
}

/// Split "The.Matrix.1999.2160p" / "Inception (2010) [1080p]" into title + year.
pub fn parse_title_year(raw: &str) -> MovieName {
    // Drop anything in square brackets: [1080p], [tmdbid-123], etc.
    let mut cleaned = String::with_capacity(raw.len());
    let mut depth = 0;
    for c in raw.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => cleaned.push(c),
            _ => {}
        }
    }

    let tokens: Vec<String> = cleaned.split(['.', '_', ' ']).filter(|t| !t.is_empty()).map(str::to_string).collect();

    let mut title_tokens: Vec<String> = Vec::new();
    let mut year = None;
    for (i, token) in tokens.iter().enumerate() {
        let bare = token.trim_matches(|c| c == '(' || c == ')');
        if let Some(y) = as_year(bare) {
            // A year as the very first token is probably part of the title ("1917", "2012").
            if i > 0 {
                year = Some(y);
                break;
            }
        }
        if STOP_WORDS.contains(&bare.to_ascii_lowercase().as_str()) && i > 0 {
            break;
        }
        title_tokens.push(token.clone());
    }

    let title = title_tokens.join(" ").trim_matches(|c: char| c == '-' || c == ' ' || c == '(').trim().to_string();
    MovieName { title, year }
}

fn as_year(s: &str) -> Option<u16> {
    if s.len() != 4 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let y: u16 = s.parse().ok()?;
    (1880..=2100).contains(&y).then_some(y)
}

/// Finds `S01E02`, `s1e2`, `S01E01E02`, `S01E01-E02` or `1x02`.
/// Returns (season, episode, episode_end, text_before, text_after).
fn find_episode_marker(stem: &str) -> Option<(u32, u32, Option<u32>, String, String)> {
    let bytes = stem.as_bytes();
    let lower = stem.to_ascii_lowercase();
    let lb = lower.as_bytes();

    // SxxEyy
    for i in 0..lb.len() {
        if lb[i] != b's' || (i > 0 && lb[i - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let (season, after_s) = read_number(lb, i + 1);
        let Some(season) = season else { continue };
        if after_s >= lb.len() || lb[after_s] != b'e' {
            continue;
        }
        let (episode, mut end) = read_number(lb, after_s + 1);
        let Some(episode) = episode else { continue };
        let mut episode_end = None;
        // Multi-episode: E02 or -E02 directly after.
        loop {
            let mut j = end;
            if j < lb.len() && lb[j] == b'-' {
                j += 1;
            }
            if j < lb.len() && lb[j] == b'e' {
                if let (Some(n), next) = read_number(lb, j + 1) {
                    episode_end = Some(n);
                    end = next;
                    continue;
                }
            }
            break;
        }
        let before = String::from_utf8_lossy(&bytes[..i]).to_string();
        let after = String::from_utf8_lossy(&bytes[end..]).to_string();
        return Some((season, episode, episode_end, before, after));
    }

    // 1x02
    for i in 0..lb.len() {
        if lb[i] != b'x' || i == 0 || !lb[i - 1].is_ascii_digit() {
            continue;
        }
        let mut start = i;
        while start > 0 && lb[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start > 0 && lb[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        let season: Option<u32> = stem[start..i].parse().ok();
        let (episode, end) = read_number(lb, i + 1);
        if let (Some(season), Some(episode)) = (season, episode) {
            if end < lb.len() && lb[end].is_ascii_alphanumeric() {
                continue; // e.g. "1920x1080p"
            }
            if i - start > 2 {
                continue; // resolution like 1920x1080
            }
            let before = String::from_utf8_lossy(&bytes[..start]).to_string();
            let after = String::from_utf8_lossy(&bytes[end..]).to_string();
            return Some((season, episode, None, before, after));
        }
    }
    None
}

fn read_number(bytes: &[u8], start: usize) -> (Option<u32>, usize) {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() && end - start < 4 {
        end += 1;
    }
    if end == start {
        return (None, start);
    }
    let n = std::str::from_utf8(&bytes[start..end]).ok().and_then(|s| s.parse().ok());
    (n, end)
}

fn clean_episode_title(after: &str) -> Option<String> {
    let trimmed = after.trim_start_matches([' ', '-', '.', '_']);
    let first = trimmed.split(['.', '_', ' ']).next().unwrap_or("").to_ascii_lowercase();
    if STOP_WORDS.contains(&first.as_str()) {
        return None; // "S01E02.1080p.WEB-DL": no title, just release info
    }
    let t = parse_title_year(trimmed);
    let title = t.title.trim().to_string();
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn movie(p: &str) -> MovieName {
        parse_movie(&PathBuf::from(p))
    }
    fn ep(p: &str) -> EpisodeName {
        parse_episode(&PathBuf::from(p)).expect("should parse")
    }

    #[test]
    fn movie_with_year_in_parens() {
        assert_eq!(
            movie("Inception (2010)/Inception (2010) [1080p].mkv"),
            MovieName { title: "Inception".into(), year: Some(2010) }
        );
    }

    #[test]
    fn scene_style_movie() {
        assert_eq!(
            movie("The.Matrix.1999.2160p.UHD.BluRay.x265.mkv"),
            MovieName { title: "The Matrix".into(), year: Some(1999) }
        );
    }

    #[test]
    fn year_only_in_folder() {
        assert_eq!(movie("Arrival (2016)/movie.mkv"), MovieName { title: "Arrival".into(), year: Some(2016) });
    }

    #[test]
    fn title_that_is_a_year() {
        assert_eq!(movie("1917 (2019).mkv"), MovieName { title: "1917".into(), year: Some(2019) });
    }

    #[test]
    fn movie_without_year() {
        assert_eq!(movie("Some Home Video.mp4"), MovieName { title: "Some Home Video".into(), year: None });
    }

    #[test]
    fn standard_episode_layout() {
        let e = ep("Show Name/Season 01/Show Name - S01E02 - Pilot.mkv");
        assert_eq!(e.show, "Show Name");
        assert_eq!((e.season, e.episode), (1, 2));
        assert_eq!(e.title.as_deref(), Some("Pilot"));
    }

    #[test]
    fn scene_episode_no_folders() {
        let e = ep("Show.Name.S03E10.1080p.WEB-DL.mkv");
        assert_eq!(e.show, "Show Name");
        assert_eq!((e.season, e.episode), (3, 10));
        assert_eq!(e.title, None);
    }

    #[test]
    fn x_style_episode() {
        let e = ep("Show Name (2019)/Show.Name.1x05.720p.mkv");
        assert_eq!(e.show, "Show Name");
        assert_eq!(e.show_year, Some(2019));
        assert_eq!((e.season, e.episode), (1, 5));
    }

    #[test]
    fn multi_episode() {
        let e = ep("Show/Season 1/Show - S01E01-E02.mkv");
        assert_eq!((e.season, e.episode, e.episode_end), (1, 1, Some(2)));
        let e = ep("Show/Season 1/Show S01E03E04.mkv");
        assert_eq!((e.episode, e.episode_end), (3, Some(4)));
    }

    #[test]
    fn episode_number_only_in_season_folder() {
        let e = ep("Show/Season 2/05 - The Title.mkv");
        assert_eq!(e.show, "Show");
        assert_eq!((e.season, e.episode), (2, 5));
        assert_eq!(e.title.as_deref(), Some("The Title"));
    }

    #[test]
    fn resolution_is_not_an_episode() {
        assert!(parse_episode(&PathBuf::from("Movie 1920x1080.mkv")).is_none());
    }

    #[test]
    fn season_folders() {
        assert_eq!(parse_season_folder("Season 01"), Some(1));
        assert_eq!(parse_season_folder("season 12"), Some(12));
        assert_eq!(parse_season_folder("S3"), Some(3));
        assert_eq!(parse_season_folder("Specials"), Some(0));
        assert_eq!(parse_season_folder("Extras"), None);
    }

    #[test]
    fn video_extensions() {
        assert!(is_video_file(&PathBuf::from("a/b.MKV")));
        assert!(!is_video_file(&PathBuf::from("a/b.srt")));
        assert!(!is_video_file(&PathBuf::from("a/b")));
    }
}

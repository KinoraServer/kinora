//! Read the video keyframe times from a Matroska file's index (Cues).
//!
//! Copied video can only be cut at keyframes, so the HLS playlist for it has
//! to be built from the real keyframe times. Matroska files carry them in
//! the Cues element (mkvmerge and ffmpeg both index every video keyframe),
//! which takes a few small reads to find, even in a 20 GB file. Reading the
//! keyframes from the packets instead would mean reading the whole file.
//!
//! This only reads; it works on anything `Read + Seek` so tests can use a
//! byte buffer.

use std::io::{self, Read, Seek, SeekFrom};

const EBML_HEADER: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const SEEK_HEAD: u32 = 0x114D_9B74;
const SEEK: u32 = 0x4DBB;
const SEEK_ID: u32 = 0x53AB;
const SEEK_POSITION: u32 = 0x53AC;
const INFO: u32 = 0x1549_A966;
const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const TRACK_TYPE: u32 = 0x83;
const CUES: u32 = 0x1C53_BB6B;
const CUE_POINT: u32 = 0xBB;
const CUE_TIME: u32 = 0xB3;
const CUE_TRACK_POSITIONS: u32 = 0xB7;
const CUE_TRACK: u32 = 0xF7;

/// Elements bigger than this aren't read into memory (the Cues of a long
/// film with a keyframe every second are a few hundred KB).
const MAX_ELEMENT: u64 = 64 * 1024 * 1024;

/// An element header: id, data size (None = unknown size) and where its data starts.
struct Header {
    id: u32,
    size: Option<u64>,
    data_start: u64,
}

fn read_vint<R: Read>(r: &mut R, keep_marker: bool) -> io::Result<(u64, usize, bool)> {
    let mut first = [0u8; 1];
    r.read_exact(&mut first)?;
    let len = first[0].leading_zeros() as usize + 1;
    if len > 8 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad EBML length"));
    }
    let mask = (0xFFu16 >> len) as u8;
    let mut value = u64::from(if keep_marker { first[0] } else { first[0] & mask });
    let mut all_ones = !keep_marker && first[0] & mask == mask;
    for _ in 1..len {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        all_ones &= b[0] == 0xFF;
        value = (value << 8) | u64::from(b[0]);
    }
    Ok((value, len, all_ones))
}

fn read_header<R: Read + Seek>(r: &mut R) -> io::Result<Header> {
    let (id, _, _) = read_vint(r, true)?;
    let (size, _, unknown) = read_vint(r, false)?;
    Ok(Header { id: id as u32, size: (!unknown).then_some(size), data_start: r.stream_position()? })
}

/// Children of an element held in memory: (id, data).
fn children(data: &[u8]) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    let mut cur = io::Cursor::new(data);
    while (cur.position() as usize) < data.len() {
        let Ok((id, _, _)) = read_vint(&mut cur, true) else { break };
        let Ok((size, _, unknown)) = read_vint(&mut cur, false) else { break };
        let start = cur.position() as usize;
        if unknown {
            break;
        }
        let Some(end) = start.checked_add(size as usize).filter(|e| *e <= data.len()) else { break };
        out.push((id as u32, &data[start..end]));
        cur.set_position(end as u64);
    }
    out
}

fn uint(data: &[u8]) -> u64 {
    data.iter().take(8).fold(0, |acc, b| (acc << 8) | u64::from(*b))
}

fn read_data<R: Read + Seek>(r: &mut R, h: &Header) -> io::Result<Vec<u8>> {
    let size = h.size.filter(|s| *s <= MAX_ELEMENT).ok_or_else(|| io::Error::other("element too large"))?;
    r.seek(SeekFrom::Start(h.data_start))?;
    let mut buf = vec![0u8; size as usize];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Presentation times (milliseconds) of every indexed keyframe of the first
/// video track, sorted. `None` when the file isn't Matroska, has no Cues, or
/// the Cues don't cover a video track.
pub fn video_keyframes_ms<R: Read + Seek>(r: &mut R) -> Option<Vec<u64>> {
    r.seek(SeekFrom::Start(0)).ok()?;
    let ebml = read_header(r).ok()?;
    if ebml.id != EBML_HEADER {
        return None;
    }
    r.seek(SeekFrom::Start(ebml.data_start + ebml.size?)).ok()?;
    let segment = read_header(r).ok()?;
    if segment.id != SEGMENT {
        return None;
    }
    let seg_start = segment.data_start;
    let seg_end = segment.size.map(|s| seg_start + s);

    // Find Info, Tracks and Cues: through the SeekHead when there is one,
    // otherwise by walking the top-level elements (skipping clusters by size).
    let mut info = None;
    let mut tracks = None;
    let mut cues = None;
    let mut pos = seg_start;
    let mut walked = 0;
    while seg_end.is_none_or(|end| pos < end) && walked < 100_000 {
        walked += 1;
        if r.seek(SeekFrom::Start(pos)).is_err() {
            break;
        }
        let Ok(h) = read_header(r) else { break };
        match h.id {
            SEEK_HEAD => {
                let data = read_data(r, &h).ok()?;
                for (id, seek) in children(&data) {
                    if id != SEEK {
                        continue;
                    }
                    let fields = children(seek);
                    let target = fields.iter().find(|(i, _)| *i == SEEK_ID).map(|(_, d)| uint(d) as u32);
                    let at = fields.iter().find(|(i, _)| *i == SEEK_POSITION).map(|(_, d)| seg_start + uint(d));
                    match (target, at) {
                        (Some(INFO), Some(at)) => info = info.or(Some(at)),
                        (Some(TRACKS), Some(at)) => tracks = tracks.or(Some(at)),
                        (Some(CUES), Some(at)) => cues = cues.or(Some(at)),
                        _ => {}
                    }
                }
            }
            INFO => info = info.or(Some(pos)),
            TRACKS => tracks = tracks.or(Some(pos)),
            CUES => cues = cues.or(Some(pos)),
            _ => {}
        }
        if info.is_some() && tracks.is_some() && cues.is_some() {
            break;
        }
        // Clusters are skipped by size (the Cues usually come after them);
        // unknown-size clusters (live recordings) can't be skipped.
        let Some(size) = h.size else { break };
        pos = h.data_start + size;
    }

    let element_at = |r: &mut R, at: u64, want: u32| -> Option<Vec<u8>> {
        r.seek(SeekFrom::Start(at)).ok()?;
        let h = read_header(r).ok()?;
        (h.id == want).then_some(())?;
        read_data(r, &h).ok()
    };

    let scale_ns = info
        .and_then(|at| element_at(r, at, INFO))
        .and_then(|d| children(&d).into_iter().find(|(id, _)| *id == TIMESTAMP_SCALE).map(|(_, v)| uint(v)))
        .filter(|s| *s > 0)
        .unwrap_or(1_000_000);

    let tracks = element_at(r, tracks?, TRACKS)?;
    let video_track = children(&tracks).into_iter().filter(|(id, _)| *id == TRACK_ENTRY).find_map(|(_, entry)| {
        let fields = children(entry);
        let kind = fields.iter().find(|(id, _)| *id == TRACK_TYPE).map(|(_, d)| uint(d));
        let number = fields.iter().find(|(id, _)| *id == TRACK_NUMBER).map(|(_, d)| uint(d));
        (kind == Some(1)).then_some(number).flatten()
    })?;

    let cues = element_at(r, cues?, CUES)?;
    let mut times: Vec<u64> = children(&cues)
        .into_iter()
        .filter(|(id, _)| *id == CUE_POINT)
        .filter_map(|(_, point)| {
            let fields = children(point);
            let time = fields.iter().find(|(id, _)| *id == CUE_TIME).map(|(_, d)| uint(d))?;
            let for_video = fields
                .iter()
                .filter(|(id, _)| *id == CUE_TRACK_POSITIONS)
                .any(|(_, pos)| children(pos).iter().any(|(id, d)| *id == CUE_TRACK && uint(d) == video_track));
            for_video.then(|| (u128::from(time) * u128::from(scale_ns) / 1_000_000) as u64)
        })
        .collect();
    times.sort_unstable();
    times.dedup();
    (!times.is_empty()).then_some(times)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal EBML writer for building test files.
    fn el(id: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let id_bytes = id.to_be_bytes();
        let skip = id_bytes.iter().position(|b| *b != 0).unwrap_or(3);
        out.extend_from_slice(&id_bytes[skip..]);
        // 8-byte size: 0x01 marker then 7 bytes.
        let size = data.len() as u64;
        out.push(0x01);
        out.extend_from_slice(&size.to_be_bytes()[1..]);
        out.extend_from_slice(data);
        out
    }

    fn uint_el(id: u32, v: u64) -> Vec<u8> {
        el(id, &v.to_be_bytes())
    }

    fn cue(time: u64, track: u64) -> Vec<u8> {
        let mut p = uint_el(CUE_TIME, time);
        p.extend(el(CUE_TRACK_POSITIONS, &uint_el(CUE_TRACK, track)));
        el(CUE_POINT, &p)
    }

    /// EBML header, then a Segment holding Info, Tracks (audio 1, video 2), a
    /// dummy Cluster and the Cues, optionally found through a SeekHead.
    fn file(with_seek_head: bool, scale: Option<u64>) -> Vec<u8> {
        let info = el(INFO, &scale.map(|s| uint_el(TIMESTAMP_SCALE, s)).unwrap_or_default());
        let mut entries = el(TRACK_ENTRY, &[uint_el(TRACK_NUMBER, 1), uint_el(TRACK_TYPE, 2)].concat());
        entries.extend(el(TRACK_ENTRY, &[uint_el(TRACK_NUMBER, 2), uint_el(TRACK_TYPE, 1)].concat()));
        let tracks = el(TRACKS, &entries);
        let cluster = el(0x1F43_B675, &[0u8; 1000]);
        let cues = el(CUES, &[cue(4000, 2), cue(0, 2), cue(2000, 2), cue(1000, 1), cue(6000, 2)].concat());

        let mut body = Vec::new();
        if with_seek_head {
            // Positions are relative to the segment data; the SeekHead itself
            // has a fixed size, so compute it with placeholder positions first.
            let seek =
                |id: u32, pos: u64| el(SEEK, &[el(SEEK_ID, &id.to_be_bytes()), uint_el(SEEK_POSITION, pos)].concat());
            let head_len = el(SEEK_HEAD, &[seek(INFO, 0), seek(TRACKS, 0), seek(CUES, 0)].concat()).len() as u64;
            let info_at = head_len;
            let tracks_at = info_at + info.len() as u64;
            let cues_at = tracks_at + tracks.len() as u64 + cluster.len() as u64;
            body.extend(el(SEEK_HEAD, &[seek(INFO, info_at), seek(TRACKS, tracks_at), seek(CUES, cues_at)].concat()));
        }
        body.extend(&info);
        body.extend(&tracks);
        body.extend(&cluster);
        body.extend(&cues);
        let mut out = el(EBML_HEADER, &[0u8; 4]);
        out.extend(el(SEGMENT, &body));
        out
    }

    #[test]
    fn reads_video_cues_through_the_seek_head() {
        let mut f = io::Cursor::new(file(true, None));
        assert_eq!(video_keyframes_ms(&mut f), Some(vec![0, 2000, 4000, 6000]));
    }

    #[test]
    fn reads_video_cues_without_a_seek_head() {
        let mut f = io::Cursor::new(file(false, None));
        assert_eq!(video_keyframes_ms(&mut f), Some(vec![0, 2000, 4000, 6000]));
    }

    #[test]
    fn applies_the_timestamp_scale() {
        // 100 µs ticks: cue time 4000 is 400 ms.
        let mut f = io::Cursor::new(file(true, Some(100_000)));
        assert_eq!(video_keyframes_ms(&mut f), Some(vec![0, 200, 400, 600]));
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(video_keyframes_ms(&mut io::Cursor::new(b"not a matroska file at all".to_vec())), None);
        assert_eq!(video_keyframes_ms(&mut io::Cursor::new(Vec::new())), None);
    }
}

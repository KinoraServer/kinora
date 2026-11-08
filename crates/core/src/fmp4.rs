//! Just enough fragmented-MP4 parsing to tell when a media segment starts.
//!
//! ffmpeg cuts copied video at every keyframe; the server groups those small
//! pieces into the playlist's segments by their start time, read here from
//! the `moof` (decode time + the first sample's composition offset, minus
//! the edit list shift from the init segment).

/// What we need from the init segment about the video track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoTrack {
    pub track_id: u32,
    pub timescale: u32,
    /// Edit list `media_time` (track timescale): ffmpeg shifts decode times
    /// by the B-frame delay and records the shift here.
    pub edit_shift: i64,
}

fn boxes(data: &[u8]) -> impl Iterator<Item = (&[u8; 4], &[u8])> {
    let mut i = 0usize;
    std::iter::from_fn(move || {
        if i + 8 > data.len() {
            return None;
        }
        let size32 = u32::from_be_bytes(data[i..i + 4].try_into().ok()?) as usize;
        let kind: &[u8; 4] = data[i + 4..i + 8].try_into().ok()?;
        let (header, size) = match size32 {
            0 => (8, data.len() - i),
            1 => (16, u64::from_be_bytes(data.get(i + 8..i + 16)?.try_into().ok()?) as usize),
            n => (8, n),
        };
        if size < header || i + size > data.len() {
            return None;
        }
        let body = &data[i + header..i + size];
        i += size;
        Some((kind, body))
    })
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).find(|(k, _)| *k == kind).map(|(_, b)| b)
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn be64(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// The video track of an init segment (`ftyp` + `moov`).
pub fn video_track(init: &[u8]) -> Option<VideoTrack> {
    let moov = child(init, b"moov")?;
    boxes(moov).filter(|(k, _)| *k == b"trak").find_map(|(_, trak)| {
        let mdia = child(trak, b"mdia")?;
        let hdlr = child(mdia, b"hdlr")?;
        if hdlr.get(8..12)? != b"vide" {
            return None;
        }
        let tkhd = child(trak, b"tkhd")?;
        let track_id = if tkhd[0] == 1 { be32(tkhd, 20)? } else { be32(tkhd, 12)? };
        let mdhd = child(mdia, b"mdhd")?;
        let timescale = if mdhd[0] == 1 { be32(mdhd, 20)? } else { be32(mdhd, 12)? };
        let edit_shift = child(trak, b"edts")
            .and_then(|edts| child(edts, b"elst"))
            .and_then(|elst| {
                // The first entry that isn't an empty edit (media_time -1).
                let count = be32(elst, 4)? as usize;
                (0..count).find_map(|n| {
                    let t = if elst[0] == 1 {
                        be64(elst, 8 + n * 20 + 8)? as i64
                    } else {
                        i64::from(be32(elst, 8 + n * 12 + 4)? as i32)
                    };
                    (t >= 0).then_some(t)
                })
            })
            .unwrap_or(0);
        Some(VideoTrack { track_id, timescale, edit_shift })
    })
}

/// Presentation time (milliseconds) of the first video sample in a media
/// segment (one or more `moof` + `mdat`).
pub fn first_video_time_ms(segment: &[u8], track: &VideoTrack) -> Option<u64> {
    boxes(segment).filter(|(k, _)| *k == b"moof").find_map(|(_, moof)| {
        boxes(moof).filter(|(k, _)| *k == b"traf").find_map(|(_, traf)| {
            let tfhd = child(traf, b"tfhd")?;
            if be32(tfhd, 4)? != track.track_id {
                return None;
            }
            let tfdt = child(traf, b"tfdt")?;
            let decode = if tfdt[0] == 1 { be64(tfdt, 4)? as i64 } else { i64::from(be32(tfdt, 4)?) };
            let offset = child(traf, b"trun").and_then(first_composition_offset).unwrap_or(0);
            let ticks = (decode + offset - track.edit_shift).max(0);
            Some((ticks as u128 * 1000 / u128::from(track.timescale.max(1))) as u64)
        })
    })
}

/// Composition offset of the first sample in a `trun`, if it has one.
fn first_composition_offset(trun: &[u8]) -> Option<i64> {
    let version = trun[0];
    let flags = be32(trun, 0)? & 0x00FF_FFFF;
    if flags & 0x800 == 0 || be32(trun, 4)? == 0 {
        return None;
    }
    let mut at = 8;
    if flags & 0x1 != 0 {
        at += 4; // data offset
    }
    if flags & 0x4 != 0 {
        at += 4; // first sample flags
    }
    for bit in [0x100, 0x200, 0x400] {
        if flags & bit != 0 {
            at += 4;
        }
    }
    let raw = be32(trun, at)?;
    Some(if version == 0 { i64::from(raw) } else { i64::from(raw as i32) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn full(version: u8, flags: u32, rest: &[u8]) -> Vec<u8> {
        let mut out = ((u32::from(version) << 24) | flags).to_be_bytes().to_vec();
        out.extend_from_slice(rest);
        out
    }

    fn trak(id: u32, handler: &[u8; 4], timescale: u32, shift: Option<u32>) -> Vec<u8> {
        let mut tkhd_rest = vec![0u8; 8];
        tkhd_rest.extend(id.to_be_bytes());
        tkhd_rest.extend([0u8; 68]);
        let mut mdhd_rest = vec![0u8; 8];
        mdhd_rest.extend(timescale.to_be_bytes());
        mdhd_rest.extend([0u8; 8]);
        let mut hdlr_rest = vec![0u8; 4];
        hdlr_rest.extend(handler);
        hdlr_rest.extend([0u8; 13]);
        let mut parts = bx(b"tkhd", &full(0, 3, &tkhd_rest));
        if let Some(s) = shift {
            let mut elst = 1u32.to_be_bytes().to_vec();
            elst.extend(0u32.to_be_bytes());
            elst.extend(s.to_be_bytes());
            elst.extend([0, 1, 0, 0]);
            parts.extend(bx(b"edts", &bx(b"elst", &full(0, 0, &elst))));
        }
        let mdia = [bx(b"mdhd", &full(0, 0, &mdhd_rest)), bx(b"hdlr", &full(0, 0, &hdlr_rest))].concat();
        parts.extend(bx(b"mdia", &mdia));
        bx(b"trak", &parts)
    }

    fn init(shift: Option<u32>) -> Vec<u8> {
        let moov = [trak(2, b"soun", 48000, None), trak(1, b"vide", 24000, shift)].concat();
        [bx(b"ftyp", b"iso5\0\0\0\0"), bx(b"moov", &moov)].concat()
    }

    fn segment(track: u32, decode: u64, first_cto: Option<u32>) -> Vec<u8> {
        let tfhd = bx(b"tfhd", &full(0, 0x020000, &track.to_be_bytes()));
        let tfdt = bx(b"tfdt", &full(1, 0, &decode.to_be_bytes()));
        let trun = match first_cto {
            // flags: data offset, duration, size, flags, composition offset
            Some(cto) => {
                let mut r = 1u32.to_be_bytes().to_vec();
                r.extend([0u8; 4]);
                r.extend([0u8; 12]);
                r.extend(cto.to_be_bytes());
                bx(b"trun", &full(0, 0xF01, &r))
            }
            None => bx(b"trun", &full(0, 0x301, &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])),
        };
        let traf = bx(b"traf", &[tfhd, tfdt, trun].concat());
        let audio = bx(
            b"traf",
            &[bx(b"tfhd", &full(0, 0x020000, &2u32.to_be_bytes())), bx(b"tfdt", &full(1, 0, &0u64.to_be_bytes()))]
                .concat(),
        );
        [bx(b"moof", &[bx(b"mfhd", &[0u8; 8]), audio, traf].concat()), bx(b"mdat", &[0u8; 16])].concat()
    }

    #[test]
    fn finds_the_video_track_and_its_edit_shift() {
        let t = video_track(&init(Some(2002))).unwrap();
        assert_eq!(t, VideoTrack { track_id: 1, timescale: 24000, edit_shift: 2002 });
        assert_eq!(video_track(&init(None)).unwrap().edit_shift, 0);
        assert_eq!(video_track(b"garbage"), None);
    }

    #[test]
    fn first_video_time_uses_decode_time_offset_and_shift() {
        let track = video_track(&init(Some(2002))).unwrap();
        // Decode time 60 s - 2002 ticks, first sample shown 2002 ticks later, shift 2002 ticks.
        let seg = segment(1, 60 * 24000 - 2002, Some(2002));
        assert_eq!(first_video_time_ms(&seg, &track), Some(59_916));
        let seg = segment(1, 60 * 24000, Some(2002));
        assert_eq!(first_video_time_ms(&seg, &track), Some(60_000));
        let no_cto = segment(1, 60 * 24000 + 2002, None);
        assert_eq!(first_video_time_ms(&no_cto, &track), Some(60_000));
        assert_eq!(first_video_time_ms(&segment(3, 0, None), &track), None);
    }
}

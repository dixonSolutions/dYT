//! Segment indexes of YouTube's DASH streams, so a clip downloads only the bytes it covers.
//!
//! MP4 streams carry a `sidx` box at `indexRange`; WebM streams carry Matroska `Cues` there.
//! Both turn into the same list of `Segment`s: a time span and the byte span that holds it.
//! A partial file is the stream's init bytes followed by the chosen segments, which is a valid
//! fragmented MP4 / WebM that ffmpeg reads with the original (absolute) timestamps.

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    /// Inclusive byte range in the full stream.
    pub first_byte: u64,
    pub last_byte: u64,
}

impl Segment {
    pub fn len(&self) -> u64 {
        self.last_byte - self.first_byte + 1
    }
}

fn be(b: &[u8]) -> u64 {
    b.iter().fold(0u64, |a, &x| (a << 8) | x as u64)
}

fn bad(what: &str) -> Error {
    Error::Parse(format!("segment index: {what}"))
}

// ---------------------------------------------------------------------------------------------
// MP4

/// Parse the `sidx` box in `buf`, which was read from `buf_offset` of the stream.
pub fn parse_sidx(buf: &[u8], buf_offset: u64, total_len: u64) -> Result<Vec<Segment>> {
    let mut pos = 0usize;
    while pos + 8 <= buf.len() {
        let mut size = be(&buf[pos..pos + 4]);
        let kind = &buf[pos + 4..pos + 8];
        let mut header = 8usize;
        if size == 1 {
            if pos + 16 > buf.len() {
                break;
            }
            size = be(&buf[pos + 8..pos + 16]);
            header = 16;
        }
        if size < header as u64 {
            return Err(bad("box size"));
        }
        if kind == b"sidx" {
            let end = pos.checked_add(size as usize).filter(|&e| e <= buf.len()).ok_or_else(|| bad("truncated sidx"))?;
            return parse_sidx_body(&buf[pos + header..end], buf_offset + end as u64, total_len);
        }
        pos += size as usize;
    }
    Err(bad("no sidx box"))
}

fn parse_sidx_body(b: &[u8], anchor: u64, total_len: u64) -> Result<Vec<Segment>> {
    let need = |n: usize| if b.len() < n { Err(bad("short sidx")) } else { Ok(()) };
    need(12)?;
    let version = b[0];
    let timescale = be(&b[8..12]);
    if timescale == 0 {
        return Err(bad("timescale 0"));
    }
    let (ept, first_offset, mut p) = if version == 0 {
        need(20)?;
        (be(&b[12..16]), be(&b[16..20]), 20)
    } else {
        need(28)?;
        (be(&b[12..20]), be(&b[20..28]), 28)
    };
    need(p + 4)?;
    let count = be(&b[p + 2..p + 4]) as usize;
    p += 4;
    need(p + count * 12)?;
    let mut out = Vec::with_capacity(count);
    let mut offset = anchor + first_offset;
    let mut t = ept;
    for i in 0..count {
        let r = &b[p + i * 12..p + i * 12 + 12];
        let size = be(&r[0..4]) & 0x7FFF_FFFF;
        let dur = be(&r[4..8]);
        if size == 0 {
            continue;
        }
        let last = (offset + size - 1).min(total_len.saturating_sub(1));
        out.push(Segment { start: t as f64 / timescale as f64, end: (t + dur) as f64 / timescale as f64, first_byte: offset, last_byte: last });
        offset += size;
        t += dur;
    }
    if out.is_empty() {
        return Err(bad("empty sidx"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// WebM / Matroska

const ID_EBML: u64 = 0x1A45_DFA3;
const ID_SEGMENT: u64 = 0x1853_8067;
const ID_SEEKHEAD: u64 = 0x114D_9B74;
const ID_INFO: u64 = 0x1549_A966;
const ID_TIMESTAMP_SCALE: u64 = 0x2A_D7B1;
const ID_CUES: u64 = 0x1C53_BB6B;
const ID_CUE_POINT: u64 = 0xBB;
const ID_CUE_TIME: u64 = 0xB3;
const ID_CUE_TRACK_POSITIONS: u64 = 0xB7;
const ID_CUE_CLUSTER_POSITION: u64 = 0xF1;
const ID_VOID: u64 = 0xEC;

/// Element id (marker bits kept) and its length in bytes.
fn read_id(b: &[u8]) -> Option<(u64, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 4 || b.len() < len {
        return None;
    }
    Some((be(&b[..len]), len))
}

/// Element size (`None` = unknown size) and its length in bytes.
fn read_size(b: &[u8]) -> Option<(Option<u64>, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || b.len() < len {
        return None;
    }
    let mask = if len == 8 { 0 } else { 0xFFu8 >> len };
    let mut v = (first & mask) as u64;
    let mut all_ones = (first & mask) == mask;
    for &x in &b[1..len] {
        v = (v << 8) | x as u64;
        all_ones &= x == 0xFF;
    }
    Some((if all_ones { None } else { Some(v) }, len))
}

struct Elem {
    id: u64,
    /// Offset of the element's header in the buffer.
    start: usize,
    /// Offset of its data.
    data: usize,
    /// Data length (to the end of the buffer for unknown sizes).
    size: usize,
    size_len: usize,
}

fn elements(b: &[u8]) -> impl Iterator<Item = Elem> + '_ {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        let (id, il) = read_id(&b[pos..])?;
        let (size, sl) = read_size(&b[pos + il..])?;
        let data = pos + il + sl;
        let size = size.map(|s| s as usize).unwrap_or(b.len().saturating_sub(data)).min(b.len().saturating_sub(data));
        let e = Elem { id, start: pos, data, size, size_len: sl };
        pos = data + size;
        Some(e)
    })
}

fn uint(b: &[u8]) -> u64 {
    be(&b[..b.len().min(8)])
}

/// What the init bytes of a WebM stream tell us.
#[derive(Debug, Clone, PartialEq)]
pub struct WebmHeader {
    /// Stream offset where the Segment's data starts; Cue positions are relative to it.
    pub segment_data_start: u64,
    /// Nanoseconds per timestamp tick (1 ms by default).
    pub timestamp_scale: u64,
}

pub fn parse_webm_header(init: &[u8]) -> Result<WebmHeader> {
    let mut seg_start = None;
    let mut scale = 1_000_000;
    for e in elements(init) {
        if e.id == ID_SEGMENT {
            seg_start = Some(e.data as u64);
            for c in elements(&init[e.data..e.data + e.size]) {
                if c.id == ID_INFO {
                    let info = &init[e.data + c.data..e.data + c.data + c.size];
                    for i in elements(info) {
                        if i.id == ID_TIMESTAMP_SCALE {
                            scale = uint(&info[i.data..i.data + i.size]).max(1);
                        }
                    }
                }
            }
            break;
        }
    }
    Ok(WebmHeader { segment_data_start: seg_start.ok_or_else(|| bad("no Segment in WebM init"))?, timestamp_scale: scale })
}

pub fn parse_cues(cues: &[u8], header: &WebmHeader, total_len: u64, duration: f64) -> Result<Vec<Segment>> {
    let body = elements(cues).find(|e| e.id == ID_CUES).map(|e| &cues[e.data..e.data + e.size]).ok_or_else(|| bad("no Cues"))?;
    let mut points: Vec<(f64, u64)> = vec![];
    for cp in elements(body).filter(|e| e.id == ID_CUE_POINT) {
        let cpb = &body[cp.data..cp.data + cp.size];
        let mut time = None;
        let mut pos = None;
        for c in elements(cpb) {
            match c.id {
                ID_CUE_TIME => time = Some(uint(&cpb[c.data..c.data + c.size])),
                ID_CUE_TRACK_POSITIONS if pos.is_none() => {
                    let tp = &cpb[c.data..c.data + c.size];
                    pos = elements(tp).find(|e| e.id == ID_CUE_CLUSTER_POSITION).map(|e| uint(&tp[e.data..e.data + e.size]));
                }
                _ => {}
            }
        }
        if let (Some(t), Some(p)) = (time, pos) {
            points.push((t as f64 * header.timestamp_scale as f64 / 1e9, header.segment_data_start + p));
        }
    }
    points.sort_by(|a, b| a.1.cmp(&b.1));
    points.dedup_by_key(|p| p.1);
    if points.is_empty() {
        return Err(bad("empty Cues"));
    }
    let mut out = Vec::with_capacity(points.len());
    for (i, &(t, p)) in points.iter().enumerate() {
        let (end_t, next) = points.get(i + 1).map(|&(t, p)| (t, p)).unwrap_or((duration.max(t), total_len));
        if next > p {
            out.push(Segment { start: t, end: end_t, first_byte: p, last_byte: next - 1 });
        }
    }
    Ok(out)
}

/// Make WebM init bytes safe to splice clusters after: the Segment gets an unknown size and
/// the SeekHead (which points at Cues and clusters that are no longer where it says) becomes
/// a Void element of the same length. Without this, demuxers seek to offsets past the end.
pub fn patch_webm_init(init: &mut [u8]) -> Result<()> {
    let seg = elements(init).find(|e| e.id == ID_SEGMENT).ok_or_else(|| bad("no Segment in WebM init"))?;
    let (sz_at, sl) = (seg.data - seg.size_len, seg.size_len);
    let marker = 0x80u8 >> (sl - 1);
    init[sz_at] = marker | (marker.wrapping_sub(1));
    for b in &mut init[sz_at + 1..sz_at + sl] {
        *b = 0xFF;
    }
    let seg_data = seg.data;
    let heads: Vec<(usize, usize)> = elements(&init[seg_data..])
        .take_while(|e| e.id != 0x1F43_B675) // Cluster
        .filter(|e| e.id == ID_SEEKHEAD)
        .map(|e| (seg_data + e.start, e.data + e.size - e.start))
        .collect();
    for (at, total) in heads {
        // 1-byte Void id + an 8-byte size covering the rest.
        if total < 9 {
            continue;
        }
        init[at] = ID_VOID as u8;
        let payload = (total - 9) as u64;
        init[at + 1] = 0x01;
        for i in 0..7 {
            init[at + 2 + i] = (payload >> (8 * (6 - i))) as u8;
        }
    }
    Ok(())
}

/// Element ids we expect at the start of a WebM file, for sanity checks.
pub fn looks_like_webm(b: &[u8]) -> bool {
    read_id(b).is_some_and(|(id, _)| id == ID_EBML)
}

/// The segments that cover `[start, end]`.
pub fn covering(segments: &[Segment], start: f64, end: f64) -> &[Segment] {
    let first = segments.iter().rposition(|s| s.start <= start + 1e-6).unwrap_or(0);
    let last = segments.iter().position(|s| s.end >= end - 1e-6).unwrap_or(segments.len().saturating_sub(1)).max(first);
    &segments[first..=last]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidx_box(timescale: u32, ept: u32, first_offset: u32, refs: &[(u32, u32)]) -> Vec<u8> {
        let mut body = vec![0, 0, 0, 0, 0, 0, 0, 1];
        body.extend(timescale.to_be_bytes());
        body.extend(ept.to_be_bytes());
        body.extend(first_offset.to_be_bytes());
        body.extend([0, 0]);
        body.extend((refs.len() as u16).to_be_bytes());
        for &(size, dur) in refs {
            body.extend(size.to_be_bytes());
            body.extend(dur.to_be_bytes());
            body.extend(0x9000_0000u32.to_be_bytes());
        }
        let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend(b"sidx");
        b.extend(body);
        b
    }

    #[test]
    fn sidx_segments() {
        let b = sidx_box(1000, 0, 0, &[(100, 5000), (200, 5000), (50, 2500)]);
        let segs = parse_sidx(&b, 742, 10_000).unwrap();
        let anchor = 742 + b.len() as u64;
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], Segment { start: 0.0, end: 5.0, first_byte: anchor, last_byte: anchor + 99 });
        assert_eq!(segs[1].first_byte, anchor + 100);
        assert_eq!((segs[2].start, segs[2].end), (10.0, 12.5));
    }

    fn vint(v: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for i in 0..len {
            out[len - 1 - i] = (v >> (8 * i)) as u8;
        }
        out[0] |= 0x80 >> (len - 1);
        out
    }

    fn el(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut b = id.to_vec();
        b.extend(vint(body.len() as u64, 8));
        b.extend(body);
        b
    }

    fn webm_init() -> Vec<u8> {
        let ebml = el(&[0x1A, 0x45, 0xDF, 0xA3], &el(&[0x42, 0x82], b"webm"));
        let seekhead = el(&[0x11, 0x4D, 0x9B, 0x74], &[0xAB; 20]);
        let info = el(&[0x15, 0x49, 0xA9, 0x66], &el(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40]));
        let mut seg_body = seekhead;
        seg_body.extend(info);
        let mut b = ebml;
        b.extend([0x18, 0x53, 0x80, 0x67]);
        b.extend(vint(1_000_000, 8));
        b.extend(seg_body);
        b
    }

    #[test]
    fn webm_header_and_cues() {
        let init = webm_init();
        let h = parse_webm_header(&init).unwrap();
        assert_eq!(h.timestamp_scale, 1_000_000);
        let ebml_len = 4 + 8 + 2 + 8 + 4;
        assert_eq!(h.segment_data_start, (ebml_len + 12) as u64);

        let cue = |t: u16, pos: u16| {
            let mut tp = el(&[0xF7], &[1]);
            tp.extend(el(&[0xF1], &pos.to_be_bytes()));
            let mut cp = el(&[0xB3], &t.to_be_bytes());
            cp.extend(el(&[0xB7], &tp));
            el(&[0xBB], &cp)
        };
        let mut cues_body = cue(0, 500);
        cues_body.extend(cue(5000, 900));
        cues_body.extend(cue(10000, 1400));
        let cues = el(&[0x1C, 0x53, 0xBB, 0x6B], &cues_body);
        let segs = parse_cues(&cues, &h, 3000, 12.0).unwrap();
        let base = h.segment_data_start;
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], Segment { start: 0.0, end: 5.0, first_byte: base + 500, last_byte: base + 899 });
        assert_eq!(segs[2], Segment { start: 10.0, end: 12.0, first_byte: base + 1400, last_byte: 2999 });
        assert_eq!(covering(&segs, 6.0, 7.0), &segs[1..2]);
        assert_eq!(covering(&segs, 4.0, 11.0), &segs[0..3]);
        assert_eq!(covering(&segs, 0.0, 5.0), &segs[0..1]);
    }

    #[test]
    fn webm_init_patch() {
        let mut init = webm_init();
        patch_webm_init(&mut init).unwrap();
        let seg = elements(&init).find(|e| e.id == ID_SEGMENT).unwrap();
        assert_eq!(read_size(&init[seg.data - 8..]).unwrap().0, None, "segment size is unknown");
        let kids: Vec<u64> = elements(&init[seg.data..]).map(|e| e.id).collect();
        assert_eq!(kids, vec![ID_VOID, ID_INFO], "seekhead became void, info is still readable");
        assert_eq!(parse_webm_header(&init).unwrap().timestamp_scale, 1_000_000);
    }
}

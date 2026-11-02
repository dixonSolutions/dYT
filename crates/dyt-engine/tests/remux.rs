//! Clips and merges offline, on small DASH-style files (tests/fixtures, made by ffmpeg's own
//! DASH muxers): the stream index is parsed, the covering segments are spliced after the init
//! bytes exactly as a clip download does, and ffmpeg cuts and merges them.
//!
//! In CI this runs against the minimal ffmpeg the apps ship (`DYT_FFMPEG`), which proves its
//! build has everything stream copy needs. Skipped when no ffmpeg is found.

use std::path::{Path, PathBuf};

use dyt_engine::ffmpeg::{self, Cut, Input};
use dyt_engine::index::{self, Segment};
use dyt_engine::CancellationToken;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

fn out_dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("remux").join(name);
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn be32(b: &[u8]) -> usize {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize
}

/// (init end, index start, index end) of a fragmented MP4: ftyp+moov, then sidx.
fn mp4_ranges(b: &[u8]) -> (u64, u64, u64) {
    let mut p = 0;
    let mut init_end = 0;
    while p + 8 <= b.len() {
        let size = be32(&b[p..]);
        match &b[p + 4..p + 8] {
            b"moov" => init_end = p + size - 1,
            b"sidx" => return (init_end as u64, p as u64, (p + size - 1) as u64),
            _ => {}
        }
        p += size;
    }
    panic!("no sidx");
}

fn vint(b: &[u8], marker: bool) -> (u64, usize) {
    let len = b[0].leading_zeros() as usize + 1;
    let first = if marker { b[0] as u64 } else { (b[0] as u16 & (0xFFu16 >> len)) as u64 };
    (b[1..len].iter().fold(first, |a, &x| (a << 8) | x as u64), len)
}

/// (init end, index start, index end) of a WebM: everything before Cues, then Cues, found by
/// walking the Segment's children the way YouTube's ranges are laid out.
fn webm_ranges(b: &[u8]) -> (u64, u64, u64) {
    let (_, il) = vint(b, true);
    let (size, sl) = vint(&b[il..], false);
    let mut p = il + sl + size as usize; // past the EBML header
    let (_, il) = vint(&b[p..], true);
    let (_, sl) = vint(&b[p + il..], false);
    p += il + sl; // into the Segment
    while p < b.len() {
        let (id, il) = vint(&b[p..], true);
        let (size, sl) = vint(&b[p + il..], false);
        let end = p + il + sl + size as usize;
        if id == 0x1C53_BB6B {
            return ((p - 1) as u64, p as u64, (end - 1) as u64);
        }
        p = end;
    }
    panic!("no Cues");
}

/// The partial file a clip download writes: init bytes, then the covering segments.
fn partial(b: &[u8], webm: bool, start: f64, end: f64, duration: f64) -> (Vec<u8>, Vec<Segment>, Vec<Segment>) {
    let (init_end, idx_start, idx_end) = if webm { webm_ranges(b) } else { mp4_ranges(b) };
    let idx = &b[idx_start as usize..=idx_end as usize];
    let mut init = b[..=init_end as usize].to_vec();
    let segs = if webm {
        let h = index::parse_webm_header(&init).unwrap();
        let s = index::parse_cues(idx, &h, b.len() as u64, duration).unwrap();
        index::patch_webm_init(&mut init).unwrap();
        s
    } else {
        index::parse_sidx(idx, idx_start, b.len() as u64).unwrap()
    };
    let cover = index::covering(&segs, start, end).to_vec();
    for s in &cover {
        init.extend_from_slice(&b[s.first_byte as usize..=s.last_byte as usize]);
    }
    (init, segs, cover)
}

/// Duration and stream kinds of a file, from `ffmpeg -i` (the minimal build has no ffprobe).
fn inspect(ff: &Path, file: &Path) -> (f64, Vec<String>) {
    let o = std::process::Command::new(ff).args(["-hide_banner", "-i"]).arg(file).output().unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    let d = err.split("Duration: ").nth(1).and_then(|r| r.split(',').next()).unwrap_or("0:0:0");
    let p: Vec<f64> = d.split(':').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
    let duration = p[0] * 3600.0 + p[1] * 60.0 + p[2];
    let kinds = err.lines().filter(|l| l.contains("Stream #")).map(|l| if l.contains("Video:") { "video".into() } else { "audio".into() }).collect();
    (duration, kinds)
}

async fn clip_and_merge(name: &str, video: &str, audio: &str, webm: bool) {
    let Some(ff) = ffmpeg::locate(&[]) else {
        eprintln!("skipped: no ffmpeg (set DYT_FFMPEG)");
        return;
    };
    let dir = out_dir(name);
    let (start, end) = (3.2, 7.7);
    let (v, vsegs, vcover) = partial(&fixture(video), webm, start, end, 12.0);
    let (a, _, acover) = partial(&fixture(audio), webm, start, end, 12.0);
    assert!(vsegs.len() >= 10, "{} video segments", vsegs.len());
    assert!(vcover.len() < vsegs.len() && acover.len() < 6, "only the covering segments are used");
    assert!(vcover[0].start <= start && vcover.last().unwrap().end >= end);
    let (vp, ap) = (dir.join(format!("v.{}", if webm { "webm" } else { "mp4" })), dir.join(format!("a.{}", if webm { "webm" } else { "m4a" })));
    std::fs::write(&vp, v).unwrap();
    std::fs::write(&ap, a).unwrap();

    let out = dir.join("clip.mp4");
    let args = ffmpeg::args(&[Input { path: &vp, stream: 'v' }, Input { path: &ap, stream: 'a' }], Some(&Cut { start, end }), None, &out);
    ffmpeg::run(&ff, &args, &CancellationToken::new(), |_| {}).await.unwrap();
    let (d, kinds) = inspect(&ff, &out);
    assert!((d - (end - start)).abs() < 0.12, "{name}: clip is {d}s, wanted {}", end - start);
    assert_eq!(kinds, ["video", "audio"], "{name}");

    // The whole thing merged, into the container a full download would use.
    let full = dir.join(format!("full.{}", if webm { "webm" } else { "mp4" }));
    let (vf, af) = (dir.join("vf"), dir.join("af"));
    std::fs::write(&vf, fixture(video)).unwrap();
    std::fs::write(&af, fixture(audio)).unwrap();
    let args = ffmpeg::args(&[Input { path: &vf, stream: 'v' }, Input { path: &af, stream: 'a' }], None, None, &full);
    ffmpeg::run(&ff, &args, &CancellationToken::new(), |_| {}).await.unwrap();
    let (d, kinds) = inspect(&ff, &full);
    assert!((d - 12.0).abs() < 0.15, "{name}: merged file is {d}s");
    assert_eq!(kinds.len(), 2);

    // Audio only, to the format an audio download uses.
    let ext = if webm { "opus" } else { "m4a" };
    let solo = dir.join(format!("audio.{ext}"));
    let args = ffmpeg::args(&[Input { path: &af, stream: 'a' }], None, None, &solo);
    ffmpeg::run(&ff, &args, &CancellationToken::new(), |_| {}).await.unwrap();
    assert_eq!(inspect(&ff, &solo).1, ["audio"]);
}

#[tokio::test]
async fn mp4_clip_and_merge() {
    clip_and_merge("mp4", "v.mp4", "a.m4a", false).await;
}

#[tokio::test]
async fn webm_clip_and_merge() {
    clip_and_merge("webm", "v.webm", "a.webm", true).await;
}

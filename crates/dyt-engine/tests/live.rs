//! Real downloads from YouTube. They need the network, so they only run with `DYT_LIVE=1`:
//!
//!     DYT_LIVE=1 cargo test -p dyt-engine --test live -- --test-threads 2
//!
//! Output goes to `target/live-tests/`. Checks use ffprobe when it is installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use dyt_engine::fetch::{fetch_all, FetchOptions, Piece, Refresh, Target};
use dyt_engine::{select, CancellationToken, DownloadRequest, Engine, EngineConfig, Error, Preference};

/// "Me at the zoo": 19 s, the first video on YouTube; tiny and never going away.
const ZOO: &str = "jNQXAC9IVRw";
/// Big Buck Bunny 4K60 (Blender Foundation), 10:35.
const BUNNY: &str = "aqz-KE-bpKQ";
/// Made for kids: refused by the first client, served by the JS fallback.
const KIDS: &str = "e_04ZrNroTo";

fn live() -> bool {
    let on = std::env::var_os("DYT_LIVE").is_some();
    if !on {
        eprintln!("skipped: set DYT_LIVE=1 to run live YouTube tests");
    }
    on
}

fn engine() -> Engine {
    Engine::new(EngineConfig::default()).unwrap()
}

fn out(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("live-tests").join(name);
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Probe {
    duration: f64,
    codecs: Vec<String>,
}

fn probe(p: &Path) -> Option<Probe> {
    let o = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration:stream=codec_name", "-of", "default=nw=1"])
        .arg(p)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    let duration = s.lines().find_map(|l| l.strip_prefix("duration=")).and_then(|d| d.parse().ok())?;
    let codecs = s.lines().filter_map(|l| l.strip_prefix("codec_name=")).map(str::to_string).collect();
    Some(Probe { duration, codecs })
}

#[tokio::test]
async fn link_shapes_resolve_to_the_same_video() {
    if !live() {
        return;
    }
    let e = engine();
    for link in [
        format!("https://www.youtube.com/watch?v={ZOO}"),
        format!("https://youtu.be/{ZOO}?si=share"),
        format!("https://www.youtube.com/shorts/{ZOO}"),
        format!("https://music.youtube.com/watch?v={ZOO}"),
        ZOO.to_string(),
    ] {
        let info = e.info(&link).await.unwrap_or_else(|err| panic!("{link}: {err}"));
        assert_eq!(info.id, ZOO);
        assert!(info.title.contains("zoo"), "{}", info.title);
        assert!((18.0..21.0).contains(&info.duration));
        assert!(select::pick_video(&info, None, Preference::Quality).is_some());
        assert!(select::pick_audio(&info, Preference::Quality).is_some());
    }
}

#[tokio::test]
async fn full_download_merges_video_and_audio() {
    if !live() {
        return;
    }
    let e = engine();
    let info = e.info(ZOO).await.unwrap();
    let dir = out("full");
    let r = e
        .download(
            DownloadRequest {
                url: ZOO.into(),
                video: select::pick_video(&info, None, Preference::Quality).map(|f| f.itag),
                audio: select::pick_audio(&info, Preference::Quality).map(|f| f.itag),
                dir: dir.clone(),
                ..Default::default()
            },
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(r.files.len(), 1);
    let f = &r.files[0];
    assert!(f.file_name().unwrap().to_string_lossy().starts_with("Me at the zoo"), "{f:?}");
    assert!(std::fs::read_dir(&dir).unwrap().count() == 1, "no temp files left behind");
    if let Some(p) = probe(f) {
        assert!((18.5..19.5).contains(&p.duration), "{}", p.duration);
        assert_eq!(p.codecs.len(), 2, "{:?}", p.codecs);
    }
}

#[tokio::test]
async fn clip_downloads_only_the_span_and_cuts_exactly() {
    if !live() {
        return;
    }
    let e = engine();
    let info = e.info(BUNNY).await.unwrap();
    let v = select::pick_video(&info, Some(720), Preference::Compatible).unwrap();
    let a = select::pick_audio(&info, Preference::Compatible).unwrap();
    let full = v.content_length.unwrap() + a.content_length.unwrap();
    let r = e
        .download(
            DownloadRequest { url: BUNNY.into(), video: Some(v.itag), audio: Some(a.itag), clip: Some((300.0, 312.5)), dir: out("clip"), file_name: Some("bunny clip".into()), ..Default::default() },
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(r.bytes < full / 10, "a 12.5 s clip fetched {} of {} bytes", r.bytes, full);
    assert!(r.files[0].ends_with("bunny clip.mp4"));
    if let Some(p) = probe(&r.files[0]) {
        assert!((p.duration - 12.5).abs() < 0.1, "{}", p.duration);
        assert!(p.codecs.contains(&"h264".to_string()) && p.codecs.contains(&"aac".to_string()), "{:?}", p.codecs);
    }
}

#[tokio::test]
async fn audio_only_and_raw_streams() {
    if !live() {
        return;
    }
    let e = engine();
    let info = e.info(ZOO).await.unwrap();
    let a = select::pick_audio(&info, Preference::Quality).unwrap();
    let dir = out("audio");
    let r = e.download(DownloadRequest { url: ZOO.into(), audio: Some(a.itag), dir: dir.clone(), ..Default::default() }, |_| {}, CancellationToken::new()).await.unwrap();
    let ext = r.files[0].extension().unwrap().to_string_lossy().to_string();
    assert!(ext == "opus" || ext == "m4a", "{ext}");
    if let Some(p) = probe(&r.files[0]) {
        assert_eq!(p.codecs.len(), 1);
    }

    let v = select::pick_video(&info, Some(360), Preference::Quality).unwrap();
    let r = e
        .download(DownloadRequest { url: ZOO.into(), video: Some(v.itag), audio: Some(a.itag), raw: true, dir, ..Default::default() }, |_| {}, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(r.files.len(), 2, "raw keeps the streams apart: {:?}", r.files);
    assert_eq!(std::fs::metadata(&r.files[0]).unwrap().len(), v.content_length.unwrap(), "raw video is byte-for-byte the stream");
}

#[tokio::test]
async fn expired_urls_are_refreshed_mid_download() {
    if !live() {
        return;
    }
    let e = engine();
    let info = e.info(ZOO).await.unwrap();
    let a = select::pick_audio(&info, Preference::Quality).unwrap().clone();
    // A tampered signature makes YouTube answer 403, exactly like an expired URL.
    let broken = a.url.replace("sig=", "sig=AA").replace("lsig=", "lsig=AA");
    let e2 = e.clone();
    let refresh: Refresh = Arc::new(move || {
        let e = e2.clone();
        Box::pin(async move { Ok(e.extract(ZOO).await?.formats.into_iter().map(|f| (f.itag, f.url)).collect()) })
    });
    let path = out("refresh").join("a.bin");
    let len = a.content_length.unwrap();
    let target = Target { path: path.clone(), url: broken, itag: a.itag, pieces: vec![Piece { first: 0, last: len - 1, dest_offset: 0 }] };
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder().http1_only().build().unwrap();
    fetch_all(&client, &[target], &FetchOptions::default(), Some(refresh), Arc::new(AtomicU64::new(0)), CancellationToken::new()).await.unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), len);
}

#[tokio::test]
async fn made_for_kids_goes_through_the_js_fallback() {
    if !live() {
        return;
    }
    let e = engine();
    let info = e.info(KIDS).await.unwrap();
    assert_ne!(info.client, "visionos");
    assert!(!info.title.is_empty() && info.title != KIDS, "details were filled in: {}", info.title);
    let v = select::pick_video(&info, Some(360), Preference::Quality).unwrap();
    let a = select::pick_audio(&info, Preference::Quality).unwrap();
    let r = e
        .download(DownloadRequest { url: KIDS.into(), video: Some(v.itag), audio: Some(a.itag), clip: Some((20.0, 25.0)), dir: out("kids"), ..Default::default() }, |_| {}, CancellationToken::new())
        .await
        .unwrap();
    // Solved `n` means full speed; an unsolved one crawls at ~50 KB/s.
    assert!(r.seconds < 20.0, "took {:.1}s", r.seconds);
    if let Some(p) = probe(&r.files[0]) {
        assert!((p.duration - 5.0).abs() < 0.1, "{}", p.duration);
    }
}

#[tokio::test]
async fn errors_are_specific() {
    if !live() {
        return;
    }
    let e = engine();
    assert!(matches!(e.info("not a link").await, Err(Error::InvalidUrl)));
    assert!(matches!(e.info("aaaaaaaaaaa").await, Err(Error::Unavailable(_))));
    // Lofi Girl's 24/7 stream.
    assert!(matches!(e.info("jfKfPfyJRdk").await, Err(Error::Live)));
}

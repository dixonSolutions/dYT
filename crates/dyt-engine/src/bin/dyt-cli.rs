//! `dyt-cli`: the engine from a terminal. Handy on its own and for testing the engine for real.
//!
//!   dyt-cli info <link>
//!   dyt-cli get <link> [-o DIR] [-q HEIGHT] [--compat] [-x | --video-only] [--clip START-END]
//!                  [--raw] [-n NAME] [-j CONNECTIONS] [--video ITAG] [--audio ITAG]

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use dyt_engine::{select, CancellationToken, DownloadRequest, Engine, EngineConfig, Preference, Stage, StreamKind};

struct Log;
impl log::Log for Log {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        eprintln!("[{}] {}", r.level(), r.args());
    }
    fn flush(&self) {}
}

fn mb(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1e6)
}

fn usage() -> ExitCode {
    eprintln!("usage: dyt-cli info <link>\n       dyt-cli get <link> [-o DIR] [-q HEIGHT] [--compat] [-x|--video-only] [--clip START-END] [--raw] [-n NAME] [-j N] [--video ITAG] [--audio ITAG]");
    ExitCode::from(2)
}

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::var_os("DYT_LOG").is_some() {
        log::set_logger(&Log).ok();
        log::set_max_level(log::LevelFilter::Debug);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(cmd), Some(link)) = (args.first(), args.get(1)) else { return usage() };
    let mut config = EngineConfig::default();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    if let Some(j) = flag("-j").and_then(|j| j.parse().ok()) {
        config.fetch.connections = j;
    }
    let engine = match Engine::new(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let t = std::time::Instant::now();
    let info = match engine.info(link).await {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error [{}]: {e}", e.kind());
            return ExitCode::FAILURE;
        }
    };
    eprintln!("{} — {} ({}), via {} in {:.2}s", info.title, info.author, dyt_engine::names::clock(info.duration), info.client, t.elapsed().as_secs_f64());

    match cmd.as_str() {
        "info" => {
            for f in &info.formats {
                let what = match f.kind {
                    StreamKind::Audio => format!("audio {}k {}Hz", f.bitrate / 1000, f.audio_sample_rate.unwrap_or(0)),
                    _ => format!("{}p{} {}", f.height.unwrap_or(0), f.fps.unwrap_or(0), if f.hdr { "HDR" } else { "" }),
                };
                let track = f.audio_track.as_ref().map(|t| format!(" [{}{}]", t.name, if t.is_default { "*" } else { "" })).unwrap_or_default();
                println!(
                    "{:>4}  {:<5} {:<6} {:<22} {:>10}  {}{}{}",
                    f.itag,
                    format!("{:?}", f.container).to_lowercase(),
                    f.codec,
                    what,
                    f.content_length.map(mb).unwrap_or_else(|| "?".into()),
                    f.codecs,
                    track,
                    if f.drc { " drc" } else { "" }
                );
            }
            ExitCode::SUCCESS
        }
        "get" => {
            let pref = if args.iter().any(|a| a == "--compat") { Preference::Compatible } else { Preference::Quality };
            let max_h = flag("-q").and_then(|q| q.trim_end_matches('p').parse().ok());
            let audio_only = args.iter().any(|a| a == "-x" || a == "--audio-only");
            let video_only = args.iter().any(|a| a == "--video-only");
            let video = flag("--video").and_then(|v| v.parse().ok()).or_else(|| (!audio_only).then(|| select::pick_video(&info, max_h, pref).map(|f| f.itag)).flatten());
            let audio = flag("--audio").and_then(|v| v.parse().ok()).or_else(|| (!video_only).then(|| select::pick_audio(&info, pref).map(|f| f.itag)).flatten());
            let clip = flag("--clip").and_then(|c| {
                let (a, b) = c.split_once('-')?;
                Some((dyt_engine::id::parse_clock(a)?, dyt_engine::id::parse_clock(b)?))
            });
            let req = DownloadRequest {
                url: link.clone(),
                video,
                audio,
                clip,
                raw: args.iter().any(|a| a == "--raw"),
                dir: flag("-o").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")),
                file_name: flag("-n"),
                ext: flag("--ext"),
            };
            eprintln!("video {:?} audio {:?} clip {:?}", req.video, req.audio, req.clip);
            let cancel = CancellationToken::new();
            let c2 = cancel.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                c2.cancel();
            });
            let r = engine
                .download(
                    req,
                    |p| {
                        let line = match p.stage {
                            Stage::Downloading => format!(
                                "\r{:5.1}%  {} / {}  {:.1} MB/s  eta {:.0}s   ",
                                p.fraction * 100.0,
                                mb(p.done),
                                mb(p.total),
                                p.speed / 1e6,
                                p.eta.unwrap_or(0.0)
                            ),
                            Stage::Merging => format!("\r{:5.1}%  merging...                          ", p.fraction * 100.0),
                            _ => String::new(),
                        };
                        eprint!("{line}");
                        std::io::stderr().flush().ok();
                    },
                    cancel,
                )
                .await;
            eprintln!();
            match r {
                Ok(r) => {
                    for f in &r.files {
                        println!("{}", f.display());
                    }
                    eprintln!("{} in {:.2}s ({:.1} MB/s)", mb(r.bytes), r.seconds, r.bytes as f64 / 1e6 / r.seconds);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error [{}]: {e}", e.kind());
                    ExitCode::FAILURE
                }
            }
        }
        _ => usage(),
    }
}

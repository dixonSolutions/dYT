//! Running ffmpeg for stream-copy merges, remuxes and cuts. Nothing is ever re-encoded.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};

/// Where to look for ffmpeg: `$DYT_FFMPEG`, beside our executable (Flatpak `/app/bin`, the
/// Android native-library dir), then `$PATH`.
pub fn locate(extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DYT_FFMPEG").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    let names: &[&str] = if cfg!(windows) { &["ffmpeg.exe"] } else { &["ffmpeg", "libffmpeg.so"] };
    let mut dirs: Vec<PathBuf> = extra_dirs.to_vec();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(dir);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

pub struct Input<'a> {
    pub path: &'a Path,
    /// Map only this kind from the input: `v` or `a`.
    pub stream: char,
}

pub struct Cut {
    pub start: f64,
    pub end: f64,
}

pub struct Metadata<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub comment: &'a str,
}

/// Build the argument list. Separate from `run` so it can be tested.
pub fn args(inputs: &[Input], cut: Option<&Cut>, meta: Option<&Metadata>, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"].map(String::from).to_vec();
    for i in inputs {
        if let Some(c) = cut {
            // Input seeking on absolute stream timestamps (the partial files keep the
            // original ones), so both streams are cut at the same instant.
            a.extend(["-seek_timestamp".into(), "1".into(), "-ss".into(), fmt_t(c.start)]);
        }
        a.extend(["-i".into(), i.path.to_string_lossy().into_owned()]);
    }
    for (n, i) in inputs.iter().enumerate() {
        a.extend(["-map".into(), format!("{n}:{}:0", i.stream)]);
    }
    if let Some(c) = cut {
        a.extend(["-t".into(), fmt_t((c.end - c.start).max(0.001))]);
    }
    a.extend(["-c".into(), "copy".into(), "-map_metadata".into(), "-1".into(), "-map_chapters".into(), "-1".into()]);
    if let Some(m) = meta {
        for (k, v) in [("title", m.title), ("artist", m.artist), ("comment", m.comment)] {
            if !v.is_empty() {
                a.extend(["-metadata".into(), format!("{k}={v}")]);
            }
        }
    }
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "mp4" | "m4a") {
        // Opus/VP9 in MP4 are standard but ffmpeg still gates them behind this on older builds.
        a.extend(["-strict".into(), "experimental".into()]);
    }
    if ext == "m4a" {
        a.extend(["-f".into(), "ipod".into()]);
    }
    if ext == "opus" {
        a.extend(["-f".into(), "opus".into()]);
    }
    a.push(out.to_string_lossy().into_owned());
    a
}

fn fmt_t(t: f64) -> String {
    format!("{:.3}", t.max(0.0))
}

/// Run ffmpeg; `on_progress` gets the output time in seconds as it advances.
pub async fn run(ffmpeg: &Path, args: &[String], cancel: &CancellationToken, mut on_progress: impl FnMut(f64)) -> Result<()> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { Error::FfmpegMissing } else { Error::Io(e) })?;
    let stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let err_task = tokio::spawn(async move {
        let mut s = String::new();
        stderr.read_to_string(&mut s).await.ok();
        s
    });
    let mut lines = BufReader::new(stdout).lines();
    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(l)) => {
                    if let Some(us) = l.strip_prefix("out_time_us=").or_else(|| l.strip_prefix("out_time_ms=")) {
                        if let Ok(us) = us.trim().parse::<i64>() {
                            on_progress(us.max(0) as f64 / 1e6);
                        }
                    }
                }
                _ => break,
            },
            _ = cancel.cancelled() => {
                child.kill().await.ok();
                return Err(Error::Cancelled);
            }
        }
    }
    let status = tokio::select! {
        s = child.wait() => s?,
        _ = cancel.cancelled() => {
            child.kill().await.ok();
            return Err(Error::Cancelled);
        }
    };
    let stderr = err_task.await.unwrap_or_default();
    if !status.success() {
        let msg = stderr.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("exited with an error").to_string();
        return Err(Error::Ffmpeg(msg));
    }
    Ok(())
}

/// The duration ffmpeg reads back from a file it wrote, or `None` when it can't (a broken file).
pub async fn duration(ffmpeg: &Path, file: &Path) -> Option<f64> {
    let out = Command::new(ffmpeg).args(["-hide_banner", "-nostdin", "-i"]).arg(file).stdin(Stdio::null()).output().await.ok()?;
    parse_duration(&String::from_utf8_lossy(&out.stderr))
}

pub(crate) fn parse_duration(stderr: &str) -> Option<f64> {
    let d = stderr.split("Duration: ").nth(1)?.split(',').next()?.trim();
    let p: Vec<f64> = d.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let t = match p.as_slice() {
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        _ => return None,
    };
    (t > 0.0).then_some(t)
}

/// Run ffmpeg and make sure what it wrote is whole: ffmpeg can log a muxing error and still
/// exit 0 with a broken file, so the output's duration must match what was asked for.
pub async fn run_checked(ffmpeg: &Path, args: &[String], out: &Path, expected: f64, cancel: &CancellationToken, on_progress: impl FnMut(f64)) -> Result<()> {
    run(ffmpeg, args, cancel, on_progress).await?;
    let got = duration(ffmpeg, out).await;
    let slack = (expected * 0.02).max(1.0);
    match got {
        Some(d) if (d - expected).abs() <= slack => Ok(()),
        Some(d) => Err(Error::Ffmpeg(format!("the file came out {d:.1} s long instead of {expected:.1} s"))),
        None => Err(Error::Ffmpeg("the file it wrote can't be read back".into())),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn durations() {
        assert_eq!(super::parse_duration("  Duration: 00:03:33.06, start: 0.000000, bitrate"), Some(213.06));
        assert_eq!(super::parse_duration("  Duration: N/A, bitrate: N/A"), None);
        assert_eq!(super::parse_duration("  Duration: 00:00:00.00, start"), None);
        assert_eq!(super::parse_duration("nothing"), None);
    }

    use super::*;

    #[test]
    fn merge_and_cut_args() {
        let (v, a) = (PathBuf::from("/t/v.part"), PathBuf::from("/t/a.part"));
        let out = PathBuf::from("/t/out.mp4");
        let args = args(
            &[Input { path: &v, stream: 'v' }, Input { path: &a, stream: 'a' }],
            Some(&Cut { start: 61.5, end: 70.0 }),
            Some(&Metadata { title: "T", artist: "A", comment: "" }),
            &out,
        );
        let s = args.join(" ");
        assert!(s.contains("-seek_timestamp 1 -ss 61.500 -i /t/v.part -seek_timestamp 1 -ss 61.500 -i /t/a.part"), "{s}");
        assert!(s.contains("-map 0:v:0 -map 1:a:0 -t 8.500 -c copy"), "{s}");
        assert!(s.contains("-metadata title=T -metadata artist=A -strict experimental"), "{s}");
        assert!(!s.contains("comment="));
        assert!(s.ends_with("/t/out.mp4"));
    }

    #[test]
    fn audio_formats() {
        let a = PathBuf::from("a");
        let s = args(&[Input { path: &a, stream: 'a' }], None, None, Path::new("x.m4a")).join(" ");
        assert!(s.contains("-f ipod x.m4a"), "{s}");
        let s = args(&[Input { path: &a, stream: 'a' }], None, None, Path::new("x.opus")).join(" ");
        assert!(s.contains("-f opus x.opus"), "{s}");
    }
}

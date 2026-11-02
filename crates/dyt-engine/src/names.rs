//! File names: a title made safe for every filesystem we write to (ext4, FAT/exFAT SD cards,
//! Android's FUSE layer), and a free name next to existing files.

use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 180;

/// Make `title` a safe file stem: no path separators or characters FAT rejects, no control
/// characters, no leading dots or trailing dots/spaces, no reserved DOS names, bounded length.
pub fn sanitize(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_space = false;
    for c in title.chars() {
        let c = match c {
            '/' | '\\' | '|' => '-',
            ':' => '-',
            '<' | '>' | '"' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        };
        if c.is_whitespace() {
            if !last_space && !out.is_empty() {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    let mut s = out.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_string();
    if s.len() > MAX_BYTES {
        let mut end = MAX_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s = s.trim_end_matches(['.', ' ', '-']).to_string();
    }
    let upper = s.to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    if reserved.contains(&upper.as_str()) {
        s.push('_');
    }
    if s.is_empty() {
        "video".into()
    } else {
        s
    }
}

/// `dir/stem.ext`, or `dir/stem (2).ext`, `(3)`... if taken.
pub fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let name = |n: u32| if n == 1 { format!("{stem}.{ext}") } else { format!("{stem} ({n}).{ext}") };
    (1..10_000).map(|n| dir.join(name(n))).find(|p| !p.exists()).unwrap_or_else(|| dir.join(name(10_000)))
}

/// `1:02:03.4` style label for a time in seconds.
pub fn clock(t: f64) -> String {
    let t = t.max(0.0);
    let total_ms = (t * 1000.0).round() as u64;
    let (h, m, s, ms) = (total_ms / 3_600_000, total_ms / 60_000 % 60, total_ms / 1000 % 60, total_ms % 1000);
    let frac = if ms == 0 { String::new() } else { format!(".{}", format!("{ms:03}").trim_end_matches('0')) };
    if h > 0 {
        format!("{h}:{m:02}:{s:02}{frac}")
    } else {
        format!("{m}:{s:02}{frac}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize("AC/DC: Back in Black"), "AC-DC- Back in Black");
        assert_eq!(sanitize("  ..hidden?  "), "hidden");
        assert_eq!(sanitize("a\tb\nc   d"), "a b c d");
        assert_eq!(sanitize("what?.."), "what");
        assert_eq!(sanitize("con"), "con_");
        assert_eq!(sanitize("???"), "video");
        assert_eq!(sanitize("Café — 日本語 🎵"), "Café — 日本語 🎵");
        let long = "日".repeat(200);
        let s = sanitize(&long);
        assert!(s.len() <= MAX_BYTES && s.chars().all(|c| c == '日'));
    }

    #[test]
    fn unique_names() {
        let dir = std::env::temp_dir().join(format!("dyt-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique(&dir, "a", "mp4"), dir.join("a.mp4"));
        std::fs::write(dir.join("a.mp4"), b"").unwrap();
        assert_eq!(unique(&dir, "a", "mp4"), dir.join("a (2).mp4"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(61.5), "1:01.5");
        assert_eq!(clock(3723.25), "1:02:03.25");
    }
}

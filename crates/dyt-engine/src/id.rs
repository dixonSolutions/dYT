//! Turning whatever the user pasted into a video id (and an optional start time).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub video_id: String,
    /// `t=` / `start=` from the link, in seconds.
    pub start: Option<f64>,
}

pub fn is_video_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

const HOSTS: &[&str] = &[
    "youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "gaming.youtube.com",
    "youtube-nocookie.com",
    "youtu.be",
];

/// Parse a pasted link or bare id. Accepts watch, youtu.be, shorts, live, embed, v/ and e/
/// links, music and mobile hosts, attribution links, and links without a scheme.
pub fn parse(input: &str) -> Option<Target> {
    let s = input.trim().trim_matches(|c: char| c == '<' || c == '>' || c == '"' || c == '\'');
    if is_video_id(s) {
        return Some(Target { video_id: s.to_string(), start: None });
    }
    let with_scheme = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    let url = url::Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    if !HOSTS.contains(&host) {
        return None;
    }
    let query = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
    let start = query("t").or_else(|| query("start")).and_then(|t| parse_time_param(&t)).or_else(|| {
        // youtu.be links sometimes carry the time in the fragment (#t=1m2s).
        url.fragment().and_then(|f| f.strip_prefix("t=")).and_then(parse_time_param)
    });
    let segs: Vec<&str> = url.path_segments().map(|p| p.filter(|s| !s.is_empty()).collect()).unwrap_or_default();

    let id = if host == "youtu.be" {
        segs.first().map(|s| s.to_string())
    } else if let Some(v) = query("v") {
        Some(v)
    } else if let [kind, id, ..] = segs.as_slice() {
        match *kind {
            "shorts" | "live" | "embed" | "v" | "e" | "watch" => Some(id.to_string()),
            _ => None,
        }
    } else if segs.first() == Some(&"attribution_link") {
        // /attribution_link?u=/watch%3Fv%3DID%26feature...
        query("u").and_then(|u| parse(&format!("https://www.youtube.com{u}"))).map(|t| t.video_id)
    } else {
        None
    }?;
    // Ids are 11 chars; some share links append junk after them.
    let id: String = id.chars().take(11).collect();
    is_video_id(&id).then(|| Target { video_id: id, start })
}

/// `90`, `90s`, `1m30s`, `1h2m3s`, `01:30`, `1:02:03`, `90.5`.
pub fn parse_time_param(t: &str) -> Option<f64> {
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains(':') {
        return parse_clock(t);
    }
    if let Ok(v) = t.trim_end_matches('s').parse::<f64>() {
        return (v >= 0.0).then_some(v);
    }
    let mut total = 0.0;
    let mut num = String::new();
    for c in t.chars() {
        match c {
            '0'..='9' | '.' => num.push(c),
            'h' | 'm' | 's' => {
                let v: f64 = num.parse().ok()?;
                num.clear();
                total += v * match c {
                    'h' => 3600.0,
                    'm' => 60.0,
                    _ => 1.0,
                };
            }
            _ => return None,
        }
    }
    if !num.is_empty() {
        total += num.parse::<f64>().ok()?;
    }
    Some(total)
}

/// `ss`, `mm:ss`, `hh:mm:ss`, each optionally with a fraction (`1:02.5`).
pub fn parse_clock(t: &str) -> Option<f64> {
    let parts: Vec<&str> = t.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut total = 0.0;
    for (i, p) in parts.iter().enumerate() {
        let v: f64 = p.trim().parse().ok()?;
        if v < 0.0 || (i > 0 && v >= 60.0) {
            return None;
        }
        total = total * 60.0 + v;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Option<String> {
        parse(s).map(|t| t.video_id)
    }

    #[test]
    fn accepts_every_link_shape() {
        let want = Some("dQw4w9WgXcQ".to_string());
        for s in [
            "dQw4w9WgXcQ",
            "  dQw4w9WgXcQ  ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ&list=PL123&index=2",
            "http://m.youtube.com/watch?feature=share&v=dQw4w9WgXcQ",
            "www.youtube.com/watch?v=dQw4w9WgXcQ",
            "youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ?si=abcdef",
            "youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://youtube.com/shorts/dQw4w9WgXcQ?feature=share",
            "https://www.youtube.com/live/dQw4w9WgXcQ?si=x",
            "https://www.youtube.com/embed/dQw4w9WgXcQ?start=10",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/v/dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&feature=share",
            "<https://www.youtube.com/watch?v=dQw4w9WgXcQ>",
            "https://www.youtube.com/attribution_link?a=x&u=/watch%3Fv%3DdQw4w9WgXcQ%26feature%3Dshare",
        ] {
            assert_eq!(id(s), want, "{s}");
        }
    }

    #[test]
    fn rejects_other_links() {
        for s in [
            "",
            "hello",
            "https://vimeo.com/123456",
            "https://www.youtube.com/@RickAstleyYT",
            "https://www.youtube.com/playlist?list=PL123",
            "https://notyoutube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=short",
        ] {
            assert_eq!(id(s), None, "{s}");
        }
    }

    #[test]
    fn reads_start_times() {
        assert_eq!(parse("https://youtu.be/dQw4w9WgXcQ?t=43").unwrap().start, Some(43.0));
        assert_eq!(parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s").unwrap().start, Some(90.0));
        assert_eq!(parse("https://www.youtube.com/embed/dQw4w9WgXcQ?start=10").unwrap().start, Some(10.0));
        assert_eq!(parse_time_param("1h2m3s"), Some(3723.0));
        assert_eq!(parse_time_param("90s"), Some(90.0));
        assert_eq!(parse_time_param("1:02:03"), Some(3723.0));
        assert_eq!(parse_clock("2:05.5"), Some(125.5));
        assert_eq!(parse_clock("1:75"), None);
        assert_eq!(parse_time_param("abc"), None);
    }
}

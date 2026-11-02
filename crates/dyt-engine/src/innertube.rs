//! YouTube's InnerTube player API: client profiles, visitor sessions and response parsing.
//!
//! The engine asks clients that return plain stream URLs (no signature cipher, no JS challenge),
//! in order, until one answers with playable streams. Profiles are data (`ClientProfile`), so a
//! version bump on YouTube's side is a one-line change or a remote override, not a code change.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::model::{AudioTrack, Container, Format, StoryboardLevel, StreamKind, VideoInfo};

pub const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProfile {
    /// Our name for it (`visionos`), used in logs and `VideoInfo::client`.
    pub key: String,
    /// InnerTube `clientName`.
    pub name: String,
    /// `X-Youtube-Client-Name`.
    pub name_id: u32,
    pub version: String,
    pub user_agent: String,
    /// Extra fields merged into `context.client` (deviceMake, osName, androidSdkVersion, ...).
    #[serde(default)]
    pub context: serde_json::Map<String, Value>,
    #[serde(default = "default_host")]
    pub host: String,
    /// Extra fields for the request's top-level `context` (e.g. `thirdParty` for embeds).
    #[serde(default)]
    pub root_context: serde_json::Map<String, Value>,
    /// Needs the embed page's config (version, visitor, encrypted host flags) per video.
    #[serde(default)]
    pub embed: bool,
    /// Its URLs carry JS challenges (`n`, ciphered signatures) that must be solved first.
    #[serde(default)]
    pub requires_js: bool,
}

fn default_host() -> String {
    "www.youtube.com".into()
}

impl ClientProfile {
    fn new(key: &str, name: &str, name_id: u32, version: &str, ua: &str, context: Value) -> Self {
        ClientProfile {
            key: key.into(),
            name: name.into(),
            name_id,
            version: version.into(),
            user_agent: ua.into(),
            context: context.as_object().cloned().unwrap_or_default(),
            host: default_host(),
            root_context: Default::default(),
            embed: false,
            requires_js: false,
        }
    }
}

/// The built-in chain, best first. Only clients that hand out direct URLs belong here.
pub fn default_clients() -> Vec<ClientProfile> {
    vec![
        ClientProfile::new(
            "visionos",
            "VISIONOS",
            101,
            "1.02",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
            json!({
                "deviceMake": "Apple",
                "deviceModel": "RealityDevice17,1",
                "osName": "visionOS",
                "osVersion": "26.5.23O471",
            }),
        ),
        // Serves made-for-kids and many age-gated videos the first one refuses.
        ClientProfile {
            requires_js: true,
            ..ClientProfile::new(
                "tv_samsung",
                "TVHTML5",
                7,
                "5.20260707",
                "Mozilla/5.0 (SMART-TV; Linux; Tizen 2.4.0) AppleWebKit/538.1 (KHTML, like Gecko) Version/2.4.0 TV Safari/538.1",
                json!({
                    "deviceMake": "Samsung",
                    "deviceModel": "UKS9800",
                    "osName": "Tizen",
                    "osVersion": "2.4.0",
                }),
            )
        },
        // Embeddable age-gated videos play in third-party embeds without signing in.
        ClientProfile {
            requires_js: true,
            embed: true,
            root_context: json!({ "thirdParty": { "embedUrl": "https://www.reddit.com/" } }).as_object().cloned().unwrap_or_default(),
            ..ClientProfile::new(
                "web_embedded",
                "WEB_EMBEDDED_PLAYER",
                56,
                "2.20260708.00.00",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.5 Safari/605.1.15,gzip(gfe)",
                json!({}),
            )
        },
        ClientProfile::new(
            "android_vr",
            "ANDROID_VR",
            28,
            "1.65.10",
            "com.google.android.apps.youtube.vr.oculus/1.65.10 (Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip",
            json!({
                "deviceMake": "Oculus",
                "deviceModel": "Quest 3",
                "androidSdkVersion": 32,
                "osName": "Android",
                "osVersion": "12L",
            }),
        ),
    ]
}

/// A client list published as JSON (`clients.json` on the project site), checked before use:
/// every profile needs a name, a version and a user agent, and at least one must not need JS.
pub fn clients_from_json(json: &str) -> Result<Vec<ClientProfile>> {
    let list: Vec<ClientProfile> = serde_json::from_str(json).map_err(|e| Error::Parse(format!("clients.json: {e}")))?;
    let ok = !list.is_empty()
        && list.iter().all(|c| !c.key.is_empty() && !c.name.is_empty() && !c.version.is_empty() && !c.user_agent.is_empty() && c.name_id > 0)
        && list.iter().any(|c| !c.requires_js);
    if !ok {
        return Err(Error::Parse("clients.json: incomplete client list".into()));
    }
    Ok(list)
}

pub(crate) fn player_body(client: &ClientProfile, video_id: &str, visitor: Option<&str>, sts: Option<u32>) -> Value {
    let mut c = client.context.clone();
    c.insert("clientName".into(), json!(client.name));
    c.insert("clientVersion".into(), json!(client.version));
    c.insert("userAgent".into(), json!(client.user_agent));
    c.insert("hl".into(), json!("en"));
    c.insert("gl".into(), json!("US"));
    c.insert("timeZone".into(), json!("UTC"));
    c.insert("utcOffsetMinutes".into(), json!(0));
    if let Some(v) = visitor {
        c.insert("visitorData".into(), json!(v));
    }
    let mut playback = json!({ "html5Preference": "HTML5_PREF_WANTS" });
    if let Some(sts) = sts {
        playback["signatureTimestamp"] = json!(sts);
    }
    let mut context = json!({ "client": c, "request": { "useSsl": true }, "user": { "lockedSafetyMode": false } });
    for (k, v) in &client.root_context {
        context[k] = v.clone();
    }
    json!({
        "context": context,
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
        "playbackContext": { "contentPlaybackContext": playback },
    })
}

/// What the embed page hands its player; without the host flags embeds are refused.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct EmbedConfig {
    pub version: Option<String>,
    pub visitor: Option<String>,
    pub host_flags: Option<String>,
    pub encrypted_context: Option<String>,
}

fn json_str_after(html: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let i = html.find(&needle)? + needle.len();
    let rest = html[i..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string()).filter(|s| !s.is_empty())
}

pub(crate) fn parse_embed_config(html: &str) -> EmbedConfig {
    EmbedConfig {
        version: json_str_after(html, "INNERTUBE_CLIENT_VERSION"),
        visitor: json_str_after(html, "VISITOR_DATA"),
        host_flags: json_str_after(html, "encryptedHostFlags"),
        encrypted_context: json_str_after(html, "embeddedPlayerEncryptedContext"),
    }
}

/// Fold the embed config into a player request body.
pub(crate) fn apply_embed_config(body: &mut Value, cfg: &EmbedConfig) {
    if let Some(v) = &cfg.version {
        body["context"]["client"]["clientVersion"] = json!(v);
    }
    if let Some(v) = &cfg.visitor {
        body["context"]["client"]["visitorData"] = json!(v);
    }
    if let Some(f) = &cfg.host_flags {
        body["playbackContext"]["contentPlaybackContext"]["encryptedHostFlags"] = json!(f);
    }
    if let Some(c) = &cfg.encrypted_context {
        body["context"]["thirdParty"]["embeddedPlayerContext"] = json!({ "embeddedPlayerEncryptedContext": c, "ancestorOriginsSupported": false });
    }
}

/// `sw.js_data` is a small JSON blob that carries a fresh visitor id; much cheaper than a page.
pub(crate) fn parse_sw_js_data(body: &str) -> Option<String> {
    let json = body.trim_start().strip_prefix(")]}'").unwrap_or(body).trim();
    let v: Value = serde_json::from_str(json).ok()?;
    let s = v.pointer("/0/2/0/0/13")?.as_str()?;
    (!s.is_empty()).then(|| s.to_string())
}

pub(crate) fn parse_page_visitor(html: &str) -> Option<String> {
    let key = "\"VISITOR_DATA\":\"";
    let i = html.find(key)? + key.len();
    let end = html[i..].find('"')?;
    Some(html[i..i + end].to_string())
}

fn str_at<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    v.pointer(path).and_then(Value::as_str)
}

fn num(v: &Value, key: &str) -> Option<u64> {
    match v.get(key)? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn range(v: &Value, key: &str) -> Option<(u64, u64)> {
    let r = v.get(key)?;
    Some((num(r, "start")?, num(r, "end")?))
}

/// `video/webm; codecs="vp9"` -> (`video/webm`, `vp9`).
pub(crate) fn split_mime(mime: &str) -> (String, String) {
    let mut parts = mime.splitn(2, ';');
    let base = parts.next().unwrap_or("").trim().to_string();
    let codecs = parts
        .next()
        .and_then(|p| p.split_once("codecs="))
        .map(|(_, c)| c.trim().trim_matches('"').to_string())
        .unwrap_or_default();
    (base, codecs)
}

pub(crate) fn codec_family(codecs: &str) -> &'static str {
    let c = codecs.split(',').next().unwrap_or("").trim().to_ascii_lowercase();
    if c.starts_with("avc1") || c.starts_with("avc3") {
        "avc1"
    } else if c.starts_with("vp9") || c.starts_with("vp09") {
        "vp9"
    } else if c.starts_with("av01") {
        "av1"
    } else if c.starts_with("hev1") || c.starts_with("hvc1") {
        "hevc"
    } else if c.starts_with("mp4a") {
        "mp4a"
    } else if c.starts_with("opus") {
        "opus"
    } else if c.starts_with("vorbis") {
        "vorbis"
    } else if c.starts_with("ec-3") || c.starts_with("ac-3") {
        "ac3"
    } else {
        "other"
    }
}

fn parse_format(f: &Value, muxed: bool) -> Option<Format> {
    // Ciphered formats need the web player's JS; the clients we use don't return them.
    let url = f.get("url")?.as_str()?.to_string();
    // OTF ("on the fly") streams are segmented and can't be range-fetched like the others.
    if f.get("type").and_then(Value::as_str) == Some("FORMAT_STREAM_TYPE_OTF") {
        return None;
    }
    let mime = f.get("mimeType")?.as_str()?.to_string();
    let (base, codecs) = split_mime(&mime);
    let container = if base.ends_with("/mp4") {
        Container::Mp4
    } else if base.ends_with("/webm") {
        Container::Webm
    } else {
        Container::Other
    };
    let kind = if muxed {
        StreamKind::Muxed
    } else if base.starts_with("audio/") {
        StreamKind::Audio
    } else {
        StreamKind::Video
    };
    let transfer = str_at(f, "/colorInfo/transferCharacteristics").unwrap_or("");
    let label = f.get("qualityLabel").and_then(Value::as_str).map(str::to_string);
    let hdr = transfer.contains("SMPTEST2084") || transfer.contains("ARIB_STD_B67") || label.as_deref().is_some_and(|l| l.contains("HDR"));
    let drc = f.get("isDrc").and_then(Value::as_bool).unwrap_or(false) || url.contains("drc%3D1") || url.contains("xtags=drc");
    let audio_track = f.get("audioTrack").map(|t| AudioTrack {
        id: str_at(t, "/id").unwrap_or("").to_string(),
        name: str_at(t, "/displayName").unwrap_or("").to_string(),
        is_default: t.get("audioIsDefault").and_then(Value::as_bool).unwrap_or(false),
    });
    Some(Format {
        itag: num(f, "itag")? as u32,
        kind,
        container,
        codec: codec_family(&codecs).to_string(),
        codecs,
        mime,
        width: num(f, "width").map(|v| v as u32),
        height: num(f, "height").map(|v| v as u32),
        fps: num(f, "fps").map(|v| v as u32),
        hdr,
        bitrate: num(f, "averageBitrate").or_else(|| num(f, "bitrate")).unwrap_or(0),
        content_length: num(f, "contentLength").filter(|&n| n > 0),
        duration: num(f, "approxDurationMs").map(|ms| ms as f64 / 1000.0),
        quality_label: label,
        audio_sample_rate: num(f, "audioSampleRate").map(|v| v as u32),
        audio_channels: num(f, "audioChannels").map(|v| v as u32),
        audio_track,
        drc,
        init_range: range(f, "initRange"),
        index_range: range(f, "indexRange"),
        url,
    })
}

pub(crate) fn parse_storyboard(spec: &str, duration: f64) -> Vec<StoryboardLevel> {
    let mut parts = spec.split('|');
    let Some(base) = parts.next() else { return vec![] };
    let mut out = vec![];
    for (level, p) in parts.enumerate() {
        let f: Vec<&str> = p.split('#').collect();
        if f.len() < 8 {
            continue;
        }
        let (Ok(width), Ok(height), Ok(count), Ok(cols), Ok(rows), Ok(mut interval_ms)) =
            (f[0].parse(), f[1].parse(), f[2].parse(), f[3].parse(), f[4].parse(), f[5].parse::<u32>())
        else {
            continue;
        };
        if count == 0 || cols == 0 || rows == 0 {
            continue;
        }
        if interval_ms == 0 && duration > 0.0 {
            interval_ms = (duration * 1000.0 / count as f64).ceil() as u32;
        }
        let url = format!("{}&sigh={}", base.replace("$L", &level.to_string()).replace("$N", f[6]), f[7]);
        out.push(StoryboardLevel { url, width, height, count, cols, rows, interval_ms });
    }
    out
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs()
}

/// Map a non-OK playability status to the error the user should see.
pub(crate) fn playability_error(v: &Value) -> Option<Error> {
    let ps = v.get("playabilityStatus")?;
    let status = ps.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "OK" {
        return None;
    }
    let reason = ps
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| str_at(ps, "/errorScreen/playerErrorMessageRenderer/subreason/simpleText").map(str::to_string))
        .unwrap_or_else(|| status.to_lowercase().replace('_', " "));
    let lower = reason.to_lowercase();
    let details = v.get("videoDetails");
    let flag = |k: &str| details.and_then(|d| d.get(k)).and_then(Value::as_bool).unwrap_or(false);
    if flag("isLive") || (flag("isLiveContent") && lower.contains("live stream") && status != "LIVE_STREAM_OFFLINE") {
        return Some(Error::Live);
    }
    Some(match status {
        "LOGIN_REQUIRED" if lower.contains("bot") => Error::BotCheck,
        "LOGIN_REQUIRED" | "AGE_CHECK_REQUIRED" | "AGE_VERIFICATION_REQUIRED" | "CONTENT_CHECK_REQUIRED"
            if lower.contains("age") || lower.contains("inappropriate") || status.starts_with("AGE") =>
        {
            Error::AgeRestricted
        }
        "LIVE_STREAM_OFFLINE" => Error::Unavailable(reason),
        _ => Error::Unavailable(reason),
    })
}

/// Turn a player response into `VideoInfo`. Fails when it has nothing we can download.
pub fn parse_player(v: &Value, video_id: &str, client: &str) -> Result<VideoInfo> {
    if let Some(e) = playability_error(v) {
        return Err(e);
    }
    let vd = v.get("videoDetails").ok_or_else(|| Error::Parse("no videoDetails".into()))?;
    if vd.get("videoId").and_then(Value::as_str).is_some_and(|id| id != video_id) {
        // YouTube occasionally serves a placeholder video to suspected bots.
        return Err(Error::BotCheck);
    }
    let is_live = vd.get("isLive").and_then(Value::as_bool).unwrap_or(false);
    let sd = v.get("streamingData").ok_or_else(|| if is_live { Error::Live } else { Error::NoFormat })?;
    let mut formats: Vec<Format> = vec![];
    for (key, muxed) in [("adaptiveFormats", false), ("formats", true)] {
        if let Some(list) = sd.get(key).and_then(Value::as_array) {
            formats.extend(list.iter().filter_map(|f| parse_format(f, muxed)));
        }
    }
    if formats.is_empty() {
        return Err(if is_live { Error::Live } else { Error::NoFormat });
    }
    let duration = num(vd, "lengthSeconds")
        .map(|s| s as f64)
        .filter(|&d| d > 0.0)
        .or_else(|| formats.iter().filter_map(|f| f.duration).fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.max(d)))))
        .unwrap_or(0.0);
    let expires_at = num(sd, "expiresInSeconds").map(|s| now_secs() + s).unwrap_or_else(|| now_secs() + 5 * 3600);
    let thumbnail = vd
        .pointer("/thumbnail/thumbnails")
        .and_then(Value::as_array)
        .and_then(|t| t.iter().max_by_key(|t| num(t, "width").unwrap_or(0)))
        .and_then(|t| t.get("url").and_then(Value::as_str))
        .map(|u| u.split('?').next().unwrap_or(u).to_string())
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"));
    let storyboard = str_at(v, "/storyboards/playerStoryboardSpecRenderer/spec")
        .map(|s| parse_storyboard(s, duration))
        .unwrap_or_default();
    Ok(VideoInfo {
        id: video_id.to_string(),
        title: vd.get("title").and_then(Value::as_str).unwrap_or(video_id).to_string(),
        author: vd.get("author").and_then(Value::as_str).unwrap_or("").to_string(),
        channel_id: vd.get("channelId").and_then(Value::as_str).unwrap_or("").to_string(),
        duration,
        view_count: num(vd, "viewCount"),
        is_live,
        thumbnail,
        formats,
        expires_at,
        client: client.to_string(),
        storyboard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_lists_round_trip_and_are_checked() {
        let json = serde_json::to_string(&default_clients()).unwrap();
        assert_eq!(clients_from_json(&json).unwrap(), default_clients());
        assert!(clients_from_json("[]").is_err());
        assert!(clients_from_json("not json").is_err());
        let js_only: Vec<ClientProfile> = default_clients().into_iter().filter(|c| c.requires_js).collect();
        assert!(clients_from_json(&serde_json::to_string(&js_only).unwrap()).is_err());
    }

    #[test]
    fn embed_config() {
        let html = r#"ytcfg.set({"INNERTUBE_CLIENT_VERSION":"2.20261029.11.00","VISITOR_DATA":"Cgt","WEB_PLAYER_CONTEXT_CONFIGS":{"X":{"encryptedHostFlags":"AAu","embeddedPlayerEncryptedContext" : "BBv"}}});"#;
        let cfg = parse_embed_config(html);
        assert_eq!(cfg.version.as_deref(), Some("2.20261029.11.00"));
        assert_eq!(cfg.host_flags.as_deref(), Some("AAu"));
        assert_eq!(cfg.encrypted_context.as_deref(), Some("BBv"));
        let client = default_clients().into_iter().find(|c| c.embed).unwrap();
        let mut body = player_body(&client, "abcdefghijk", Some("old"), Some(20000));
        apply_embed_config(&mut body, &cfg);
        assert_eq!(body["context"]["client"]["clientVersion"], "2.20261029.11.00");
        assert_eq!(body["context"]["client"]["visitorData"], "Cgt");
        assert_eq!(body["context"]["thirdParty"]["embedUrl"], "https://www.reddit.com/");
        assert_eq!(body["context"]["thirdParty"]["embeddedPlayerContext"]["embeddedPlayerEncryptedContext"], "BBv");
        assert_eq!(body["playbackContext"]["contentPlaybackContext"]["encryptedHostFlags"], "AAu");
        assert_eq!(body["playbackContext"]["contentPlaybackContext"]["signatureTimestamp"], 20000);
    }

    #[test]
    fn mime_and_codecs() {
        assert_eq!(split_mime(r#"video/webm; codecs="vp9""#), ("video/webm".into(), "vp9".into()));
        assert_eq!(split_mime(r#"video/mp4; codecs="avc1.640028""#).1, "avc1.640028");
        assert_eq!(codec_family("av01.0.08M.08"), "av1");
        assert_eq!(codec_family("vp09.00.40.08"), "vp9");
        assert_eq!(codec_family("mp4a.40.2"), "mp4a");
        assert_eq!(codec_family("opus"), "opus");
    }

    #[test]
    fn visitor_from_sw_js_data() {
        let body = r#")]}'
[["x",null,[[[null,null,null,null,null,null,null,null,null,null,null,null,null,"CgtABCDEF"]]]]]"#;
        assert_eq!(parse_sw_js_data(body).as_deref(), Some("CgtABCDEF"));
        assert_eq!(parse_page_visitor(r#"..."VISITOR_DATA":"Cgt123","x":1"#).as_deref(), Some("Cgt123"));
    }

    #[test]
    fn storyboard_levels() {
        let spec = "https://i.ytimg.com/sb/ID/storyboard3_L$L/$N.jpg?sqp=abc|48#27#100#10#10#0#default#rs$A|80#45#108#10#10#2000#M$M#rs$B";
        let l = parse_storyboard(spec, 213.0);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].url, "https://i.ytimg.com/sb/ID/storyboard3_L0/default.jpg?sqp=abc&sigh=rs$A");
        assert_eq!(l[0].interval_ms, 2130);
        assert_eq!(l[1].url, "https://i.ytimg.com/sb/ID/storyboard3_L1/M$M.jpg?sqp=abc&sigh=rs$B");
        assert_eq!((l[1].width, l[1].interval_ms), (80, 2000));
    }

    #[test]
    fn playability_errors() {
        let bot = json!({"playabilityStatus": {"status": "LOGIN_REQUIRED", "reason": "Sign in to confirm you’re not a bot"}});
        assert!(matches!(playability_error(&bot), Some(Error::BotCheck)));
        let age = json!({"playabilityStatus": {"status": "LOGIN_REQUIRED", "reason": "Sign in to confirm your age"}});
        assert!(matches!(playability_error(&age), Some(Error::AgeRestricted)));
        let gone = json!({"playabilityStatus": {"status": "ERROR", "reason": "Video unavailable"}});
        assert!(matches!(playability_error(&gone), Some(Error::Unavailable(r)) if r == "Video unavailable"));
        assert!(playability_error(&json!({"playabilityStatus": {"status": "OK"}})).is_none());
        let live = json!({"playabilityStatus": {"status": "UNPLAYABLE", "reason": "This live stream recording is not available."}, "videoDetails": {"isLiveContent": true}});
        assert!(matches!(playability_error(&live), Some(Error::Live)));
    }

    #[test]
    fn parses_a_player_response() {
        let v = json!({
            "playabilityStatus": {"status": "OK"},
            "videoDetails": {"videoId": "abcdefghijk", "title": "T", "author": "A", "channelId": "C", "lengthSeconds": "60", "viewCount": "5",
                "thumbnail": {"thumbnails": [{"url": "https://i.ytimg.com/vi/x/default.jpg?a=1", "width": 120}, {"url": "https://i.ytimg.com/vi/x/maxres.jpg", "width": 1280}]}},
            "streamingData": {"expiresInSeconds": "21540", "adaptiveFormats": [
                {"itag": 251, "url": "https://r/videoplayback?a=1", "mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 140000, "averageBitrate": 130000,
                 "contentLength": "1000", "initRange": {"start": "0", "end": "258"}, "indexRange": {"start": "259", "end": "629"}, "audioSampleRate": "48000", "audioChannels": 2},
                {"itag": 137, "signatureCipher": "s=abc&url=x", "mimeType": "video/mp4; codecs=\"avc1.640028\""},
                {"itag": 299, "url": "https://r/v", "type": "FORMAT_STREAM_TYPE_OTF", "mimeType": "video/mp4; codecs=\"avc1.64002a\""},
                {"itag": 399, "url": "https://r/v2", "mimeType": "video/mp4; codecs=\"av01.0.08M.08\"", "width": 1920, "height": 1080, "fps": 30,
                 "qualityLabel": "1080p", "bitrate": 1000, "colorInfo": {"transferCharacteristics": "COLOR_TRANSFER_CHARACTERISTICS_BT709"}}
            ]}
        });
        let info = parse_player(&v, "abcdefghijk", "visionos").unwrap();
        assert_eq!(info.formats.len(), 2, "ciphered and OTF formats are skipped");
        assert_eq!(info.thumbnail, "https://i.ytimg.com/vi/x/maxres.jpg");
        assert_eq!(info.duration, 60.0);
        let a = info.format(251).unwrap();
        assert_eq!((a.kind, a.container, a.codec.as_str()), (StreamKind::Audio, Container::Webm, "opus"));
        assert_eq!(a.index_range, Some((259, 629)));
        let v = info.format(399).unwrap();
        assert_eq!((v.height, v.fps, v.hdr, v.codec.as_str()), (Some(1080), Some(30), false, "av1"));
    }

    #[test]
    fn wrong_video_is_a_bot_check() {
        let v = json!({"playabilityStatus": {"status": "OK"}, "videoDetails": {"videoId": "zzzzzzzzzzz"}});
        assert!(matches!(parse_player(&v, "abcdefghijk", "x"), Err(Error::BotCheck)));
    }
}

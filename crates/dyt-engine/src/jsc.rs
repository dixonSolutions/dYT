//! YouTube's JS challenges (the `n` throttling parameter and ciphered signatures), solved
//! natively in an embedded QuickJS with the yt-dlp-ejs solver (public domain; see js/).
//!
//! Only the fallback clients need this. Solutions are cached per player version, so a player
//! is parsed once per session however many videos go through it.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::error::{Error, Result};

const LIB: &str = include_str!("../js/lib.min.js");
const CORE: &str = include_str!("../js/core.min.js");

/// The player id (`11a45ffb`) from the iframe API script.
pub(crate) fn parse_player_id(iframe_api: &str) -> Option<String> {
    let i = iframe_api.find("/s/player/").map(|i| i + 10).or_else(|| iframe_api.find("player\\/").map(|i| i + 8))?;
    let id: String = iframe_api[i..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    (id.len() >= 8).then_some(id)
}

pub(crate) fn player_url(id: &str) -> String {
    format!("https://www.youtube.com/s/player/{id}/player_ias.vflset/en_US/base.js")
}

/// `signatureTimestamp` baked into the player; the API wants it with ciphered requests.
pub(crate) fn parse_sts(code: &str) -> Option<u32> {
    for key in ["signatureTimestamp:", "sts:"] {
        let mut from = 0;
        while let Some(i) = code[from..].find(key) {
            let rest = code[from + i + key.len()..].trim_start();
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.len() == 5 {
                return digits.parse().ok();
            }
            from += i + key.len();
        }
    }
    None
}

#[derive(Default)]
pub(crate) struct Solutions {
    pub n: HashMap<String, String>,
    pub sig: HashMap<String, String>,
    /// The solver's reduced copy of the player; parsing it is far cheaper than the original.
    pub preprocessed: Option<String>,
}

/// The player in the form the solver takes: the full script the first time, then the
/// preprocessed one it handed back.
pub(crate) enum PlayerInput<'a> {
    Full(&'a str),
    Preprocessed(&'a str),
}

/// Run the solver for every challenge. Blocking; call from a blocking thread.
pub(crate) fn solve(player: PlayerInput, n: &[String], sig: &[String]) -> Result<Solutions> {
    let mut requests = vec![];
    if !n.is_empty() {
        requests.push(json!({"type": "n", "challenges": n}));
    }
    if !sig.is_empty() {
        requests.push(json!({"type": "sig", "challenges": sig}));
    }
    if requests.is_empty() {
        return Ok(Solutions::default());
    }
    let input = match player {
        PlayerInput::Full(code) => json!({"type": "player", "player": code, "requests": requests, "output_preprocessed": true}),
        PlayerInput::Preprocessed(code) => json!({"type": "preprocessed", "preprocessed_player": code, "requests": requests}),
    };
    let script = format!("{LIB}\nObject.assign(globalThis, lib);\n{CORE}\nJSON.stringify(jsc({input}));");
    let rt = rquickjs::Runtime::new().map_err(|e| Error::Parse(format!("js runtime: {e}")))?;
    rt.set_memory_limit(768 * 1024 * 1024);
    rt.set_max_stack_size(8 * 1024 * 1024);
    let ctx = rquickjs::Context::full(&rt).map_err(|e| Error::Parse(format!("js context: {e}")))?;
    let out: String = ctx.with(|ctx| {
        ctx.eval::<String, _>(script.as_bytes()).map_err(|e| {
            let detail = ctx.catch().as_exception().map(|ex| ex.message().unwrap_or_default()).unwrap_or_else(|| e.to_string());
            Error::Parse(format!("js challenge: {detail}"))
        })
    })?;
    let v: Value = serde_json::from_str(&out).map_err(|e| Error::Parse(format!("js challenge output: {e}")))?;
    if v["type"] != "result" {
        return Err(Error::Parse(format!("js challenge: {}", v["error"])));
    }
    let mut sol = Solutions { preprocessed: v["preprocessed_player"].as_str().map(str::to_string), ..Default::default() };
    let kinds: Vec<&str> = [(!n.is_empty()).then_some("n"), (!sig.is_empty()).then_some("sig")].into_iter().flatten().collect();
    for (kind, resp) in kinds.iter().zip(v["responses"].as_array().into_iter().flatten()) {
        if resp["type"] != "result" {
            log::warn!("{kind} challenge failed: {}", resp["error"]);
            continue;
        }
        let map = if *kind == "n" { &mut sol.n } else { &mut sol.sig };
        for (k, val) in resp["data"].as_object().into_iter().flatten() {
            if let Some(s) = val.as_str() {
                map.insert(k.clone(), s.to_string());
            }
        }
    }
    Ok(sol)
}

/// Decoded `signatureCipher`: the ciphered signature, the param it goes in, and the base URL.
pub(crate) fn parse_cipher(cipher: &str) -> Option<(String, String, String)> {
    let mut s = None;
    let mut sp = "signature".to_string();
    let mut url = None;
    for (k, v) in url::form_urlencoded::parse(cipher.as_bytes()) {
        match k.as_ref() {
            "s" => s = Some(v.into_owned()),
            "sp" => sp = v.into_owned(),
            "url" => url = Some(v.into_owned()),
            _ => {}
        }
    }
    Some((s?, sp, url?))
}

pub(crate) fn query_param(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    url::form_urlencoded::parse(q.as_bytes()).find(|(k, _)| k == key).map(|(_, v)| v.into_owned())
}

pub(crate) fn set_query_param(url: &str, key: &str, value: &str) -> String {
    let Ok(mut u) = url::Url::parse(url) else { return url.to_string() };
    let pairs: Vec<(String, String)> = u.query_pairs().filter(|(k, _)| k != key).map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    {
        let mut q = u.query_pairs_mut();
        q.clear();
        for (k, v) in &pairs {
            q.append_pair(k, v);
        }
        q.append_pair(key, value);
    }
    u.to_string()
}

/// Remembers solutions per player so repeat challenges never hit the JS engine.
#[derive(Default)]
pub(crate) struct SolutionCache {
    inner: Mutex<HashMap<String, Solutions>>,
}

impl SolutionCache {
    pub fn split<'a>(&self, player: &str, n: &'a [String], sig: &'a [String]) -> (Vec<String>, Vec<String>) {
        let guard = self.inner.lock().unwrap();
        let known = guard.get(player);
        let miss = |list: &[String], f: fn(&Solutions) -> &HashMap<String, String>| -> Vec<String> {
            let mut v: Vec<String> = list.iter().filter(|c| known.is_none_or(|k| !f(k).contains_key(*c))).cloned().collect();
            v.sort();
            v.dedup();
            v
        };
        (miss(n, |s| &s.n), miss(sig, |s| &s.sig))
    }

    pub fn add(&self, player: &str, s: Solutions) {
        let mut guard = self.inner.lock().unwrap();
        let e = guard.entry(player.to_string()).or_default();
        e.n.extend(s.n);
        e.sig.extend(s.sig);
        if s.preprocessed.is_some() {
            e.preprocessed = s.preprocessed;
        }
    }

    pub fn preprocessed(&self, player: &str) -> Option<String> {
        self.inner.lock().unwrap().get(player)?.preprocessed.clone()
    }

    pub fn n(&self, player: &str, c: &str) -> Option<String> {
        self.inner.lock().unwrap().get(player)?.n.get(c).cloned()
    }

    pub fn sig(&self, player: &str, c: &str) -> Option<String> {
        self.inner.lock().unwrap().get(player)?.sig.get(c).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_id_and_sts() {
        assert_eq!(parse_player_id(r#"var scriptUrl = 'https:\/\/www.youtube.com\/s\/player\/11a45ffb\/www-widgetapi.vflset\/www-widgetapi.js';"#).as_deref(), Some("11a45ffb"));
        assert_eq!(parse_sts("x={signatureTimestamp:20391,y:1}"), Some(20391));
        assert_eq!(parse_sts("sts:123,b;sts: 20400"), Some(20400));
        assert_eq!(parse_sts("nothing"), None);
    }

    #[test]
    fn cipher_and_params() {
        let (s, sp, url) = parse_cipher("s=AB%3DCD&sp=sig&url=https%3A%2F%2Fr.googlevideo.com%2Fvideoplayback%3Fn%3Dxyz%26a%3D1").unwrap();
        assert_eq!((s.as_str(), sp.as_str()), ("AB=CD", "sig"));
        assert_eq!(query_param(&url, "n").as_deref(), Some("xyz"));
        let u = set_query_param(&url, "n", "solved");
        assert_eq!(query_param(&u, "n").as_deref(), Some("solved"));
        assert_eq!(query_param(&u, "a").as_deref(), Some("1"));
    }

    #[test]
    fn solver_runs_in_quickjs() {
        // A minimal fake player is not something the solver understands; it must fail cleanly.
        let r = solve(PlayerInput::Full("var x = 1;"), &["abc".into()], &[]);
        assert!(r.is_err() || r.unwrap().n.is_empty());
    }
}

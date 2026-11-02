//! The engine: extraction with fallbacks and caching, and download jobs end to end.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::fetch::{self, FetchOptions, Piece, Refresh, Target};
use crate::ffmpeg::{self, Cut, Input, Metadata};
use crate::index::{self, Segment};
use crate::innertube::{self, ClientProfile};
use crate::model::{Container, Format, VideoInfo};
use crate::{id, jsc, names, select};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub fetch: FetchOptions,
    /// Extra places to look for ffmpeg (e.g. Android's native library dir).
    pub ffmpeg_dirs: Vec<PathBuf>,
    pub clients: Vec<ClientProfile>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig { fetch: FetchOptions::default(), ffmpeg_dirs: vec![], clients: innertube::default_clients() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    /// A link or id.
    pub url: String,
    /// itag of the video stream, if video is wanted.
    pub video: Option<u32>,
    /// itag of the audio stream, if audio is wanted.
    pub audio: Option<u32>,
    /// `[start, end]` in seconds; `None` downloads everything.
    pub clip: Option<(f64, f64)>,
    /// Keep the streams as they come (separate files, original containers) instead of merging.
    #[serde(default)]
    pub raw: bool,
    pub dir: PathBuf,
    /// File name without extension; defaults to the title.
    pub file_name: Option<String>,
    /// Container for merged output (`mp4`, `mkv`, `webm`...); chosen from the codecs if unset.
    pub ext: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Preparing,
    Downloading,
    Merging,
    Done,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
    /// Bytes per second (smoothed).
    pub speed: f64,
    /// Seconds left, when it can be estimated.
    pub eta: Option<f64>,
    /// 0..1 across the whole job.
    pub fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub files: Vec<PathBuf>,
    pub bytes: u64,
    pub seconds: f64,
}

struct Inner {
    api: reqwest::Client,
    media: reqwest::Client,
    config: EngineConfig,
    clients: RwLock<Vec<ClientProfile>>,
    visitor: Mutex<Option<(String, Instant)>>,
    cache: Mutex<HashMap<String, VideoInfo>>,
    /// Current web player: (id, code, signature timestamp, fetched at).
    player_js: tokio::sync::Mutex<Option<(String, Arc<String>, u32, Instant)>>,
    solutions: jsc::SolutionCache,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

fn tls_roots() -> Vec<reqwest::Certificate> {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter().filter_map(|c| reqwest::Certificate::from_der(c.as_ref()).ok()).collect()
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs()
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Engine> {
        // Bundled roots and ring: identical TLS on Linux, Flatpak and Android, no system deps.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let api = reqwest::Client::builder()
            .tls_certs_only(tls_roots())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .gzip(true)
            .brotli(true)
            .build()
            .map_err(Error::from)?;
        // HTTP/1.1 on purpose: one TCP connection per worker is what makes the downloads
        // parallel. With HTTP/2 every chunk would share (and queue on) a single connection.
        let media = reqwest::Client::builder()
            .tls_certs_only(tls_roots())
            .http1_only()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(32)
            .tcp_nodelay(true)
            .build()
            .map_err(Error::from)?;
        let clients = config.clients.clone();
        Ok(Engine {
            inner: Arc::new(Inner {
                api,
                media,
                config,
                clients: RwLock::new(clients),
                visitor: Mutex::new(None),
                cache: Mutex::new(HashMap::new()),
                player_js: tokio::sync::Mutex::new(None),
                solutions: jsc::SolutionCache::default(),
            }),
        })
    }

    pub fn ffmpeg(&self) -> Option<PathBuf> {
        ffmpeg::locate(&self.inner.config.ffmpeg_dirs)
    }

    /// Replace the client chain (e.g. with a newer list fetched at startup).
    pub fn set_clients(&self, clients: Vec<ClientProfile>) {
        if !clients.is_empty() {
            *self.inner.clients.write().unwrap() = clients;
        }
    }

    /// A small text resource over the engine's own TLS setup (used for the published client list).
    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        let r = self.inner.api.get(url).timeout(Duration::from_secs(8)).send().await?;
        let status = r.status().as_u16();
        if status != 200 {
            return Err(Error::Http { status, what: format!("fetching {url}") });
        }
        Ok(r.text().await?)
    }

    pub fn clients(&self) -> Vec<ClientProfile> {
        self.inner.clients.read().unwrap().clone()
    }

    async fn visitor(&self, force: bool) -> Option<String> {
        if !force {
            if let Some((v, at)) = self.inner.visitor.lock().unwrap().as_ref() {
                if at.elapsed() < Duration::from_secs(3600) {
                    return Some(v.clone());
                }
            }
        }
        let fetch = async {
            let sw = self.inner.api.get("https://www.youtube.com/sw.js_data").header("User-Agent", innertube::WEB_UA).send().await.ok()?;
            let body = sw.text().await.ok()?;
            innertube::parse_sw_js_data(&body)
        };
        let v = match fetch.await {
            Some(v) => Some(v),
            None => {
                let page = self
                    .inner
                    .api
                    .get("https://www.youtube.com/")
                    .header("User-Agent", innertube::WEB_UA)
                    .header("Cookie", "PREF=hl=en&tz=UTC; SOCS=CAI")
                    .send()
                    .await
                    .ok()?;
                innertube::parse_page_visitor(&page.text().await.ok()?)
            }
        };
        if let Some(v) = &v {
            *self.inner.visitor.lock().unwrap() = Some((v.clone(), Instant::now()));
        }
        v
    }

    /// The current web player script and its signature timestamp, refreshed hourly.
    async fn player_js(&self) -> Result<(String, Arc<String>, u32)> {
        let mut guard = self.inner.player_js.lock().await;
        if let Some((id, code, sts, at)) = guard.as_ref() {
            if at.elapsed() < Duration::from_secs(3600) {
                return Ok((id.clone(), code.clone(), *sts));
            }
        }
        let api = self.inner.api.get("https://www.youtube.com/iframe_api").header("User-Agent", innertube::WEB_UA).send().await?.text().await?;
        let id = jsc::parse_player_id(&api).ok_or_else(|| Error::Parse("no player id in iframe_api".into()))?;
        if let Some((old, code, sts, _)) = guard.as_ref() {
            if *old == id {
                let r = (id.clone(), code.clone(), *sts);
                *guard = Some((id, code.clone(), *sts, Instant::now()));
                return Ok(r);
            }
        }
        let resp = self.inner.api.get(jsc::player_url(&id)).header("User-Agent", innertube::WEB_UA).timeout(Duration::from_secs(60)).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Http { status: resp.status().as_u16(), what: "fetching the web player".into() });
        }
        let code = Arc::new(resp.text().await?);
        let sts = jsc::parse_sts(&code).ok_or_else(|| Error::Parse("no signature timestamp in player".into()))?;
        *guard = Some((id.clone(), code.clone(), sts, Instant::now()));
        Ok((id, code, sts))
    }

    /// Rewrite every format's URL in a player response so it needs no further JS: decipher
    /// `signatureCipher` and replace the `n` parameter with its solution.
    async fn solve_challenges(&self, v: &mut Value, player_id: &str, code: Arc<String>) -> Result<()> {
        let mut ns = vec![];
        let mut sigs = vec![];
        let lists = ["/streamingData/adaptiveFormats", "/streamingData/formats"];
        for path in lists {
            for f in v.pointer(path).and_then(Value::as_array).into_iter().flatten() {
                let url = match (f.get("url").and_then(Value::as_str), f.get("signatureCipher").and_then(Value::as_str)) {
                    (Some(u), _) => u.to_string(),
                    (None, Some(c)) => match jsc::parse_cipher(c) {
                        Some((s, _, u)) => {
                            sigs.push(s);
                            u
                        }
                        None => continue,
                    },
                    _ => continue,
                };
                if let Some(n) = jsc::query_param(&url, "n") {
                    ns.push(n);
                }
            }
        }
        let (need_n, need_sig) = self.inner.solutions.split(player_id, &ns, &sigs);
        if !need_n.is_empty() || !need_sig.is_empty() {
            let started = Instant::now();
            let pre = self.inner.solutions.preprocessed(player_id);
            let sol = tokio::task::spawn_blocking(move || match pre {
                Some(p) => jsc::solve(jsc::PlayerInput::Preprocessed(&p), &need_n, &need_sig),
                None => jsc::solve(jsc::PlayerInput::Full(&code), &need_n, &need_sig),
            })
                .await
                .map_err(|e| Error::Parse(format!("js solver crashed: {e}")))??;
            log::info!("solved {} n / {} sig challenges in {:.2}s", sol.n.len(), sol.sig.len(), started.elapsed().as_secs_f64());
            self.inner.solutions.add(player_id, sol);
        }
        for path in lists {
            for f in v.pointer_mut(path).and_then(Value::as_array_mut).into_iter().flatten() {
                let Some(obj) = f.as_object_mut() else { continue };
                let mut url = match (obj.get("url").and_then(Value::as_str), obj.get("signatureCipher").and_then(Value::as_str)) {
                    (Some(u), _) => u.to_string(),
                    (None, Some(c)) => {
                        let Some((s, sp, u)) = jsc::parse_cipher(c) else { continue };
                        // An unsolved signature means a URL that would 403: drop the format.
                        let Some(sig) = self.inner.solutions.sig(player_id, &s) else { continue };
                        jsc::set_query_param(&u, &sp, &sig)
                    }
                    _ => continue,
                };
                if let Some(n) = jsc::query_param(&url, "n") {
                    match self.inner.solutions.n(player_id, &n) {
                        Some(solved) => url = jsc::set_query_param(&url, "n", &solved),
                        // Unsolved n is throttled to a crawl; better to skip the format.
                        None => continue,
                    }
                }
                obj.remove("signatureCipher");
                obj.insert("url".into(), Value::String(url));
            }
        }
        Ok(())
    }

    async fn player(&self, client: &ClientProfile, video_id: &str, visitor: Option<&str>) -> Result<Value> {
        let js = if client.requires_js { Some(self.player_js().await?) } else { None };
        let mut body = innertube::player_body(client, video_id, visitor, js.as_ref().map(|j| j.2));
        let mut client_version = client.version.clone();
        let mut visitor = visitor.map(str::to_string);
        if client.embed {
            let referer = body.pointer("/context/thirdParty/embedUrl").and_then(Value::as_str).unwrap_or("https://www.google.com/").to_string();
            // The host flags encode the embedding site, which the page reads from the Referer.
            let page = self
                .inner
                .api
                .get(format!("https://www.youtube.com/embed/{video_id}?html5=1"))
                .header("User-Agent", &client.user_agent)
                .header("Referer", referer)
                .header("Cookie", "PREF=hl=en&tz=UTC; SOCS=CAI")
                .send()
                .await?
                .text()
                .await?;
            let cfg = innertube::parse_embed_config(&page);
            innertube::apply_embed_config(&mut body, &cfg);
            if let Some(v) = cfg.version {
                client_version = v;
            }
            if cfg.visitor.is_some() {
                visitor = cfg.visitor;
            }
        }
        let mut req = self
            .inner
            .api
            .post(format!("https://{}/youtubei/v1/player?prettyPrint=false", client.host))
            .header("User-Agent", &client.user_agent)
            .header("X-Youtube-Client-Name", client.name_id.to_string())
            .header("X-Youtube-Client-Version", &client_version)
            .header("Origin", format!("https://{}", client.host))
            .json(&body);
        if let Some(v) = &visitor {
            req = req.header("X-Goog-Visitor-Id", v);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(Error::Http { status, what: "asking for the video's streams".into() });
        }
        let mut v = resp.json::<Value>().await.map_err(|e| Error::Parse(e.to_string()))?;
        if let Some((id, code, _)) = js {
            if innertube::playability_error(&v).is_none() {
                self.solve_challenges(&mut v, &id, code).await?;
            }
        }
        Ok(v)
    }

    /// TV clients leave out the title and channel; oEmbed has both.
    async fn fill_details(&self, info: &mut VideoInfo) {
        let url = format!("https://www.youtube.com/oembed?format=json&url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3D{}", info.id);
        let Ok(resp) = self.inner.api.get(url).header("User-Agent", innertube::WEB_UA).send().await else { return };
        let Ok(v) = resp.json::<Value>().await else { return };
        if let Some(t) = v["title"].as_str().filter(|t| !t.is_empty()) {
            info.title = t.to_string();
        }
        if let Some(a) = v["author_name"].as_str().filter(|a| !a.is_empty()) {
            info.author = a.to_string();
        }
    }

    /// Fresh extraction, trying each client in turn.
    pub async fn extract(&self, video_id: &str) -> Result<VideoInfo> {
        let clients = self.clients();
        let mut errors: Vec<Error> = vec![];
        let mut visitor = self.visitor(false).await;
        for client in &clients {
            for attempt in 0..3 {
                let r = match self.player(client, video_id, visitor.as_deref()).await {
                    Ok(v) => innertube::parse_player(&v, video_id, &client.key),
                    Err(e) => Err(e),
                };
                match r {
                    Ok(mut info) => {
                        if info.title == video_id || info.author.is_empty() {
                            self.fill_details(&mut info).await;
                        }
                        log::info!("{video_id}: {} formats from {}", info.formats.len(), client.key);
                        self.inner.cache.lock().unwrap().insert(video_id.to_string(), info.clone());
                        return Ok(info);
                    }
                    // A new visitor session usually clears a bot check.
                    Err(Error::BotCheck) if attempt == 0 => visitor = self.visitor(true).await,
                    Err(e) if e.is_transient() && attempt < 2 => tokio::time::sleep(Duration::from_millis(400 << attempt)).await,
                    Err(e) => {
                        log::warn!("{video_id}: {} failed: {e}", client.key);
                        errors.push(e);
                        break;
                    }
                }
            }
        }
        // Report the most telling failure: a definite answer beats a vague one.
        let rank = |e: &Error| match e {
            Error::AgeRestricted => 0,
            Error::Live => 1,
            Error::Unavailable(_) => 2,
            Error::BotCheck => 3,
            Error::NoFormat => 4,
            _ => 5,
        };
        Err(errors.into_iter().min_by_key(rank).unwrap_or(Error::NoFormat))
    }

    /// Video details and streams, cached until shortly before the URLs expire.
    pub async fn info(&self, input: &str) -> Result<VideoInfo> {
        let target = id::parse(input).ok_or(Error::InvalidUrl)?;
        if let Some(info) = self.inner.cache.lock().unwrap().get(&target.video_id) {
            if info.expires_at > now_secs() + 15 * 60 {
                return Ok(info.clone());
            }
        }
        self.extract(&target.video_id).await
    }

    async fn content_length(&self, url: &str) -> Result<u64> {
        let r = self.inner.media.head(url).header("User-Agent", &self.inner.config.fetch.user_agent).send().await?;
        let status = r.status().as_u16();
        if status >= 400 {
            return Err(Error::Http { status, what: "checking the stream size".into() });
        }
        r.content_length().filter(|&n| n > 0).ok_or_else(|| Error::Parse("stream has no length".into()))
    }

    async fn get_range(&self, url: &str, first: u64, last: u64) -> Result<Vec<u8>> {
        let mut err = Error::Network("unreachable".into());
        for attempt in 0..4 {
            let r = self
                .inner
                .media
                .get(fetch::range_url(url, first, last))
                .header("User-Agent", &self.inner.config.fetch.user_agent)
                .timeout(Duration::from_secs(8))
                .send()
                .await;
            match r {
                Ok(r) if r.status().is_success() => match r.bytes().await {
                    Ok(b) => return Ok(b.to_vec()),
                    Err(e) => err = e.into(),
                },
                Ok(r) => err = Error::Http { status: r.status().as_u16(), what: "reading the stream index".into() },
                Err(e) => err = e.into(),
            }
            if !err.is_transient() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300 << attempt)).await;
        }
        Err(err)
    }

    /// Segments of a stream plus its init bytes (WebM ones already patched for splicing).
    async fn segments(&self, f: &Format, duration: f64, total: u64) -> Result<(Vec<u8>, Vec<Segment>)> {
        let (Some((_, init_end)), Some((idx_start, idx_end))) = (f.init_range, f.index_range) else {
            return Err(Error::Parse("stream has no index".into()));
        };
        let head = self.get_range(&f.url, 0, idx_end.max(init_end)).await?;
        if head.len() as u64 <= idx_end {
            return Err(Error::Parse("short index read".into()));
        }
        let mut init = head[..=init_end as usize].to_vec();
        let idx = &head[idx_start as usize..=idx_end as usize];
        let segs = match f.container {
            Container::Mp4 => index::parse_sidx(idx, idx_start, total)?,
            Container::Webm => {
                let header = index::parse_webm_header(&init)?;
                let segs = index::parse_cues(idx, &header, total, duration)?;
                index::patch_webm_init(&mut init)?;
                segs
            }
            Container::Other => return Err(Error::Parse("unknown container".into())),
        };
        Ok((init, segs))
    }

    /// Download per the request. `on_progress` is called a few times a second.
    pub async fn download(&self, req: DownloadRequest, on_progress: impl Fn(Progress) + Send + Sync + 'static, cancel: CancellationToken) -> Result<DownloadResult> {
        let started = Instant::now();
        let on_progress = Arc::new(on_progress);
        on_progress(Progress { stage: Stage::Preparing, done: 0, total: 0, speed: 0.0, eta: None, fraction: 0.0 });
        let target = id::parse(&req.url).ok_or(Error::InvalidUrl)?;
        let info = self.info(&req.url).await?;
        let video = req.video.map(|t| info.format(t).cloned().ok_or(Error::NoFormat)).transpose()?;
        let audio = req.audio.map(|t| info.format(t).cloned().ok_or(Error::NoFormat)).transpose()?;
        let streams: Vec<(&Format, char)> = video.iter().map(|f| (f, 'v')).chain(audio.iter().map(|f| (f, 'a'))).collect();
        if streams.is_empty() {
            return Err(Error::NoFormat);
        }
        let clip = match req.clip {
            Some((s, e)) => {
                let end = if info.duration > 0.0 { e.min(info.duration) } else { e };
                let start = s.max(0.0);
                if end - start < 0.05 {
                    return Err(Error::Parse("the clip is empty".into()));
                }
                Some((start, end))
            }
            None => None,
        };
        let ffmpeg = self.ffmpeg();
        let needs_ffmpeg = clip.is_some() || (!req.raw && streams.len() > 1);
        if needs_ffmpeg && ffmpeg.is_none() {
            return Err(Error::FfmpegMissing);
        }
        std::fs::create_dir_all(&req.dir)?;
        let stem = names::sanitize(req.file_name.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(&info.title));
        let work = TempDir::new(&req.dir, &target.video_id)?;

        // Work out the byte ranges for every stream.
        let mut targets: Vec<Target> = vec![];
        let mut prefixes: Vec<(PathBuf, Vec<u8>)> = vec![];
        for (n, (f, kind)) in streams.iter().enumerate() {
            let total = match f.content_length {
                Some(n) => n,
                None => self.content_length(&f.url).await?,
            };
            let path = work.path.join(format!("{n}-{kind}.{}", f.raw_ext()));
            let mut pieces = vec![Piece { first: 0, last: total - 1, dest_offset: 0 }];
            if let Some((s, e)) = clip {
                match self.segments(f, info.duration, total).await {
                    Ok((init, segs)) => {
                        let cover = index::covering(&segs, s, e);
                        let mut off = init.len() as u64;
                        pieces = cover
                            .iter()
                            .map(|seg| {
                                let p = Piece { first: seg.first_byte, last: seg.last_byte, dest_offset: off };
                                off += seg.len();
                                p
                            })
                            .collect();
                        prefixes.push((path.clone(), init));
                    }
                    // No usable index: fetch the whole stream and cut it locally.
                    Err(e) => log::warn!("itag {}: {e}; downloading the whole stream for the clip", f.itag),
                }
            }
            targets.push(Target { path, url: f.url.clone(), itag: f.itag, pieces });
        }

        // Download.
        let total: u64 = targets.iter().map(Target::bytes).sum();
        let done = Arc::new(AtomicU64::new(0));
        let merge_share = if needs_ffmpeg || !req.raw { 0.03 } else { 0.0 };
        let reporter = {
            let (done, on_progress, cancel) = (done.clone(), on_progress.clone(), cancel.clone());
            tokio::spawn(async move {
                let mut last = (Instant::now(), 0u64);
                let mut speed = 0.0f64;
                let mut tick = tokio::time::interval(Duration::from_millis(250));
                loop {
                    tokio::select! { _ = tick.tick() => {}, _ = cancel.cancelled() => break }
                    let d = done.load(Ordering::Relaxed).min(total);
                    let dt = last.0.elapsed().as_secs_f64();
                    if dt > 0.0 {
                        let inst = (d - last.1.min(d)) as f64 / dt;
                        speed = if speed == 0.0 { inst } else { speed * 0.75 + inst * 0.25 };
                    }
                    last = (Instant::now(), d);
                    let eta = (speed > 1.0).then(|| (total - d) as f64 / speed);
                    let fraction = if total > 0 { d as f64 / total as f64 * (1.0 - merge_share) } else { 0.0 };
                    on_progress(Progress { stage: Stage::Downloading, done: d, total, speed, eta, fraction });
                }
            })
        };
        let refresh: Refresh = {
            let engine = self.clone();
            let id = target.video_id.clone();
            Arc::new(move || {
                let (engine, id) = (engine.clone(), id.clone());
                Box::pin(async move {
                    log::info!("{id}: stream URLs expired, refreshing");
                    let info = engine.extract(&id).await?;
                    Ok(info.formats.iter().map(|f| (f.itag, f.url.clone())).collect())
                })
            })
        };
        for (path, init) in &prefixes {
            std::fs::write(path, init)?;
        }
        let fetched = fetch::fetch_all(&self.inner.media, &targets, &self.inner.config.fetch, Some(refresh), done.clone(), cancel.child_token()).await;
        reporter.abort();
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        fetched?;
        let downloaded = done.load(Ordering::Relaxed).min(total);
        let report_merge = |frac: f64| {
            on_progress(Progress { stage: Stage::Merging, done: total, total, speed: 0.0, eta: None, fraction: (1.0 - merge_share) + merge_share * frac.clamp(0.0, 1.0) })
        };

        // Assemble the output.
        let meta_comment = info.watch_url();
        let meta = Metadata { title: &info.title, artist: &info.author, comment: &meta_comment };
        let cut = clip.map(|(start, end)| Cut { start, end });
        let duration = clip.map(|(s, e)| e - s).unwrap_or(info.duration).max(0.001);
        let mut files = vec![];
        if req.raw {
            for (t, (f, kind)) in targets.iter().zip(&streams) {
                let suffix = if streams.len() > 1 { if *kind == 'v' { " (video)" } else { " (audio)" } } else { "" };
                let s = format!("{stem}{suffix}");
                if let (Some(cut), Some(ff)) = (&cut, &ffmpeg) {
                    let ext = if *kind == 'v' { select::output_ext(Some(f), None, true) } else { select::output_ext(None, Some(f), true) };
                    let tmp = work.path.join(format!("out-{kind}.{ext}"));
                    let args = ffmpeg::args(&[Input { path: &t.path, stream: *kind }], Some(cut), Some(&meta), &tmp);
                    ffmpeg::run(ff, &args, &cancel, |t| report_merge(t / duration)).await?;
                    files.push(place(&tmp, &req.dir, &s, ext)?);
                } else {
                    files.push(place(&t.path, &req.dir, &s, f.raw_ext())?);
                }
            }
        } else if let Some(ff) = &ffmpeg {
            let ext = req.ext.clone().unwrap_or_else(|| select::output_ext(video.as_ref(), audio.as_ref(), clip.is_some()).to_string());
            let tmp = work.path.join(format!("out.{ext}"));
            let inputs: Vec<Input> = targets.iter().zip(&streams).map(|(t, (_, k))| Input { path: &t.path, stream: *k }).collect();
            let args = ffmpeg::args(&inputs, cut.as_ref(), Some(&meta), &tmp);
            ffmpeg::run(ff, &args, &cancel, |t| report_merge(t / duration)).await?;
            files.push(place(&tmp, &req.dir, &stem, &ext)?);
        } else {
            // A single stream and no ffmpeg: the downloaded file is already playable.
            let (t, (f, _)) = (&targets[0], &streams[0]);
            files.push(place(&t.path, &req.dir, &stem, f.raw_ext())?);
        }
        drop(work);
        let seconds = started.elapsed().as_secs_f64();
        on_progress(Progress { stage: Stage::Done, done: total, total, speed: downloaded as f64 / seconds.max(0.001), eta: Some(0.0), fraction: 1.0 });
        Ok(DownloadResult { files, bytes: downloaded, seconds })
    }
}

/// Remove work directories (`.dyt-<id>-<pid>-<n>`) that a process which is no longer running
/// left behind in `dir` (a crash, a forced quit, a power cut).
pub fn clean_stale_work(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let me = std::process::id();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(".dyt-") else { continue };
        // <11-char id>-<pid>-<hex>; ids may contain '-', so read from the right.
        let mut parts = rest.rsplitn(3, '-');
        let (Some(_), Some(pid)) = (parts.next(), parts.next()) else { continue };
        let Ok(pid) = pid.parse::<u32>() else { continue };
        let alive = pid == me || (cfg!(unix) && Path::new(&format!("/proc/{pid}")).exists());
        if !alive && e.path().is_dir() {
            std::fs::remove_dir_all(e.path()).ok();
        }
    }
}

/// Move a finished file into place under a free name.
fn place(from: &Path, dir: &Path, stem: &str, ext: &str) -> Result<PathBuf> {
    let to = names::unique(dir, stem, ext);
    if std::fs::rename(from, &to).is_err() {
        std::fs::copy(from, &to)?;
        std::fs::remove_file(from).ok();
    }
    Ok(to)
}

/// A hidden working directory beside the output (same filesystem, so the final move is a
/// rename), removed however the job ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(dir: &Path, id: &str) -> Result<TempDir> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).subsec_nanos();
        let path = dir.join(format!(".dyt-{id}-{}-{nanos:x}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(TempDir { path })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_work_dirs_are_removed_live_ones_kept() {
        let dir = std::env::temp_dir().join(format!("dyt-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dead = dir.join(".dyt-ab-cdefghij-999999999-1f");
        let mine = dir.join(format!(".dyt-abcdefghijk-{}-2e", std::process::id()));
        let other = dir.join("keep-me");
        for d in [&dead, &mine, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        clean_stale_work(&dir);
        assert!(!dead.exists() && mine.exists() && other.exists());
        std::fs::remove_dir_all(dir).ok();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).ok();
    }
}

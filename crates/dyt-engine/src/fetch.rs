//! The parallel range downloader.
//!
//! Every byte range a job needs (whole streams, or a clip's init + segments, for one or two
//! streams) becomes a queue of chunks served by one pool of connections. Each chunk is a
//! `&range=a-b` request written straight to its offset in a preallocated file. When the queue
//! runs dry, idle workers steal the back half of the slowest chunk still in flight, so the
//! last few megabytes never wait on one slow connection. Failed chunks resume from the last
//! byte written; expired URLs (403/410) are refreshed once for all workers and the job carries on.

use std::collections::VecDeque;
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};

/// A byte range of one source stream and where it lands in that stream's file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub first: u64,
    pub last: u64,
    pub dest_offset: u64,
}

pub struct Target {
    pub path: PathBuf,
    pub url: String,
    /// The source stream's itag, used to look up a fresh URL.
    pub itag: u32,
    pub pieces: Vec<Piece>,
}

impl Target {
    pub fn bytes(&self) -> u64 {
        self.pieces.iter().map(|p| p.last - p.first + 1).sum()
    }
}

/// Re-extracts the video and returns fresh URLs, `(itag, url)`.
pub type Refresh = Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<(u32, String)>>> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct FetchOptions {
    pub connections: usize,
    pub min_chunk: u64,
    pub max_chunk: u64,
    pub max_attempts: u32,
    /// A read that stalls this long is abandoned and retried.
    pub stall: Duration,
    pub user_agent: String,
}

impl Default for FetchOptions {
    fn default() -> Self {
        FetchOptions {
            connections: 8,
            min_chunk: 512 * 1024,
            max_chunk: 8 * 1024 * 1024,
            // With the 8 s backoff cap this rides out ~2.5 minutes of a dead connection
            // (a Wi-Fi switch, a tunnel) before giving up on the job.
            max_attempts: 22,
            stall: Duration::from_secs(15),
            user_agent: crate::innertube::WEB_UA.into(),
        }
    }
}

struct Chunk {
    target: usize,
    /// Next source byte to fetch; advanced as data is written.
    pos: Arc<AtomicU64>,
    /// Last source byte this chunk owns; lowered when another worker steals the tail.
    end: Arc<AtomicU64>,
    /// `dest = src - src_base + dest_base` within the piece.
    src_base: u64,
    dest_base: u64,
    attempts: u32,
}

#[derive(Clone)]
struct InFlight {
    pos: Arc<AtomicU64>,
    end: Arc<AtomicU64>,
    target: usize,
    src_base: u64,
    dest_base: u64,
    /// Milliseconds (since the job started) when this chunk last wrote anything.
    touched: Arc<AtomicU64>,
}

struct Shared {
    queue: Mutex<VecDeque<Chunk>>,
    in_flight: Mutex<Vec<InFlight>>,
    urls: Mutex<Vec<String>>,
    url_gen: AtomicU64,
    refresh_lock: AsyncMutex<()>,
    files: Vec<Arc<File>>,
    itags: Vec<u32>,
    done: Arc<AtomicU64>,
    failed: AtomicBool,
    started: std::time::Instant,
}

impl Shared {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

/// A chunk that has written nothing for this long is taken over by an idle worker.
const TAKEOVER_MS: u64 = 4000;

/// Download every target's pieces. `done` is bumped as bytes land, for progress reporting.
pub async fn fetch_all(
    client: &reqwest::Client,
    targets: &[Target],
    opts: &FetchOptions,
    refresh: Option<Refresh>,
    done: Arc<AtomicU64>,
    cancel: CancellationToken,
) -> Result<()> {
    let total: u64 = targets.iter().map(Target::bytes).sum();
    if total == 0 {
        return Ok(());
    }
    let mut files = vec![];
    for t in targets {
        let f = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&t.path)?;
        let size = t.pieces.iter().map(|p| p.dest_offset + p.last - p.first + 1).max().unwrap_or(0);
        f.set_len(size)?;
        files.push(Arc::new(f));
    }
    let connections = opts.connections.clamp(1, 32);
    let chunk_size = (total / (connections as u64 * 3)).clamp(opts.min_chunk, opts.max_chunk);

    // Interleave the targets' chunks so both streams progress together.
    let mut per_target: Vec<VecDeque<Chunk>> = targets
        .iter()
        .enumerate()
        .map(|(ti, t)| {
            let mut q = VecDeque::new();
            for p in &t.pieces {
                let mut a = p.first;
                while a <= p.last {
                    let b = (a + chunk_size - 1).min(p.last);
                    q.push_back(Chunk {
                        target: ti,
                        pos: Arc::new(AtomicU64::new(a)),
                        end: Arc::new(AtomicU64::new(b)),
                        src_base: p.first,
                        dest_base: p.dest_offset,
                        attempts: 0,
                    });
                    a = b + 1;
                }
            }
            q
        })
        .collect();
    let mut queue = VecDeque::new();
    while per_target.iter().any(|q| !q.is_empty()) {
        for q in per_target.iter_mut() {
            if let Some(c) = q.pop_front() {
                queue.push_back(c);
            }
        }
    }

    let shared = Arc::new(Shared {
        queue: Mutex::new(queue),
        in_flight: Mutex::new(vec![]),
        urls: Mutex::new(targets.iter().map(|t| t.url.clone()).collect()),
        url_gen: AtomicU64::new(0),
        refresh_lock: AsyncMutex::new(()),
        files,
        itags: targets.iter().map(|t| t.itag).collect(),
        done,
        failed: AtomicBool::new(false),
        started: std::time::Instant::now(),
    });

    let worker_cancel = cancel.child_token();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..connections {
        let (client, shared, opts, refresh, cancel) = (client.clone(), shared.clone(), opts.clone(), refresh.clone(), worker_cancel.clone());
        set.spawn(async move { worker(client, shared, opts, refresh, cancel).await });
    }
    let mut first_err = None;
    while let Some(r) = set.join_next().await {
        let r = r.unwrap_or_else(|e| Err(Error::Network(format!("worker crashed: {e}"))));
        if let Err(e) = r {
            if first_err.is_none() {
                first_err = Some(e);
            }
            worker_cancel.cancel();
        }
    }
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    for f in &shared.files {
        f.sync_data().ok();
    }
    Ok(())
}

/// Take the next chunk; when none are queued, steal the unstarted back half of the largest
/// chunk in flight, or take over one that has stopped making progress.
fn next_chunk(shared: &Shared, min_steal: u64) -> Option<Chunk> {
    if let Some(c) = shared.queue.lock().unwrap().pop_front() {
        return Some(c);
    }
    let now = shared.now_ms();
    let flight = shared.in_flight.lock().unwrap();
    let remaining = |f: &InFlight| (f.end.load(Ordering::Acquire) + 1).saturating_sub(f.pos.load(Ordering::Acquire));
    if let Some(f) = flight.iter().filter(|f| remaining(f) >= 2 * min_steal).max_by_key(|f| remaining(f)) {
        let p = f.pos.load(Ordering::Acquire);
        let e = f.end.load(Ordering::Acquire);
        let mid = p + (e + 1 - p) / 2;
        // The owner stops at `end`; any overlap from the race is the same bytes at the same offset.
        f.end.store(mid - 1, Ordering::Release);
        return Some(Chunk { target: f.target, pos: Arc::new(AtomicU64::new(mid)), end: Arc::new(AtomicU64::new(e)), src_base: f.src_base, dest_base: f.dest_base, attempts: 0 });
    }
    let stuck = flight.iter().filter(|f| remaining(f) > 0 && now.saturating_sub(f.touched.load(Ordering::Acquire)) > TAKEOVER_MS).max_by_key(|f| remaining(f))?;
    let p = stuck.pos.load(Ordering::Acquire);
    let e = stuck.end.load(Ordering::Acquire);
    // Hand the whole rest to a fresh connection; the stuck one finds itself done when it wakes.
    stuck.end.store(p.saturating_sub(1), Ordering::Release);
    stuck.touched.store(now, Ordering::Release);
    log::debug!("taking over a stalled chunk at {p}..={e}");
    Some(Chunk { target: stuck.target, pos: Arc::new(AtomicU64::new(p)), end: Arc::new(AtomicU64::new(e)), src_base: stuck.src_base, dest_base: stuck.dest_base, attempts: 0 })
}

async fn worker(client: reqwest::Client, shared: Arc<Shared>, opts: FetchOptions, refresh: Option<Refresh>, cancel: CancellationToken) -> Result<()> {
    while let Some(mut chunk) = next_chunk(&shared, opts.min_chunk) {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let entry = InFlight {
            pos: chunk.pos.clone(),
            end: chunk.end.clone(),
            target: chunk.target,
            src_base: chunk.src_base,
            dest_base: chunk.dest_base,
            touched: Arc::new(AtomicU64::new(shared.now_ms())),
        };
        shared.in_flight.lock().unwrap().push(entry.clone());
        let result = loop {
            let generation = shared.url_gen.load(Ordering::Acquire);
            let url = shared.urls.lock().unwrap()[chunk.target].clone();
            match fetch_chunk(&client, &url, &chunk, &entry.touched, &shared, &opts, &cancel).await {
                Ok(()) => break Ok(()),
                Err(Error::Cancelled) => break Err(Error::Cancelled),
                Err(e) => {
                    chunk.attempts += 1;
                    let expired = matches!(e, Error::Http { status: 403 | 404 | 410, .. });
                    log::debug!("chunk of target {} failed (attempt {}): {e}", chunk.target, chunk.attempts);
                    if chunk.attempts >= opts.max_attempts {
                        break Err(e);
                    }
                    if expired {
                        let Some(refresh) = &refresh else { break Err(e) };
                        if let Err(re) = refresh_urls(&shared, refresh, generation).await {
                            break Err(re);
                        }
                        continue;
                    }
                    if !e.is_transient() {
                        break Err(e);
                    }
                    let backoff = Duration::from_millis((200u64 << chunk.attempts.min(6)).min(8000));
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = cancel.cancelled() => break Err(Error::Cancelled),
                    }
                }
            }
        };
        shared.in_flight.lock().unwrap().retain(|f| !Arc::ptr_eq(&f.pos, &entry.pos));
        if let Err(e) = result {
            shared.failed.store(true, Ordering::Release);
            return Err(e);
        }
    }
    Ok(())
}

/// One refresh for everyone: workers that hit a stale URL after another worker already
/// refreshed it (`generation` moved on) just retry with the new one.
async fn refresh_urls(shared: &Shared, refresh: &Refresh, generation: u64) -> Result<()> {
    let _guard = shared.refresh_lock.lock().await;
    if shared.url_gen.load(Ordering::Acquire) != generation {
        return Ok(());
    }
    let fresh = refresh().await?;
    {
        let mut urls = shared.urls.lock().unwrap();
        for (i, itag) in shared.itags.iter().enumerate() {
            if let Some((_, u)) = fresh.iter().find(|(t, _)| t == itag) {
                urls[i] = u.clone();
            }
        }
    }
    shared.url_gen.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

pub(crate) fn range_url(url: &str, first: u64, last: u64) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}range={first}-{last}")
}

async fn fetch_chunk(
    client: &reqwest::Client,
    url: &str,
    chunk: &Chunk,
    touched: &AtomicU64,
    shared: &Shared,
    opts: &FetchOptions,
    cancel: &CancellationToken,
) -> Result<()> {
    let start = chunk.pos.load(Ordering::Acquire);
    let end = chunk.end.load(Ordering::Acquire);
    if start > end {
        return Ok(());
    }
    let req = client.get(range_url(url, start, end)).header("User-Agent", &opts.user_agent).header("Accept", "*/*").send();
    let mut resp = tokio::select! {
        r = tokio::time::timeout(opts.stall, req) => r.map_err(|_| Error::Network("connection timed out".into()))??,
        _ = cancel.cancelled() => return Err(Error::Cancelled),
    };
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Error::Http { status, what: "downloading".into() });
    }
    if resp.content_length() == Some(0) {
        return Err(Error::Network("empty response".into()));
    }
    let file = shared.files[chunk.target].clone();
    let mut buf: Vec<u8> = Vec::with_capacity(1 << 20);
    let mut pos = start;
    loop {
        let next = tokio::select! {
            r = tokio::time::timeout(opts.stall, resp.chunk()) => r.map_err(|_| Error::Network("download stalled".into()))?,
            _ = cancel.cancelled() => return Err(Error::Cancelled),
        };
        let data = match next {
            Ok(Some(d)) => Some(d),
            Ok(None) => None,
            Err(e) => {
                // Keep what we have; the retry resumes after it.
                flush(&file, chunk, &mut buf, &mut pos, &shared.done).await?;
                return Err(e.into());
            }
        };
        let finished = data.is_none();
        if let Some(d) = data {
            buf.extend_from_slice(&d);
            touched.store(shared.now_ms(), Ordering::Release);
        }
        if chunk.end.load(Ordering::Acquire) < pos {
            // Taken over or stolen while we waited: whatever we hold is someone else's now.
            return Ok(());
        }
        if buf.len() >= (1 << 20) || finished {
            let reached_end = flush(&file, chunk, &mut buf, &mut pos, &shared.done).await?;
            if reached_end {
                return Ok(());
            }
        }
        if finished {
            // The server closed early; the caller retries from `pos`.
            return Err(Error::Network(format!("connection closed {} bytes early", chunk.end.load(Ordering::Acquire) + 1 - pos)));
        }
    }
}

/// Write `buf` at `pos` (clipped to the chunk's current end). Returns true once the chunk is done.
async fn flush(file: &Arc<File>, chunk: &Chunk, buf: &mut Vec<u8>, pos: &mut u64, done: &AtomicU64) -> Result<bool> {
    let end = chunk.end.load(Ordering::Acquire);
    if *pos > end {
        buf.clear();
        return Ok(true);
    }
    let allowed = ((end + 1 - *pos) as usize).min(buf.len());
    if allowed > 0 {
        let data: Vec<u8> = buf.drain(..allowed).collect();
        let dest = *pos - chunk.src_base + chunk.dest_base;
        let f = file.clone();
        tokio::task::spawn_blocking(move || write_at(&f, &data, dest)).await.map_err(|e| Error::Io(std::io::Error::other(e)))??;
        *pos += allowed as u64;
        chunk.pos.store(*pos, Ordering::Release);
        done.fetch_add(allowed as u64, Ordering::Relaxed);
    }
    buf.clear();
    Ok(*pos > chunk.end.load(Ordering::Acquire))
}

#[cfg(unix)]
fn write_at(f: &File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.write_all_at(data, offset)
}

#[cfg(windows)]
fn write_at(f: &File, mut data: &[u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !data.is_empty() {
        let n = f.seek_write(data, offset)?;
        data = &data[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_urls() {
        assert_eq!(range_url("https://x/videoplayback?a=1", 0, 99), "https://x/videoplayback?a=1&range=0-99");
        assert_eq!(range_url("https://x/v", 5, 9), "https://x/v?range=5-9");
    }

    fn shared() -> Shared {
        Shared {
            queue: Mutex::new(VecDeque::new()),
            in_flight: Mutex::new(vec![]),
            urls: Mutex::new(vec![]),
            url_gen: AtomicU64::new(0),
            refresh_lock: AsyncMutex::new(()),
            files: vec![],
            itags: vec![],
            done: Arc::new(AtomicU64::new(0)),
            failed: AtomicBool::new(false),
            started: std::time::Instant::now(),
        }
    }

    fn flight(pos: u64, end: u64, touched: u64) -> InFlight {
        InFlight {
            pos: Arc::new(AtomicU64::new(pos)),
            end: Arc::new(AtomicU64::new(end)),
            target: 0,
            src_base: 0,
            dest_base: 0,
            touched: Arc::new(AtomicU64::new(touched)),
        }
    }

    #[test]
    fn stealing_splits_the_tail() {
        let s = shared();
        let f = flight(1000, 10_999, 0);
        s.in_flight.lock().unwrap().push(f.clone());
        let stolen = next_chunk(&s, 1000).unwrap();
        assert_eq!(f.end.load(Ordering::Acquire), 5999);
        assert_eq!(stolen.pos.load(Ordering::Acquire), 6000);
        assert_eq!(stolen.end.load(Ordering::Acquire), 10_999);
        // Too small to split, and still making progress: nothing to do.
        f.pos.store(5500, Ordering::Release);
        f.touched.store(s.now_ms(), Ordering::Release);
        assert!(next_chunk(&s, 1000).is_none());
    }

    #[test]
    fn stalled_chunks_are_taken_over() {
        let s = shared();
        let f = flight(5500, 5999, 0);
        s.in_flight.lock().unwrap().push(f.clone());
        std::thread::sleep(Duration::from_millis(TAKEOVER_MS + 50));
        let took = next_chunk(&s, 1000).unwrap();
        assert_eq!((took.pos.load(Ordering::Acquire), took.end.load(Ordering::Acquire)), (5500, 5999));
        assert_eq!(f.end.load(Ordering::Acquire), 5499, "the stuck owner is finished");
        assert!(next_chunk(&s, 1000).is_none(), "nothing left to take");
    }
}

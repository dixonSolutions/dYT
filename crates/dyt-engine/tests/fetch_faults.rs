//! The downloader against a hostile local server: 503s, bodies cut off half way, stalled
//! connections, and URLs that expire mid-job. The file must still come out byte-identical.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dyt_engine::fetch::{fetch_all, FetchOptions, Piece, Refresh, Target};
use dyt_engine::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn blob(len: usize) -> Arc<Vec<u8>> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    Arc::new(
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect(),
    )
}

#[derive(Clone, Copy)]
struct Faults {
    /// Every nth request answers 503.
    unavailable: usize,
    /// Every nth request sends half its body and hangs up.
    truncate: usize,
    /// Every nth request stalls before answering.
    stall: usize,
    /// URLs with `tok=old` get 403 once this many requests have been served.
    expire_after: usize,
}

async fn serve(data: Arc<Vec<u8>>, faults: Faults) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let (data, count) = (data.clone(), c.clone());
            tokio::spawn(async move {
                let mut req = vec![0u8; 4096];
                let mut n = 0;
                while !req[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut req[n..]).await {
                        Ok(0) | Err(_) => return,
                        Ok(k) => n += k,
                    }
                }
                let head = String::from_utf8_lossy(&req[..n]).to_string();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let i = count.fetch_add(1, Ordering::SeqCst) + 1;
                let q = path.split_once('?').map(|x| x.1).unwrap_or("");
                let param = |k: &str| q.split('&').find_map(|p| p.strip_prefix(&format!("{k}="))).map(str::to_string);
                if param("tok").as_deref() == Some("old") && i > faults.expire_after {
                    sock.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.ok();
                    return;
                }
                if faults.unavailable > 0 && i % faults.unavailable == 0 {
                    sock.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.ok();
                    return;
                }
                if faults.stall > 0 && i % faults.stall == 0 {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return;
                }
                let (a, b) = param("range")
                    .and_then(|r| r.split_once('-').map(|(a, b)| (a.parse::<usize>().ok(), b.parse::<usize>().ok())))
                    .and_then(|(a, b)| Some((a?, b?)))
                    .unwrap_or((0, data.len() - 1));
                let body = &data[a..=b.min(data.len() - 1)];
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                sock.write_all(header.as_bytes()).await.ok();
                if faults.truncate > 0 && i % faults.truncate == 0 {
                    sock.write_all(&body[..body.len() / 2]).await.ok();
                    return;
                }
                for part in body.chunks(64 * 1024) {
                    if sock.write_all(part).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (format!("http://{addr}/videoplayback"), count)
}

fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder().http1_only().build().unwrap()
}

fn opts() -> FetchOptions {
    FetchOptions { connections: 6, min_chunk: 64 * 1024, max_chunk: 512 * 1024, stall: Duration::from_secs(2), ..FetchOptions::default() }
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fault-tests").join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn survives_errors_truncation_and_stalls() {
    let data = blob(6 * 1024 * 1024 + 123);
    let (url, served) = serve(data.clone(), Faults { unavailable: 5, truncate: 7, stall: 13, expire_after: usize::MAX }).await;
    let path = dir("faults").join("out.bin");
    let target = Target { path: path.clone(), url: format!("{url}?tok=x"), itag: 1, pieces: vec![Piece { first: 0, last: data.len() as u64 - 1, dest_offset: 0 }] };
    let done = Arc::new(AtomicU64::new(0));
    let started = std::time::Instant::now();
    fetch_all(&client(), &[target], &opts(), None, done.clone(), CancellationToken::new()).await.unwrap();
    let got = std::fs::read(&path).unwrap();
    assert_eq!(got.len(), data.len());
    assert!(got == *data, "downloaded bytes differ from the source");
    assert!(done.load(Ordering::Relaxed) >= data.len() as u64);
    assert!(served.load(Ordering::SeqCst) > 20, "the job really was split into many requests");
    assert!(started.elapsed() < Duration::from_secs(25), "stalls were worked around, took {:?}", started.elapsed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refreshes_expired_urls_once_and_carries_on() {
    let data = blob(3 * 1024 * 1024);
    let (url, _) = serve(data.clone(), Faults { unavailable: 0, truncate: 0, stall: 0, expire_after: 8 }).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let r = refreshes.clone();
    let fresh = format!("{url}?tok=new");
    let refresh: Refresh = Arc::new(move || {
        r.fetch_add(1, Ordering::SeqCst);
        let fresh = fresh.clone();
        Box::pin(async move { Ok(vec![(7, fresh)]) })
    });
    let path = dir("refresh").join("out.bin");
    let target = Target { path: path.clone(), url: format!("{url}?tok=old"), itag: 7, pieces: vec![Piece { first: 0, last: data.len() as u64 - 1, dest_offset: 0 }] };
    fetch_all(&client(), &[target], &opts(), Some(refresh), Arc::new(AtomicU64::new(0)), CancellationToken::new()).await.unwrap();
    assert!(std::fs::read(&path).unwrap() == *data);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1, "concurrent 403s share one refresh");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scattered_pieces_land_at_their_offsets() {
    // A clip: init bytes plus two non-adjacent segments, packed one after another.
    let data = blob(2 * 1024 * 1024);
    let (url, _) = serve(data.clone(), Faults { unavailable: 4, truncate: 0, stall: 0, expire_after: usize::MAX }).await;
    let pieces = vec![
        Piece { first: 0, last: 999, dest_offset: 0 },
        Piece { first: 500_000, last: 799_999, dest_offset: 1000 },
        Piece { first: 1_500_000, last: 1_600_000, dest_offset: 301_000 },
    ];
    let path = dir("pieces").join("out.bin");
    let target = Target { path: path.clone(), url: format!("{url}?tok=x"), itag: 1, pieces: pieces.clone() };
    fetch_all(&client(), &[target], &opts(), None, Arc::new(AtomicU64::new(0)), CancellationToken::new()).await.unwrap();
    let got = std::fs::read(&path).unwrap();
    let mut want = vec![];
    for p in &pieces {
        want.extend_from_slice(&data[p.first as usize..=p.last as usize]);
    }
    assert!(got == want);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancels_promptly() {
    let data = blob(8 * 1024 * 1024);
    let (url, _) = serve(data.clone(), Faults { unavailable: 0, truncate: 0, stall: 2, expire_after: usize::MAX }).await;
    let path = dir("cancel").join("out.bin");
    let target = Target { path, url: format!("{url}?tok=x"), itag: 1, pieces: vec![Piece { first: 0, last: data.len() as u64 - 1, dest_offset: 0 }] };
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        c.cancel();
    });
    let started = std::time::Instant::now();
    let r = fetch_all(&client(), &[target], &opts(), None, Arc::new(AtomicU64::new(0)), cancel).await;
    assert!(matches!(r, Err(dyt_engine::Error::Cancelled)), "{r:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

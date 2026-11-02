//! dYT's app: the engine behind Tauri commands, download jobs streamed to the UI over
//! channels, history and settings, and the system theme.

mod store;
mod theme;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use dyt_engine::{CancellationToken, DownloadRequest, Engine, EngineConfig, Progress, VideoInfo};
use serde::Serialize;
use store::{Download, HistoryEntry, Settings, Store};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    engine: Mutex<Engine>,
    jobs: Mutex<HashMap<u64, CancellationToken>>,
    next_job: AtomicU64,
    store: Store,
    /// A link passed on the command line (or shared to the app) before the UI was ready.
    pending_link: Mutex<Option<String>>,
    /// Set once the user has agreed to quit with downloads still running.
    quitting: std::sync::atomic::AtomicBool,
}

type CmdResult<T> = Result<T, dyt_engine::Error>;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Directory of our own shared library: on Android that's the native-library dir, where the
/// bundled ffmpeg (`libffmpeg.so`) is extracted.
fn own_library_dir() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .find(|p| p.ends_with("/libdyt_lib.so"))
        .and_then(|p| PathBuf::from(p).parent().map(PathBuf::from))
}

fn engine_config(s: &Settings) -> EngineConfig {
    let mut c = EngineConfig::default();
    c.fetch.connections = s.connections.clamp(1, 32) as usize;
    c.ffmpeg_dirs = own_library_dir().into_iter().collect();
    c
}

fn engine(state: &AppState) -> Engine {
    state.engine.lock().unwrap().clone()
}

#[tauri::command]
async fn resolve(input: String, state: State<'_, AppState>) -> CmdResult<VideoInfo> {
    let info = engine(&state).info(&input).await?;
    state.store.touch(HistoryEntry {
        video_id: info.id.clone(),
        url: info.watch_url(),
        title: info.title.clone(),
        author: info.author.clone(),
        thumbnail: info.thumbnail.clone(),
        duration: info.duration,
        at: now_ms(),
        downloads: vec![],
    });
    Ok(info)
}

#[tauri::command]
fn parse_link(input: String) -> Option<dyt_engine::id::Target> {
    dyt_engine::id::parse(&input)
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum JobEvent {
    Progress(Progress),
    Done { files: Vec<String>, bytes: u64, seconds: f64 },
    Failed { error: dyt_engine::Error },
    Cancelled,
}

#[tauri::command]
async fn download(request: DownloadRequest, on_event: Channel<JobEvent>, app: AppHandle, state: State<'_, AppState>) -> CmdResult<u64> {
    let id = state.next_job.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = CancellationToken::new();
    state.jobs.lock().unwrap().insert(id, cancel.clone());
    let engine = engine(&state);
    let video_id = dyt_engine::id::parse(&request.url).map(|t| t.video_id).unwrap_or_default();
    let clip = request.clip;
    tauri::async_runtime::spawn(async move {
        let events = on_event.clone();
        let r = engine.download(request, move |p| drop(events.send(JobEvent::Progress(p))), cancel.clone()).await;
        let state = app.state::<AppState>();
        state.jobs.lock().unwrap().remove(&id);
        let event = match r {
            Ok(r) => {
                let files: Vec<String> = r.files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
                state.store.add_download(&video_id, Download { files: files.clone(), clip, at: now_ms() });
                app.emit("downloaded", &files).ok();
                JobEvent::Done { files, bytes: r.bytes, seconds: r.seconds }
            }
            Err(dyt_engine::Error::Cancelled) => JobEvent::Cancelled,
            Err(error) => {
                log::warn!("download {id} failed: {error}");
                JobEvent::Failed { error }
            }
        };
        on_event.send(event).ok();
    });
    Ok(id)
}

#[tauri::command]
fn cancel(id: u64, state: State<'_, AppState>) {
    if let Some(c) = state.jobs.lock().unwrap().get(&id) {
        c.cancel();
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.store.settings()
}

#[tauri::command]
fn set_settings(settings: Settings, state: State<'_, AppState>) {
    let old = state.store.settings();
    if old.connections != settings.connections {
        if let Ok(e) = Engine::new(engine_config(&settings)) {
            *state.engine.lock().unwrap() = e;
        }
    }
    state.store.set_settings(settings);
}

#[tauri::command]
fn history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state.store.history()
}

#[tauri::command]
fn history_remove(video_id: String, state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state.store.remove(&video_id)
}

#[tauri::command]
fn history_clear(state: State<'_, AppState>) {
    state.store.clear()
}

#[derive(Serialize)]
struct Folder {
    label: &'static str,
    path: String,
}

/// Where downloads can go without a folder picker: the shared folders on Android, the
/// usual XDG ones on desktop.
#[tauri::command]
fn folders(app: AppHandle) -> Vec<Folder> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        let root = "/storage/emulated/0";
        [("Downloads", "Download"), ("Movies", "Movies"), ("Music", "Music"), ("Documents", "Documents")]
            .into_iter()
            .map(|(label, d)| Folder { label, path: format!("{root}/{d}") })
            .collect()
    }
    #[cfg(not(target_os = "android"))]
    {
        let p = app.path();
        [("Downloads", p.download_dir()), ("Videos", p.video_dir()), ("Music", p.audio_dir()), ("Desktop", p.desktop_dir())]
            .into_iter()
            .filter_map(|(label, d)| d.ok().map(|d| Folder { label, path: d.to_string_lossy().into_owned() }))
            .collect()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Platform {
    os: &'static str,
    ffmpeg: bool,
    version: &'static str,
}

#[tauri::command]
fn platform(state: State<'_, AppState>) -> Platform {
    Platform { os: std::env::consts::OS, ffmpeg: engine(&state).ffmpeg().is_some(), version: env!("CARGO_PKG_VERSION") }
}

/// YouTube's clients change; the project publishes the current list as clients.json so a fix
/// reaches installed apps without an update. The cached copy applies at once, a fresh one when
/// it arrives; anything malformed is ignored and the built-in list stays.
fn update_clients(app: AppHandle) {
    const URL: &str = "https://dixonsolutions.github.io/dYT/clients.json";
    let cache = app.path().app_data_dir().map(|d| d.join("clients.json")).ok();
    if let Some(list) = cache.as_ref().and_then(|c| std::fs::read_to_string(c).ok()).and_then(|j| dyt_engine::innertube::clients_from_json(&j).ok()) {
        engine(&app.state::<AppState>()).set_clients(list);
    }
    tauri::async_runtime::spawn(async move {
        let engine = engine(&app.state::<AppState>());
        let fetch = async {
            let body = engine.fetch_text(URL).await.ok()?;
            dyt_engine::innertube::clients_from_json(&body).ok().map(|l| (l, body))
        };
        if let Some((list, body)) = fetch.await {
            engine.set_clients(list);
            if let Some(c) = cache {
                std::fs::write(c, body).ok();
            }
        }
    });
}

/// Quit even though downloads are running (the user confirmed).
#[tauri::command]
fn quit(app: AppHandle, state: State<'_, AppState>) {
    state.quitting.store(true, Ordering::SeqCst);
    for c in state.jobs.lock().unwrap().values() {
        c.cancel();
    }
    // Give the cancelled jobs a moment to remove their work directories.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        app.exit(0);
    });
}

/// The link the app was opened with, once.
#[tauri::command]
fn take_pending_link(state: State<'_, AppState>) -> Option<String> {
    state.pending_link.lock().unwrap().take()
}

fn link_from_args(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| dyt_engine::id::parse(a).is_some()).cloned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        // A second launch (`dyt <link>` from a browser or a terminal) hands its link to the
        // window that's already open.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                w.unminimize().ok();
                w.set_focus().ok();
            }
            if let Some(link) = link_from_args(&args) {
                app.emit("open-link", link).ok();
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(theme::Current::default())
        .setup(|app| {
            let dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
            let store = Store::open(dir);
            let engine = Engine::new(engine_config(&store.settings()))?;
            let args: Vec<String> = std::env::args().collect();
            app.manage(AppState {
                engine: Mutex::new(engine),
                jobs: Mutex::new(HashMap::new()),
                next_job: AtomicU64::new(0),
                store,
                pending_link: Mutex::new(link_from_args(&args)),
                quitting: std::sync::atomic::AtomicBool::new(false),
            });
            theme::watch(app.handle().clone());
            update_clients(app.handle().clone());
            // Work left behind by a crash or a forced quit, in every folder downloads go to.
            let mut dirs: Vec<PathBuf> = folders(app.handle().clone()).into_iter().map(|f| PathBuf::from(f.path)).collect();
            dirs.extend(app.state::<AppState>().store.settings().folder.map(PathBuf::from));
            std::thread::spawn(move || dirs.iter().for_each(|d| dyt_engine::clean_stale_work(d)));
            Ok(())
        })
        // Closing the window mid-download asks first instead of silently dropping the work.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let running = state.jobs.lock().unwrap().len();
                if running > 0 && !state.quitting.load(Ordering::SeqCst) {
                    api.prevent_close();
                    window.emit("close-requested", running).ok();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            resolve,
            parse_link,
            download,
            cancel,
            get_settings,
            set_settings,
            history,
            history_remove,
            history_clear,
            folders,
            platform,
            take_pending_link,
            quit,
            theme::system_theme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running dYT");
}

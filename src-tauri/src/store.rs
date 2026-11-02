//! Settings and history, kept as small JSON files in the app's data directory.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Where downloads go; the platform's Downloads folder when unset.
    pub folder: Option<String>,
    /// `quality` (AV1/VP9/Opus) or `compatible` (H.264/AAC in MP4).
    pub preference: String,
    /// Parallel connections per download.
    pub connections: u32,
    /// Remembered quality cap: `best` or a height like `1080`.
    pub quality: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { folder: None, preference: "quality".into(), connections: 8, quality: "best".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub files: Vec<String>,
    pub clip: Option<(f64, f64)>,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub video_id: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub thumbnail: String,
    pub duration: f64,
    /// Unix milliseconds of the last time it was opened.
    pub at: u64,
    #[serde(default)]
    pub downloads: Vec<Download>,
}

const MAX_HISTORY: usize = 500;

pub struct Store {
    dir: PathBuf,
    settings: Mutex<Settings>,
    history: Mutex<Vec<HistoryEntry>>,
}

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Write via a temp file and rename, so a crash never leaves half a file.
fn save<T: Serialize>(path: &Path, value: &T) {
    let tmp = path.with_extension("json.tmp");
    let Ok(bytes) = serde_json::to_vec_pretty(value) else { return };
    if std::fs::write(&tmp, bytes).is_ok() {
        if let Err(e) = std::fs::rename(&tmp, path) {
            log::warn!("saving {}: {e}", path.display());
        }
    }
}

impl Store {
    pub fn open(dir: PathBuf) -> Store {
        std::fs::create_dir_all(&dir).ok();
        let settings = load(&dir.join("settings.json"));
        let history = load(&dir.join("history.json"));
        Store { dir, settings: Mutex::new(settings), history: Mutex::new(history) }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, s: Settings) {
        save(&self.dir.join("settings.json"), &s);
        *self.settings.lock().unwrap() = s;
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.history.lock().unwrap().clone()
    }

    fn persist(&self, h: &[HistoryEntry]) {
        save(&self.dir.join("history.json"), &h);
    }

    /// Record an opened video: newest first, one entry per video, downloads kept.
    pub fn touch(&self, mut e: HistoryEntry) -> Vec<HistoryEntry> {
        let mut h = self.history.lock().unwrap();
        if let Some(i) = h.iter().position(|x| x.video_id == e.video_id) {
            let old = h.remove(i);
            e.downloads = old.downloads;
        }
        h.insert(0, e);
        h.truncate(MAX_HISTORY);
        self.persist(&h);
        h.clone()
    }

    pub fn add_download(&self, video_id: &str, d: Download) {
        let mut h = self.history.lock().unwrap();
        if let Some(e) = h.iter_mut().find(|x| x.video_id == video_id) {
            e.downloads.insert(0, d);
            e.downloads.truncate(20);
            self.persist(&h);
        }
    }

    pub fn remove(&self, video_id: &str) -> Vec<HistoryEntry> {
        let mut h = self.history.lock().unwrap();
        h.retain(|x| x.video_id != video_id);
        self.persist(&h);
        h.clone()
    }

    pub fn clear(&self) {
        let mut h = self.history.lock().unwrap();
        h.clear();
        self.persist(&h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, at: u64) -> HistoryEntry {
        HistoryEntry { video_id: id.into(), url: format!("https://youtu.be/{id}"), title: id.into(), author: String::new(), thumbnail: String::new(), duration: 1.0, at, downloads: vec![] }
    }

    #[test]
    fn history_dedupes_keeps_downloads_and_persists() {
        let dir = std::env::temp_dir().join(format!("dyt-store-{}", std::process::id()));
        let s = Store::open(dir.clone());
        s.touch(entry("a", 1));
        s.touch(entry("b", 2));
        s.add_download("a", Download { files: vec!["/x.mp4".into()], clip: None, at: 3 });
        let h = s.touch(entry("a", 4));
        assert_eq!(h.iter().map(|e| e.video_id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(h[0].downloads.len(), 1, "re-opening keeps the downloads");
        let again = Store::open(dir.clone());
        assert_eq!(again.history().len(), 2);
        again.set_settings(Settings { connections: 4, ..Settings::default() });
        assert_eq!(Store::open(dir.clone()).settings().connections, 4);
        assert_eq!(again.remove("b").len(), 1);
        again.clear();
        assert!(Store::open(dir.clone()).history().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}

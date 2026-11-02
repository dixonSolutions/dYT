use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Video,
    Audio,
    /// Video and audio in one file (the legacy progressive formats).
    Muxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mp4,
    Webm,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    pub itag: u32,
    pub kind: StreamKind,
    pub container: Container,
    /// Short codec family: `avc1`, `vp9`, `av1`, `hevc`, `mp4a`, `opus`, `vorbis`.
    pub codec: String,
    /// Full codecs string from the mime type, e.g. `av01.0.08M.08`.
    pub codecs: String,
    pub mime: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
    pub hdr: bool,
    pub bitrate: u64,
    pub content_length: Option<u64>,
    pub duration: Option<f64>,
    pub quality_label: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub audio_track: Option<AudioTrack>,
    /// Dynamic-range-compressed audio (quieter parts boosted); never the default pick.
    pub drc: bool,
    pub init_range: Option<(u64, u64)>,
    pub index_range: Option<(u64, u64)>,
    pub url: String,
}

impl Format {
    pub fn has_video(&self) -> bool {
        matches!(self.kind, StreamKind::Video | StreamKind::Muxed)
    }
    pub fn has_audio(&self) -> bool {
        matches!(self.kind, StreamKind::Audio | StreamKind::Muxed)
    }
    /// The best size estimate we have, in bytes.
    pub fn size_estimate(&self, duration: f64) -> Option<u64> {
        self.content_length.or_else(|| (self.bitrate > 0 && duration > 0.0).then(|| (self.bitrate as f64 / 8.0 * duration) as u64))
    }
    /// Extension a raw (unmuxed) copy of this stream should carry.
    pub fn raw_ext(&self) -> &'static str {
        match (self.container, self.kind) {
            (Container::Mp4, StreamKind::Audio) => "m4a",
            (Container::Mp4, _) => "mp4",
            (Container::Webm, _) => "webm",
            _ => "bin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardLevel {
    /// URL of sheet `n` is `url.replace("$M", n)`.
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub count: u32,
    pub cols: u32,
    pub rows: u32,
    /// Milliseconds between frames; 0 means "spread evenly over the video".
    pub interval_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub author: String,
    pub channel_id: String,
    pub duration: f64,
    pub view_count: Option<u64>,
    pub is_live: bool,
    pub thumbnail: String,
    pub formats: Vec<Format>,
    /// Unix seconds after which the stream URLs stop working.
    pub expires_at: u64,
    /// InnerTube client that produced the streams.
    pub client: String,
    pub storyboard: Vec<StoryboardLevel>,
}

impl VideoInfo {
    pub fn format(&self, itag: u32) -> Option<&Format> {
        self.formats.iter().find(|f| f.itag == itag)
    }
    pub fn watch_url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.id)
    }
}

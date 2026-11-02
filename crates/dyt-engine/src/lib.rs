//! dYT's download engine: YouTube extraction over InnerTube, a parallel range downloader,
//! segment-exact clips, and stream-copy merging through ffmpeg.

pub mod engine;
pub mod error;
pub mod fetch;
pub mod ffmpeg;
pub mod id;
pub mod index;
pub mod innertube;
mod jsc;
pub mod model;
pub mod names;
pub mod select;

pub use engine::{clean_stale_work, DownloadRequest, DownloadResult, Engine, EngineConfig, Progress, Stage};
pub use error::{Error, Result};
pub use model::{Container, Format, StreamKind, VideoInfo};
pub use select::Preference;
pub use tokio_util::sync::CancellationToken;

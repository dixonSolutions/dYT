use serde::Serialize;

/// Everything the engine can fail with. `kind()` is a stable machine code the UI maps to a message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("That doesn't look like a YouTube link or video id")]
    InvalidUrl,
    #[error("This video is unavailable: {0}")]
    Unavailable(String),
    #[error("This video is age-restricted and needs a signed-in account")]
    AgeRestricted,
    #[error("YouTube asked to confirm this isn't a bot; try again in a moment")]
    BotCheck,
    #[error("Live streams can't be downloaded while they're live")]
    Live,
    #[error("No downloadable stream matches the selection")]
    NoFormat,
    #[error("Network error: {0}")]
    Network(String),
    #[error("YouTube answered {status} while {what}")]
    Http { status: u16, what: String },
    #[error("Unexpected response from YouTube: {0}")]
    Parse(String),
    #[error("ffmpeg isn't available, so streams can't be merged or trimmed")]
    FfmpegMissing,
    #[error("ffmpeg failed: {0}")]
    Ffmpeg(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("Cancelled")]
    Cancelled,
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::InvalidUrl => "invalid_url",
            Error::Unavailable(_) => "unavailable",
            Error::AgeRestricted => "age_restricted",
            Error::BotCheck => "bot_check",
            Error::Live => "live",
            Error::NoFormat => "no_format",
            Error::Network(_) => "network",
            Error::Http { .. } => "http",
            Error::Parse(_) => "parse",
            Error::FfmpegMissing => "ffmpeg_missing",
            Error::Ffmpeg(_) => "ffmpeg",
            Error::Io(_) => "io",
            Error::Cancelled => "cancelled",
        }
    }

    /// Worth retrying the same request (transient network trouble, 5xx, throttling).
    pub fn is_transient(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Http { status, .. } => *status == 429 || *status >= 500,
            _ => false,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if let Some(status) = e.status() {
            return Error::Http { status: status.as_u16(), what: "requesting".into() };
        }
        // reqwest's Display hides the cause ("error sending request"); include the chain.
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        Error::Network(msg)
    }
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Error", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type Result<T> = std::result::Result<T, Error>;

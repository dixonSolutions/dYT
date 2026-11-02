//! Picking streams: the best video under a height cap, the best audio, and the container
//! the merged file goes into.

use serde::{Deserialize, Serialize};

use crate::model::{Format, StreamKind, VideoInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preference {
    /// Highest quality per byte: AV1 > VP9 > H.264, Opus > AAC.
    #[default]
    Quality,
    /// Plays everywhere: H.264 + AAC in MP4.
    Compatible,
}

fn video_codec_rank(codec: &str, pref: Preference) -> u32 {
    match (pref, codec) {
        (Preference::Compatible, "avc1") => 10,
        (Preference::Compatible, _) => 0,
        (_, "av1") => 4,
        (_, "vp9") => 3,
        (_, "hevc") => 2,
        (_, "avc1") => 1,
        _ => 0,
    }
}

fn audio_codec_rank(codec: &str, pref: Preference) -> u32 {
    match (pref, codec) {
        (Preference::Compatible, "mp4a") => 10,
        (Preference::Compatible, _) => 0,
        (_, "opus") => 3,
        (_, "mp4a") => 2,
        (_, "vorbis") => 1,
        _ => 0,
    }
}

/// Best video-only stream at or under `max_height` (or the smallest one above it when nothing
/// fits). SDR is preferred over HDR: HDR looks washed out on most screens and players.
pub fn pick_video(info: &VideoInfo, max_height: Option<u32>, pref: Preference) -> Option<&Format> {
    let cands: Vec<&Format> = info.formats.iter().filter(|f| f.kind == StreamKind::Video && f.height.is_some()).collect();
    let fits = |f: &&Format| max_height.is_none_or(|m| f.height.unwrap_or(0) <= m);
    let key = |f: &&Format| {
        (
            video_codec_rank(&f.codec, pref) > 0,
            f.height.unwrap_or(0),
            f.fps.unwrap_or(0),
            !f.hdr,
            video_codec_rank(&f.codec, pref),
            f.bitrate,
        )
    };
    cands.iter().copied().filter(fits).max_by_key(key).or_else(|| {
        // Nothing small enough: take the lowest resolution available rather than nothing.
        cands.iter().copied().min_by_key(|f| (f.height.unwrap_or(0), std::cmp::Reverse(key(f))))
    })
}

/// Best audio stream: the default language track, never the DRC variant when a plain one exists.
pub fn pick_audio(info: &VideoInfo, pref: Preference) -> Option<&Format> {
    info.formats.iter().filter(|f| f.kind == StreamKind::Audio).max_by_key(|f| {
        (
            f.audio_track.as_ref().is_none_or(|t| t.is_default),
            !f.drc,
            audio_codec_rank(&f.codec, pref) > 0,
            // Opus at 130k beats AAC at 128k, but 48k opus doesn't beat 128k AAC.
            f.bitrate / 32_000,
            audio_codec_rank(&f.codec, pref),
            f.bitrate,
        )
    })
}

/// Distinct heights on offer, highest first (for the quality picker).
pub fn heights(info: &VideoInfo) -> Vec<u32> {
    let mut h: Vec<u32> = info.formats.iter().filter(|f| f.kind == StreamKind::Video).filter_map(|f| f.height).collect();
    h.sort_unstable_by(|a, b| b.cmp(a));
    h.dedup();
    h
}

/// Extension for the merged or remuxed output.
pub fn output_ext(video: Option<&Format>, audio: Option<&Format>, clip: bool) -> &'static str {
    match (video, audio) {
        (None, Some(a)) => match a.codec.as_str() {
            "opus" => "opus",
            "vorbis" => "ogg",
            _ => "m4a",
        },
        (Some(v), a) => {
            let webm_ok = matches!(v.codec.as_str(), "vp9" | "av1") && a.is_none_or(|a| matches!(a.codec.as_str(), "opus" | "vorbis"));
            // Clips go to MP4 even for VP9/Opus: its edit list makes the cut start exactly on
            // the requested timestamp instead of the keyframe before it.
            if webm_ok && !clip {
                "webm"
            } else if matches!(v.codec.as_str(), "avc1" | "av1" | "vp9" | "hevc") && a.is_none_or(|a| matches!(a.codec.as_str(), "mp4a" | "opus" | "ac3")) {
                "mp4"
            } else {
                "mkv"
            }
        }
        (None, None) => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Container;

    fn f(itag: u32, kind: StreamKind, codec: &str, h: Option<u32>, fps: u32, bitrate: u64) -> Format {
        Format {
            itag,
            kind,
            container: if matches!(codec, "avc1" | "mp4a" | "av1") { Container::Mp4 } else { Container::Webm },
            codec: codec.into(),
            codecs: codec.into(),
            mime: String::new(),
            width: h.map(|h| h * 16 / 9),
            height: h,
            fps: Some(fps),
            hdr: false,
            bitrate,
            content_length: Some(bitrate),
            duration: Some(10.0),
            quality_label: None,
            audio_sample_rate: None,
            audio_channels: None,
            audio_track: None,
            drc: false,
            init_range: None,
            index_range: None,
            url: String::new(),
        }
    }

    fn info(formats: Vec<Format>) -> VideoInfo {
        VideoInfo {
            id: "x".into(),
            title: "t".into(),
            author: String::new(),
            channel_id: String::new(),
            duration: 10.0,
            view_count: None,
            is_live: false,
            thumbnail: String::new(),
            formats,
            expires_at: 0,
            client: String::new(),
            storyboard: vec![],
        }
    }

    fn sample() -> VideoInfo {
        use StreamKind::*;
        info(vec![
            f(137, Video, "avc1", Some(1080), 30, 3_000_000),
            f(248, Video, "vp9", Some(1080), 30, 1_200_000),
            f(399, Video, "av1", Some(1080), 30, 1_100_000),
            f(313, Video, "vp9", Some(2160), 30, 13_000_000),
            f(136, Video, "avc1", Some(720), 30, 1_000_000),
            f(298, Video, "avc1", Some(720), 60, 1_500_000),
            f(140, Audio, "mp4a", None, 0, 130_000),
            f(251, Audio, "opus", None, 0, 129_000),
            f(250, Audio, "opus", None, 0, 61_000),
        ])
    }

    #[test]
    fn best_video_respects_cap_and_codec() {
        let i = sample();
        assert_eq!(pick_video(&i, None, Preference::Quality).unwrap().itag, 313);
        assert_eq!(pick_video(&i, Some(1080), Preference::Quality).unwrap().itag, 399);
        assert_eq!(pick_video(&i, Some(1080), Preference::Compatible).unwrap().itag, 137);
        assert_eq!(pick_video(&i, Some(720), Preference::Quality).unwrap().itag, 298, "60fps beats 30fps");
        assert_eq!(pick_video(&i, Some(144), Preference::Quality).unwrap().itag, 298, "falls back to the smallest");
    }

    #[test]
    fn best_audio() {
        let i = sample();
        assert_eq!(pick_audio(&i, Preference::Quality).unwrap().itag, 251);
        assert_eq!(pick_audio(&i, Preference::Compatible).unwrap().itag, 140);
    }

    #[test]
    fn audio_skips_drc_and_dubs() {
        use crate::model::AudioTrack;
        let mut i = sample();
        let mut drc = f(9251, StreamKind::Audio, "opus", None, 0, 160_000);
        drc.drc = true;
        let mut dub = f(9252, StreamKind::Audio, "opus", None, 0, 200_000);
        dub.audio_track = Some(AudioTrack { id: "de.3".into(), name: "German".into(), is_default: false });
        i.formats.push(drc);
        i.formats.push(dub);
        assert_eq!(pick_audio(&i, Preference::Quality).unwrap().itag, 251);
    }

    #[test]
    fn containers() {
        let i = sample();
        let (av1, vp9, avc, opus, aac) = (i.format(399), i.format(313), i.format(137), i.format(251), i.format(140));
        assert_eq!(output_ext(vp9, opus, false), "webm");
        assert_eq!(output_ext(av1, opus, false), "webm");
        assert_eq!(output_ext(vp9, opus, true), "mp4");
        assert_eq!(output_ext(avc, aac, false), "mp4");
        assert_eq!(output_ext(vp9, aac, false), "mp4");
        assert_eq!(output_ext(None, opus, false), "opus");
        assert_eq!(output_ext(None, aac, true), "m4a");
        assert_eq!(output_ext(avc, None, false), "mp4");
        assert_eq!(heights(&i), vec![2160, 1080, 720]);
    }
}

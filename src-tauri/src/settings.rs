use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Options that decide what a single download produces. A copy is stored on
/// every queue item so that items can be tweaked individually.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ItemOptions {
    /// "video" or "audio"
    pub mode: String,
    /// "best", "2160", "1440", "1080", "720", "480", "360"
    pub video_quality: String,
    /// "mp4", "mkv", "webm"
    pub container: String,
    /// "opus", "m4a", "mp3", "flac", "wav"
    pub audio_format: String,
    /// "best", "320", "192", "128"
    pub audio_quality: String,
    /// Exact yt-dlp format selector chosen in the format picker (overrides the above).
    pub format_id: Option<String>,
    /// Optional clip, e.g. "1:30" .. "3:45"
    pub clip_start: Option<String>,
    pub clip_end: Option<String>,
}

impl Default for ItemOptions {
    fn default() -> Self {
        Self {
            mode: "video".into(),
            video_quality: "1080".into(),
            container: "mp4".into(),
            audio_format: "opus".into(),
            audio_quality: "best".into(),
            format_id: None,
            clip_start: None,
            clip_end: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub defaults: ItemOptions,
    pub output_dir: String,
    pub parallel: usize,

    // File naming
    pub filename_template: String,
    pub playlist_subfolder: bool,
    pub playlist_numbering: bool,
    pub restrict_filenames: bool,
    /// "skip" or "overwrite"
    pub overwrite: String,

    // Embedding
    pub embed_thumbnail: bool,
    pub embed_metadata: bool,
    pub embed_chapters: bool,
    pub split_chapters: bool,

    // Subtitles
    pub subtitles: bool,
    pub sub_langs: String,
    pub auto_subs: bool,
    pub embed_subs: bool,
    /// "srt" or "vtt"
    pub sub_format: String,

    // SponsorBlock: "off", "mark", "remove"
    pub sponsorblock: String,
    pub sponsorblock_categories: Vec<String>,

    // Access
    pub cookies_browser: String,
    pub cookies_profile: String,
    pub cookies_file: String,

    // Network
    pub rate_limit: String,
    pub concurrent_fragments: u32,
    pub retries: u32,
    pub proxy: String,

    // Playlists
    pub playlist_items: String,
    pub playlist_reverse: bool,
    pub playlist_random: bool,
    pub use_archive: bool,

    // Formats
    /// "any", "h264", "vp9", "av1"
    pub codec_pref: String,
    pub force_keyframes: bool,

    // App
    /// "stable" or "nightly"
    pub channel: String,
    pub auto_update: bool,
    pub custom_ytdlp: String,
    pub notifications: bool,
    pub extra_args: String,
    pub last_update_check: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            defaults: ItemOptions::default(),
            output_dir: default_output_dir().to_string_lossy().into_owned(),
            parallel: 2,
            filename_template: "%(title)s.%(ext)s".into(),
            playlist_subfolder: true,
            playlist_numbering: true,
            restrict_filenames: false,
            overwrite: "skip".into(),
            embed_thumbnail: true,
            embed_metadata: true,
            embed_chapters: true,
            split_chapters: false,
            subtitles: false,
            sub_langs: "en".into(),
            auto_subs: false,
            embed_subs: true,
            sub_format: "srt".into(),
            sponsorblock: "off".into(),
            sponsorblock_categories: vec![
                "sponsor".into(),
                "selfpromo".into(),
                "interaction".into(),
            ],
            cookies_browser: String::new(),
            cookies_profile: String::new(),
            cookies_file: String::new(),
            rate_limit: String::new(),
            concurrent_fragments: 4,
            retries: 10,
            proxy: String::new(),
            playlist_items: String::new(),
            playlist_reverse: false,
            playlist_random: false,
            use_archive: false,
            codec_pref: "any".into(),
            force_keyframes: false,
            channel: "stable".into(),
            auto_update: true,
            custom_ytdlp: String::new(),
            notifications: true,
            extra_args: String::new(),
            last_update_check: 0,
        }
    }
}

fn default_output_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    // Respect XDG on Linux when set.
    if let Ok(xdg) = std::env::var("XDG_DOWNLOAD_DIR") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("YouGet");
        }
    }
    home.join("Downloads").join("YouGet")
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        write_private(path, &json)
    }
}

pub(crate) fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let result = file.write_all(contents);
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn old_settings_keep_values_and_fill_new_defaults() {
        let settings: Settings = serde_json::from_str(
            r#"{
            "output_dir":"/tmp/media","parallel":3,"defaults":{"mode":"audio","audio_format":"mp3"}
        }"#,
        )
        .unwrap();
        assert_eq!(settings.output_dir, "/tmp/media");
        assert_eq!(settings.parallel, 3);
        assert_eq!(settings.defaults.mode, "audio");
        assert_eq!(settings.defaults.audio_format, "mp3");
        assert_eq!(
            settings.defaults.video_quality,
            ItemOptions::default().video_quality
        );
        assert!(settings.auto_update);
    }

    #[test]
    fn private_write_replaces_old_file_with_owner_only_permissions() {
        let path = std::env::temp_dir().join(format!("youget-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, b"new").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o077, 0);
        assert_eq!(saved, b"new");
    }
}

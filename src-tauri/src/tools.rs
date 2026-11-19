//! Locates, downloads, verifies and updates the external tools YouGet drives:
//! yt-dlp (always managed by us unless the user picks a custom binary),
//! ffmpeg/ffprobe and Deno (system copy preferred, downloaded as a fallback).

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ToolInfo {
    pub path: Option<String>,
    pub version: Option<String>,
    /// "checking" | "downloading" | "updating" | "ok" | "missing" | "error"
    pub status: String,
    pub message: Option<String>,
    /// true when the binary lives in YouGet's own data directory
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ToolsState {
    pub ytdlp: ToolInfo,
    pub ffmpeg: ToolInfo,
    pub deno: ToolInfo,
}

pub struct Tools {
    pub bin_dir: PathBuf,
    pub state: Mutex<ToolsState>,
}

#[derive(Clone, Copy)]
enum Which {
    YtDlp,
    Ffmpeg,
    Deno,
}

const UA: &str = concat!("YouGet/", env!("CARGO_PKG_VERSION"));

impl Tools {
    pub fn new(bin_dir: PathBuf) -> Self {
        let mut st = ToolsState::default();
        for t in [&mut st.ytdlp, &mut st.ffmpeg, &mut st.deno] {
            t.status = "checking".into();
        }
        Self { bin_dir, state: Mutex::new(st) }
    }

    pub fn snapshot(&self) -> ToolsState {
        self.state.lock().unwrap().clone()
    }

    pub fn ytdlp_path(&self) -> Option<PathBuf> {
        let st = self.state.lock().unwrap();
        if st.ytdlp.status == "ok" {
            st.ytdlp.path.as_ref().map(PathBuf::from)
        } else {
            None
        }
    }

    pub fn ffmpeg_dir(&self) -> Option<PathBuf> {
        let st = self.state.lock().unwrap();
        if st.ffmpeg.status != "ok" {
            return None;
        }
        st.ffmpeg.path.as_ref().and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
    }

    pub fn deno_path(&self) -> Option<PathBuf> {
        let st = self.state.lock().unwrap();
        if st.deno.status != "ok" {
            return None;
        }
        st.deno.path.as_ref().map(PathBuf::from)
    }

    fn set(&self, app: &AppHandle, which: Which, f: impl FnOnce(&mut ToolInfo)) {
        let snap = {
            let mut st = self.state.lock().unwrap();
            let t = match which {
                Which::YtDlp => &mut st.ytdlp,
                Which::Ffmpeg => &mut st.ffmpeg,
                Which::Deno => &mut st.deno,
            };
            f(t);
            st.clone()
        };
        let _ = app.emit("tools", snap);
    }

    /// Make sure every tool is available. Safe to call repeatedly.
    pub async fn ensure_all(&self, app: &AppHandle, custom_ytdlp: &str) {
        let _ = std::fs::create_dir_all(&self.bin_dir);
        // Run the three in parallel; they're independent.
        tokio::join!(
            self.ensure_ytdlp(app, custom_ytdlp),
            self.ensure_ffmpeg(app),
            self.ensure_deno(app)
        );
    }

    // ---------------------------------------------------------------- yt-dlp

    async fn ensure_ytdlp(&self, app: &AppHandle, custom: &str) {
        let w = Which::YtDlp;
        if !custom.trim().is_empty() {
            let p = PathBuf::from(custom.trim());
            match version_of(&p, &["--version"]).await {
                Some(v) => self.set(app, w, |t| ok(t, &p, v, false)),
                None => self.set(app, w, |t| {
                    t.status = "error".into();
                    t.path = Some(p.to_string_lossy().into());
                    t.message = Some("The custom yt-dlp binary could not be run.".into());
                }),
            }
            return;
        }

        let managed = self.bin_dir.join("yt-dlp");
        if let Some(v) = version_of(&managed, &["--version"]).await {
            self.set(app, w, |t| ok(t, &managed, v, true));
            return;
        }

        let Some(asset) = ytdlp_asset() else {
            return self.fallback_system_ytdlp(app, "No standalone yt-dlp build for this platform.").await;
        };
        let base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
        let res = self
            .download_with_sums(app, w, &format!("{base}/{asset}"), &format!("{base}/SHA2-256SUMS"), asset, &managed)
            .await;
        match res {
            Ok(()) => match version_of(&managed, &["--version"]).await {
                Some(v) => self.set(app, w, |t| ok(t, &managed, v, true)),
                None => self.fallback_system_ytdlp(app, "Downloaded yt-dlp does not run.").await,
            },
            Err(e) => self.fallback_system_ytdlp(app, &e).await,
        }
    }

    async fn fallback_system_ytdlp(&self, app: &AppHandle, why: &str) {
        if let Some((p, v)) = find_system("yt-dlp", &["--version"]).await {
            self.set(app, Which::YtDlp, |t| {
                ok(t, &p, v, false);
                t.message = Some(format!("Using system yt-dlp ({why})"));
            });
        } else {
            let why = why.to_string();
            self.set(app, Which::YtDlp, |t| {
                t.status = "error".into();
                t.message = Some(format!("Could not get yt-dlp: {why}"));
            });
        }
    }

    /// Self-update the managed yt-dlp binary. Returns a human readable summary.
    pub async fn update_ytdlp(&self, app: &AppHandle, channel: &str) -> Result<String, String> {
        let (path, managed) = {
            let st = self.state.lock().unwrap();
            (st.ytdlp.path.clone(), st.ytdlp.managed)
        };
        let Some(path) = path else { return Err("yt-dlp is not installed yet".into()) };
        if !managed {
            return Err("yt-dlp is not managed by YouGet (custom or system binary); update it yourself.".into());
        }
        let path = PathBuf::from(path);
        self.set(app, Which::YtDlp, |t| {
            t.status = "updating".into();
            t.message = Some(format!("Checking for {channel} updates…"));
        });
        let channel = if channel == "nightly" { "nightly" } else { "stable" };
        let out = Command::new(&path)
            .args(["--update-to", channel])
            .env("PYTHONIOENCODING", "utf-8")
            .output()
            .await;
        let text = match &out {
            Ok(o) => format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
            Err(e) => e.to_string(),
        };
        let success = out.map(|o| o.status.success()).unwrap_or(false);
        let version = version_of(&path, &["--version"]).await;
        let summary = text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        self.set(app, Which::YtDlp, |t| {
            t.status = if version.is_some() { "ok".into() } else { "error".into() };
            t.version = version.clone().or(t.version.take());
            t.message = if success { None } else { Some(summary.clone()) };
        });
        if success {
            Ok(summary)
        } else {
            Err(summary)
        }
    }

    // ---------------------------------------------------------------- ffmpeg

    async fn ensure_ffmpeg(&self, app: &AppHandle) {
        let w = Which::Ffmpeg;
        let managed = self.bin_dir.join("ffmpeg");
        let managed_probe = self.bin_dir.join("ffprobe");
        if let (Some(v), Some(_)) = (
            version_of(&managed, &["-version"]).await,
            version_of(&managed_probe, &["-version"]).await,
        ) {
            return self.set(app, w, |t| ok(t, &managed, v, true));
        }
        // System copy: both ffmpeg and ffprobe must exist side by side and actually run
        // (a broken Homebrew install fails with missing dylibs, so we test-run it).
        for dir in search_dirs() {
            let f = dir.join("ffmpeg");
            let p = dir.join("ffprobe");
            if let (Some(v), Some(_)) = (version_of(&f, &["-version"]).await, version_of(&p, &["-version"]).await) {
                return self.set(app, w, |t| ok(t, &f, v, false));
            }
        }

        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            let base = "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release";
            for name in ["ffmpeg", "ffprobe"] {
                let url = format!("{base}/{name}.zip");
                if let Err(e) = self.download_zip_with_sidecar(app, w, &url, name, &self.bin_dir.join(name)).await {
                    return self.set(app, w, |t| {
                        t.status = "error".into();
                        t.message = Some(format!("Could not download {name}: {e}"));
                    });
                }
            }
            match version_of(&managed, &["-version"]).await {
                Some(v) => self.set(app, w, |t| ok(t, &managed, v, true)),
                None => self.set(app, w, |t| {
                    t.status = "error".into();
                    t.message = Some("Downloaded ffmpeg does not run.".into());
                }),
            }
        } else {
            let hint = if cfg!(target_os = "macos") {
                "Install it with: brew install ffmpeg"
            } else {
                "Install it with your package manager (e.g. pacman -S ffmpeg or apt install ffmpeg)."
            };
            self.set(app, w, |t| {
                t.status = "missing".into();
                t.message = Some(format!("ffmpeg not found. It is needed to merge video and audio. {hint}"));
            });
        }
    }

    // ---------------------------------------------------------------- deno

    async fn ensure_deno(&self, app: &AppHandle) {
        let w = Which::Deno;
        let managed = self.bin_dir.join("deno");
        if let Some(v) = version_of(&managed, &["--version"]).await {
            return self.set(app, w, |t| ok(t, &managed, first_word_after(&v, "deno"), true));
        }
        if let Some((p, v)) = find_system("deno", &["--version"]).await {
            return self.set(app, w, |t| ok(t, &p, first_word_after(&v, "deno"), false));
        }
        let Some(target) = deno_target() else {
            return self.set(app, w, |t| {
                t.status = "missing".into();
                t.message = Some("No Deno build for this platform.".into());
            });
        };
        let url = format!("https://github.com/denoland/deno/releases/latest/download/deno-{target}.zip");
        match self.download_zip_with_sidecar(app, w, &url, "deno", &managed).await {
            Ok(()) => match version_of(&managed, &["--version"]).await {
                Some(v) => self.set(app, w, |t| ok(t, &managed, first_word_after(&v, "deno"), true)),
                None => self.set(app, w, |t| {
                    t.status = "error".into();
                    t.message = Some("Downloaded Deno does not run.".into());
                }),
            },
            Err(e) => self.set(app, w, |t| {
                t.status = "missing".into();
                t.message = Some(format!(
                    "Could not download Deno ({e}). Some YouTube formats may be unavailable without it."
                ));
            }),
        }
    }

    // ---------------------------------------------------------------- downloads

    /// Download a single binary whose checksum is listed in a SHA2-256SUMS style file.
    async fn download_with_sums(
        &self,
        app: &AppHandle,
        which: Which,
        url: &str,
        sums_url: &str,
        asset: &str,
        dest: &Path,
    ) -> Result<(), String> {
        let client = client()?;
        let sums = client.get(sums_url).send().await.map_err(err)?.error_for_status().map_err(err)?.text().await.map_err(err)?;
        let expected = sums
            .lines()
            .find_map(|l| {
                let mut it = l.split_whitespace();
                let h = it.next()?;
                let n = it.next()?.trim_start_matches('*');
                (n == asset).then(|| h.to_lowercase())
            })
            .ok_or("checksum not published")?;
        let (bytes, _) = self.fetch(app, which, &client, url, asset).await?;
        verify(&bytes, &expected)?;
        write_exec(dest, &bytes)
    }

    /// Download a zip that has a `<url>.sha256` (or `.sha256sum`) file next to it and
    /// extract the single executable `entry` from it.
    async fn download_zip_with_sidecar(
        &self,
        app: &AppHandle,
        which: Which,
        url: &str,
        entry: &str,
        dest: &Path,
    ) -> Result<(), String> {
        let client = client()?;
        let (bytes, final_url) = self.fetch(app, which, &client, url, entry).await?;
        let mut expected = None;
        for candidate in [format!("{final_url}.sha256"), format!("{url}.sha256sum"), format!("{url}.sha256")] {
            if let Ok(r) = client.get(&candidate).send().await {
                if r.status().is_success() {
                    if let Ok(t) = r.text().await {
                        if let Some(h) = t.split_whitespace().next().filter(|h| h.len() == 64) {
                            expected = Some(h.to_lowercase());
                            break;
                        }
                    }
                }
            }
        }
        let expected = expected.ok_or("checksum not published")?;
        verify(&bytes, &expected)?;
        let entry = entry.to_string();
        let data = tokio::task::spawn_blocking(move || extract_zip_entry(&bytes, &entry))
            .await
            .map_err(err)??;
        write_exec(dest, &data)
    }

    async fn fetch(
        &self,
        app: &AppHandle,
        which: Which,
        client: &reqwest::Client,
        url: &str,
        label: &str,
    ) -> Result<(Vec<u8>, String), String> {
        let mut resp = client.get(url).send().await.map_err(err)?.error_for_status().map_err(err)?;
        let final_url = resp.url().to_string();
        let total = resp.content_length();
        let mut buf: Vec<u8> = Vec::with_capacity(total.unwrap_or(0) as usize);
        let mut last_pct = u64::MAX;
        while let Some(chunk) = resp.chunk().await.map_err(err)? {
            buf.extend_from_slice(&chunk);
            let pct = total.map(|t| buf.len() as u64 * 100 / t.max(1)).unwrap_or(0);
            if pct != last_pct {
                last_pct = pct;
                let msg = match total {
                    Some(_) => format!("Downloading {label}… {pct}%"),
                    None => format!("Downloading {label}… {:.1} MB", buf.len() as f64 / 1e6),
                };
                self.set(app, which, |t| {
                    t.status = "downloading".into();
                    t.message = Some(msg);
                });
            }
        }
        Ok((buf, final_url))
    }
}

// -------------------------------------------------------------------- helpers

fn ok(t: &mut ToolInfo, p: &Path, version: String, managed: bool) {
    t.path = Some(p.to_string_lossy().into_owned());
    t.version = Some(version);
    t.status = "ok".into();
    t.message = None;
    t.managed = managed;
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(err)
}

fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    let got = hex::encode(Sha256::digest(bytes));
    if got == expected {
        Ok(())
    } else {
        Err("checksum mismatch".into())
    }
}

fn write_exec(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    let tmp = dest.with_extension("download");
    std::fs::write(&tmp, bytes).map_err(err)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(err)?;
    std::fs::rename(&tmp, dest).map_err(err)
}

fn extract_zip_entry(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(err)?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(err)?;
        let fname = f.name().rsplit('/').next().unwrap_or("").to_string();
        if f.is_file() && fname == name {
            let mut out = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut out).map_err(err)?;
            return Ok(out);
        }
    }
    Err(format!("{name} not found in archive"))
}

/// Run `bin args` and return the first line of output if it exits successfully.
async fn version_of(bin: &Path, args: &[&str]) -> Option<String> {
    if !bin.is_file() {
        return None;
    }
    let out = tokio::time::timeout(Duration::from_secs(30), Command::new(bin).args(args).output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?.trim().to_string();
    // "ffmpeg version 7.1 Copyright…" -> "7.1"
    if let Some(rest) = line.strip_prefix("ffmpeg version ") {
        return Some(rest.split_whitespace().next().unwrap_or(rest).to_string());
    }
    Some(line)
}

fn first_word_after(s: &str, prefix: &str) -> String {
    s.strip_prefix(prefix)
        .map(|r| r.trim().split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_else(|| s.to_string())
}

/// Directories to look in. GUI apps on macOS do not inherit the shell PATH, so
/// the usual Homebrew/user locations are added explicitly.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut extra = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/snap/bin"),
    ];
    if let Some(h) = home {
        extra.push(h.join(".deno/bin"));
        extra.push(h.join(".local/bin"));
    }
    for d in extra {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs
}

async fn find_system(name: &str, args: &[&str]) -> Option<(PathBuf, String)> {
    for dir in search_dirs() {
        let p = dir.join(name);
        if let Some(v) = version_of(&p, args).await {
            return Some((p, v));
        }
    }
    None
}

fn ytdlp_asset() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("yt-dlp_macos")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("yt-dlp_linux")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("yt-dlp_linux_aarch64")
    } else {
        None
    }
}

fn deno_target() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("aarch64-apple-darwin")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("x86_64-apple-darwin")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("x86_64-unknown-linux-gnu")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("aarch64-unknown-linux-gnu")
    } else {
        None
    }
}

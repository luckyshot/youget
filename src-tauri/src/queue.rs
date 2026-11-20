//! The download queue: metadata fetching, the parallel worker pool, progress
//! parsing and persistence.

use crate::settings::{ItemOptions, Settings};
use crate::tools::Tools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Notify, Semaphore};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub group_id: Option<String>,
    pub group_title: Option<String>,
    pub group_index: Option<u32>,
    /// fetching | queued | downloading | processing | paused | done | error | canceled
    pub status: String,
    pub stage: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub percent: f64,
    pub filepath: Option<String>,
    pub error: Option<String>,
    pub opts: ItemOptions,
    pub added_at: u64,
    /// Automatic retries used for the current download.
    pub attempts: u32,
    /// Unix time before which a queued item must not start (retry back-off).
    pub retry_at: u64,
}

/// Automatic retries for transient failures (HTTP 403/429/5xx, network hiccups).
const RETRY_DELAYS: [u64; 3] = [5, 20, 60];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stop {
    Pause,
    Cancel,
    Remove,
    Shutdown,
}

struct Running {
    pid: i32,
    stop: Option<Stop>,
}

struct Inner {
    settings: Settings,
    items: Vec<Item>,
    running: HashMap<String, Running>,
    logs: HashMap<String, VecDeque<String>>,
    dirty: bool,
    completed_since_idle: usize,
}

pub struct Manager {
    app: AppHandle,
    pub tools: Arc<Tools>,
    inner: Mutex<Inner>,
    wake: Notify,
    fetch_sem: Semaphore,
    data_dir: PathBuf,
}

#[derive(Serialize)]
pub struct FullState {
    pub settings: Settings,
    pub items: Vec<Item>,
    pub tools: crate::tools::ToolsState,
    pub version: String,
    pub platform: String,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

const ACTIVE: [&str; 2] = ["downloading", "processing"];

impl Manager {
    pub fn new(app: AppHandle, data_dir: PathBuf) -> Arc<Self> {
        let settings = Settings::load(&data_dir.join("settings.json"));
        let mut items: Vec<Item> = std::fs::read_to_string(data_dir.join("queue.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // Anything that was in flight when the app closed is resumed.
        for it in items.iter_mut() {
            if ACTIVE.contains(&it.status.as_str()) {
                it.status = "queued".into();
                it.stage = String::new();
                it.speed = None;
                it.eta = None;
            }
        }
        let tools = Arc::new(Tools::new(data_dir.join("bin")));
        Arc::new(Self {
            app,
            tools,
            inner: Mutex::new(Inner {
                settings,
                items,
                running: HashMap::new(),
                logs: HashMap::new(),
                dirty: false,
                completed_since_idle: 0,
            }),
            wake: Notify::new(),
            fetch_sem: Semaphore::new(4),
            data_dir,
        })
    }

    pub fn settings(&self) -> Settings {
        self.inner.lock().unwrap().settings.clone()
    }

    pub fn full_state(&self) -> FullState {
        let inner = self.inner.lock().unwrap();
        FullState {
            settings: inner.settings.clone(),
            items: inner.items.clone(),
            tools: self.tools.snapshot(),
            version: env!("CARGO_PKG_VERSION").into(),
            platform: std::env::consts::OS.into(),
        }
    }

    pub fn save_settings(&self, mut s: Settings) {
        let mut inner = self.inner.lock().unwrap();
        // The UI does not own this field; never let it roll it back.
        s.last_update_check = s.last_update_check.max(inner.settings.last_update_check);
        s.save(&self.data_dir.join("settings.json"));
        inner.settings = s;
        drop(inner);
        self.wake.notify_one();
    }

    pub fn mark_update_checked(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.settings.last_update_check = now();
        inner.settings.save(&self.data_dir.join("settings.json"));
    }

    pub fn is_busy(&self) -> bool {
        !self.inner.lock().unwrap().running.is_empty()
    }

    // ------------------------------------------------------------------ events

    fn emit_item(&self, it: &Item) {
        let _ = self.app.emit("item", it);
    }

    fn emit_items(&self, inner: &Inner) {
        let _ = self.app.emit("items", &inner.items);
    }

    fn with_item<R>(&self, id: &str, f: impl FnOnce(&mut Item) -> R) -> Option<R> {
        let mut inner = self.inner.lock().unwrap();
        inner.dirty = true;
        let it = inner.items.iter_mut().find(|i| i.id == id)?;
        let r = f(it);
        let snap = it.clone();
        drop(inner);
        self.emit_item(&snap);
        Some(r)
    }

    fn log(&self, id: &str, line: &str) {
        let mut inner = self.inner.lock().unwrap();
        let l = inner.logs.entry(id.to_string()).or_default();
        l.push_back(line.to_string());
        while l.len() > 400 {
            l.pop_front();
        }
    }

    pub fn get_log(&self, id: &str) -> Vec<String> {
        self.inner.lock().unwrap().logs.get(id).map(|l| l.iter().cloned().collect()).unwrap_or_default()
    }

    // --------------------------------------------------------------- adding

    pub fn add_urls(self: &Arc<Self>, urls: Vec<String>, opts: Option<ItemOptions>) {
        let mut inner = self.inner.lock().unwrap();
        let opts = opts.unwrap_or_else(|| inner.settings.defaults.clone());
        let mut new_ids = vec![];
        for url in urls {
            let url = url.trim().to_string();
            if url.is_empty() {
                continue;
            }
            let dup = inner
                .items
                .iter()
                .any(|i| i.url == url && !["done", "error", "canceled"].contains(&i.status.as_str()));
            if dup {
                continue;
            }
            let id = new_id();
            inner.items.push(Item {
                id: id.clone(),
                url: url.clone(),
                title: url.clone(),
                status: "fetching".into(),
                stage: "Fetching info…".into(),
                opts: opts.clone(),
                added_at: now(),
                ..Default::default()
            });
            new_ids.push(id);
        }
        inner.dirty = true;
        self.emit_items(&inner);
        drop(inner);
        for id in new_ids {
            let me = self.clone();
            tauri::async_runtime::spawn(async move { me.fetch_info(id).await });
        }
    }

    async fn fetch_info(self: Arc<Self>, id: String) {
        let _permit = self.fetch_sem.acquire().await;
        // Wait for yt-dlp to be ready (first launch downloads it).
        let ytdlp = loop {
            if let Some(p) = self.tools.ytdlp_path() {
                break p;
            }
            if self.tools.snapshot().ytdlp.status == "error" {
                self.with_item(&id, |it| {
                    it.status = "error".into();
                    it.error = Some("yt-dlp is not available".into());
                });
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        let (url, settings) = {
            let inner = self.inner.lock().unwrap();
            let Some(it) = inner.items.iter().find(|i| i.id == id) else { return };
            (it.url.clone(), inner.settings.clone())
        };
        match self.run_json(&ytdlp, &settings, &url, true).await {
            Ok(info) => self.apply_info(&id, info, &ytdlp, &settings).await,
            Err(e) => {
                self.with_item(&id, |it| {
                    it.status = "error".into();
                    it.stage = String::new();
                    it.error = Some(friendly_error(&e));
                });
            }
        }
        self.wake.notify_one();
    }

    /// `yt-dlp -J` for metadata. `flat` expands playlists without visiting each video.
    async fn run_json(&self, ytdlp: &Path, s: &Settings, url: &str, flat: bool) -> Result<Value, String> {
        let mut args: Vec<String> = vec!["-J".into(), "--color".into(), "never".into()];
        if flat {
            args.push("--flat-playlist".into());
            if !s.playlist_items.trim().is_empty() {
                args.extend(["-I".into(), s.playlist_items.trim().into()]);
            }
            if s.playlist_reverse {
                args.push("--playlist-reverse".into());
            }
            if s.playlist_random {
                args.push("--playlist-random".into());
            }
        }
        // A watch URL that also carries &list= means "this video", not the whole list.
        if url.contains("v=") || !flat {
            args.push("--no-playlist".into());
        }
        args.extend(self.access_args(s));
        args.extend(["--".into(), url.into()]);
        let out = Command::new(ytdlp)
            .args(&args)
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let line = err
                .lines()
                .filter(|l| l.starts_with("ERROR"))
                .last()
                .or_else(|| err.lines().last())
                .unwrap_or("yt-dlp failed")
                .to_string();
            return Err(line);
        }
        serde_json::from_slice(&out.stdout).map_err(|e| format!("Could not read yt-dlp output: {e}"))
    }

    /// Cookies, proxy, JS runtime: needed both for metadata and downloads.
    fn access_args(&self, s: &Settings) -> Vec<String> {
        let mut a = vec![];
        if !s.cookies_browser.is_empty() {
            let v = if s.cookies_profile.trim().is_empty() {
                s.cookies_browser.clone()
            } else {
                format!("{}:{}", s.cookies_browser, s.cookies_profile.trim())
            };
            a.extend(["--cookies-from-browser".into(), v]);
        } else if !s.cookies_file.trim().is_empty() {
            a.extend(["--cookies".into(), s.cookies_file.trim().into()]);
        }
        if !s.proxy.trim().is_empty() {
            a.extend(["--proxy".into(), s.proxy.trim().into()]);
        }
        if let Some(deno) = self.tools.deno_path() {
            a.extend(["--js-runtimes".into(), format!("deno:{}", deno.display())]);
        }
        a
    }

    async fn apply_info(self: &Arc<Self>, id: &str, info: Value, ytdlp: &Path, s: &Settings) {
        let is_list = info.get("_type").and_then(Value::as_str) == Some("playlist") || info.get("entries").is_some();
        if !is_list {
            self.with_item(id, |it| {
                fill_from_info(it, &info);
                it.status = "queued".into();
                it.stage = String::new();
            });
            return;
        }

        let group_title = str_of(&info, "title").unwrap_or_else(|| "Playlist".into());
        let mut entries: Vec<Value> = info.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();

        // Channel root URLs return tabs (Videos, Shorts, Live) as nested playlists.
        // Expand those one level deep.
        let mut expanded = vec![];
        for e in entries.drain(..) {
            let nested = e.get("_type").and_then(Value::as_str) == Some("playlist")
                || e.get("ie_key").and_then(Value::as_str) == Some("YoutubeTab");
            if nested {
                if let Some(sub) = e.get("entries").and_then(Value::as_array) {
                    expanded.extend(sub.iter().cloned());
                } else if let Some(u) = str_of(&e, "url") {
                    if let Ok(v) = self.run_json(ytdlp, s, &u, true).await {
                        if let Some(sub) = v.get("entries").and_then(Value::as_array) {
                            expanded.extend(sub.iter().cloned());
                        }
                    }
                }
            } else {
                expanded.push(e);
            }
        }

        let mut inner = self.inner.lock().unwrap();
        let Some(pos) = inner.items.iter().position(|i| i.id == id) else { return };
        let template = inner.items[pos].clone();
        let group_id = new_id();
        let existing: Vec<String> = inner
            .items
            .iter()
            .filter(|i| !["done", "error", "canceled"].contains(&i.status.as_str()))
            .map(|i| i.url.clone())
            .collect();
        let mut new_items = vec![];
        for (n, e) in expanded.iter().enumerate() {
            let title = str_of(e, "title").unwrap_or_default();
            if title == "[Private video]" || title == "[Deleted video]" {
                continue;
            }
            let Some(url) = entry_url(e) else { continue };
            if existing.contains(&url) {
                continue;
            }
            let mut it = Item {
                id: new_id(),
                url,
                group_id: Some(group_id.clone()),
                group_title: Some(group_title.clone()),
                group_index: Some(
                    e.get("playlist_index").and_then(Value::as_u64).map(|v| v as u32).unwrap_or(n as u32 + 1),
                ),
                status: "queued".into(),
                opts: template.opts.clone(),
                added_at: now(),
                ..Default::default()
            };
            fill_from_info(&mut it, e);
            new_items.push(it);
        }
        if new_items.is_empty() {
            let it = &mut inner.items[pos];
            it.status = "error".into();
            it.stage = String::new();
            it.title = group_title;
            it.error = Some("The playlist is empty or has no available videos.".into());
        } else {
            inner.items.splice(pos..=pos, new_items);
        }
        inner.dirty = true;
        self.emit_items(&inner);
    }

    // --------------------------------------------------------------- actions

    pub fn item_action(self: &Arc<Self>, id: &str, action: &str) {
        let mut inner = self.inner.lock().unwrap();
        let running = inner.running.contains_key(id);
        let stop = match action {
            "pause" => Some(Stop::Pause),
            "cancel" => Some(Stop::Cancel),
            "remove" => Some(Stop::Remove),
            _ => None,
        };
        if let (true, Some(stop)) = (running, stop) {
            let r = inner.running.get_mut(id).unwrap();
            r.stop = Some(stop);
            interrupt(r.pid);
            if stop == Stop::Remove {
                inner.items.retain(|i| i.id != id);
                inner.logs.remove(id);
                inner.dirty = true;
                self.emit_items(&inner);
            }
            return;
        }
        match action {
            "pause" => set_status(&mut inner, id, &["queued"], "paused"),
            "cancel" => set_status(&mut inner, id, &["queued", "paused"], "canceled"),
            "resume" => set_status(&mut inner, id, &["paused"], "queued"),
            "retry" => {
                if let Some(it) = inner.items.iter_mut().find(|i| i.id == id) {
                    if ["error", "canceled", "done"].contains(&it.status.as_str()) {
                        let refetch = it.duration.is_none() && it.title == it.url;
                        it.status = if refetch { "fetching".into() } else { "queued".into() };
                        it.error = None;
                        it.stage = String::new();
                        it.percent = 0.0;
                        it.downloaded = 0;
                        it.filepath = None;
                        it.attempts = 0;
                        it.retry_at = 0;
                        if refetch {
                            // Retry a failed metadata fetch.
                            inner.dirty = true;
                            self.emit_items(&inner);
                            drop(inner);
                            self.refetch_arc(id.to_string());
                            return;
                        }
                    }
                }
            }
            "remove" => {
                inner.items.retain(|i| i.id != id);
                inner.logs.remove(id);
            }
            _ => {}
        }
        inner.dirty = true;
        self.emit_items(&inner);
        drop(inner);
        self.wake.notify_one();
    }

    pub fn refetch_arc(self: &Arc<Self>, id: String) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.fetch_info(id).await });
    }

    pub fn set_item_options(&self, id: &str, opts: ItemOptions) {
        let mut inner = self.inner.lock().unwrap();
        if inner.running.contains_key(id) {
            return;
        }
        if let Some(it) = inner.items.iter_mut().find(|i| i.id == id) {
            it.opts = opts;
            let snap = it.clone();
            inner.dirty = true;
            drop(inner);
            self.emit_item(&snap);
        }
    }

    /// Apply new options to every item that has not started yet.
    pub fn set_pending_options(&self, opts: ItemOptions) {
        let mut inner = self.inner.lock().unwrap();
        for it in inner.items.iter_mut() {
            if ["queued", "paused", "fetching"].contains(&it.status.as_str()) {
                let keep = (it.opts.format_id.clone(), it.opts.clip_start.clone(), it.opts.clip_end.clone());
                it.opts = opts.clone();
                (it.opts.format_id, it.opts.clip_start, it.opts.clip_end) = keep;
            }
        }
        inner.dirty = true;
        self.emit_items(&inner);
    }

    pub fn reorder(&self, ids: Vec<String>) {
        let mut inner = self.inner.lock().unwrap();
        let mut map: HashMap<String, Item> = inner.items.drain(..).map(|i| (i.id.clone(), i)).collect();
        let mut out: Vec<Item> = ids.iter().filter_map(|id| map.remove(id)).collect();
        // Keep anything the frontend did not mention, in its original order.
        let mut rest: Vec<Item> = map.into_values().collect();
        rest.sort_by_key(|i| i.added_at);
        out.extend(rest);
        inner.items = out;
        inner.dirty = true;
        self.emit_items(&inner);
    }

    pub fn clear_finished(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.items.retain(|i| !["done", "canceled"].contains(&i.status.as_str()));
        inner.dirty = true;
        self.emit_items(&inner);
    }

    pub fn pause_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        for r in inner.running.values_mut() {
            r.stop = Some(Stop::Pause);
            interrupt(r.pid);
        }
        for it in inner.items.iter_mut() {
            if it.status == "queued" {
                it.status = "paused".into();
            }
        }
        inner.dirty = true;
        self.emit_items(&inner);
    }

    pub fn resume_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        for it in inner.items.iter_mut() {
            if it.status == "paused" {
                it.status = "queued".into();
            }
        }
        inner.dirty = true;
        self.emit_items(&inner);
        drop(inner);
        self.wake.notify_one();
    }

    pub fn item_path(&self, id: &str) -> Option<String> {
        self.inner.lock().unwrap().items.iter().find(|i| i.id == id).and_then(|i| i.filepath.clone())
    }

    pub fn command_line(&self, id: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        let it = inner.items.iter().find(|i| i.id == id)?;
        let bin = self.tools.ytdlp_path().unwrap_or_else(|| PathBuf::from("yt-dlp"));
        let mut parts = vec![bin.to_string_lossy().into_owned()];
        parts.extend(self.build_args(&inner.settings, it, false));
        Some(parts.iter().map(|p| shell_quote(p)).collect::<Vec<_>>().join(" "))
    }

    pub async fn list_formats(&self, url: &str) -> Result<Value, String> {
        let ytdlp = self.tools.ytdlp_path().ok_or("yt-dlp is not ready yet")?;
        let s = self.settings();
        let info = self.run_json(&ytdlp, &s, url, false).await.map_err(|e| friendly_error(&e))?;
        let formats: Vec<Value> = info
            .get("formats")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|f| str_of(f, "format_note").as_deref() != Some("storyboard") && str_of(f, "ext").as_deref() != Some("mhtml"))
            .map(|f| {
                serde_json::json!({
                    "id": f.get("format_id"),
                    "ext": f.get("ext"),
                    "height": f.get("height"),
                    "width": f.get("width"),
                    "fps": f.get("fps"),
                    "vcodec": f.get("vcodec"),
                    "acodec": f.get("acodec"),
                    "abr": f.get("abr"),
                    "tbr": f.get("tbr"),
                    "size": f.get("filesize").filter(|v| !v.is_null()).or(f.get("filesize_approx")),
                    "note": f.get("format_note"),
                    "hdr": f.get("dynamic_range"),
                    "lang": f.get("language"),
                })
            })
            .collect();
        Ok(serde_json::json!({ "title": info.get("title"), "duration": info.get("duration"), "formats": formats }))
    }

    // ------------------------------------------------------------- scheduler

    pub async fn run_scheduler(self: Arc<Self>) {
        let mut last_save = Instant::now();
        loop {
            let _ = tokio::time::timeout(Duration::from_millis(1000), self.wake.notified()).await;
            self.start_ready();
            if last_save.elapsed() > Duration::from_secs(2) {
                self.persist(false);
                last_save = Instant::now();
            }
        }
    }

    pub fn persist(&self, force: bool) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.dirty && !force {
            return;
        }
        inner.dirty = false;
        let mut items = inner.items.clone();
        drop(inner);
        for it in items.iter_mut() {
            if ACTIVE.contains(&it.status.as_str()) {
                it.status = "queued".into();
            }
        }
        let path = self.data_dir.join("queue.json");
        let tmp = path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_string(&items) {
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
        }
    }

    fn start_ready(self: &Arc<Self>) {
        let Some(ytdlp) = self.tools.ytdlp_path() else { return };
        let mut inner = self.inner.lock().unwrap();
        let limit = inner.settings.parallel.clamp(1, 8);
        let mut to_start = vec![];
        while inner.running.len() + to_start.len() < limit {
            let Some(it) = inner
                .items
                .iter_mut()
                .find(|i| i.status == "queued" && i.retry_at <= now() && !to_start.contains(&i.id))
            else {
                break;
            };
            it.status = "downloading".into();
            it.stage = "Starting…".into();
            it.error = None;
            to_start.push(it.id.clone());
        }
        if to_start.is_empty() {
            return;
        }
        inner.dirty = true;
        self.emit_items(&inner);
        // Reserve slots so the next pass doesn't overshoot before the processes spawn.
        for id in &to_start {
            inner.running.insert(id.clone(), Running { pid: 0, stop: None });
        }
        drop(inner);
        for id in to_start {
            let me = self.clone();
            let ytdlp = ytdlp.clone();
            tauri::async_runtime::spawn(async move { me.run_download(id, ytdlp).await });
        }
    }

    async fn run_download(self: Arc<Self>, id: String, ytdlp: PathBuf) {
        let (args, mode) = {
            let inner = self.inner.lock().unwrap();
            let Some(it) = inner.items.iter().find(|i| i.id == id) else {
                drop(inner);
                self.inner.lock().unwrap().running.remove(&id);
                return;
            };
            (self.build_args(&inner.settings, it, true), it.opts.mode.clone())
        };
        if let Some(dir) = self.output_dir_for(&id) {
            let _ = std::fs::create_dir_all(dir);
        }
        self.log(&id, &format!("$ yt-dlp {}", args.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ")));

        let mut cmd = Command::new(&ytdlp);
        cmd.args(&args)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.inner.lock().unwrap().running.remove(&id);
                self.with_item(&id, |it| {
                    it.status = "error".into();
                    it.error = Some(format!("Could not start yt-dlp: {e}"));
                });
                self.wake.notify_one();
                return;
            }
        };
        let pid = child.id().map(|p| p as i32).unwrap_or(0);
        let stop_now = {
            let mut inner = self.inner.lock().unwrap();
            match inner.running.get_mut(&id) {
                Some(r) => {
                    r.pid = pid;
                    r.stop
                }
                None => Some(Stop::Remove),
            }
        };
        if stop_now.is_some() {
            interrupt(pid);
        }

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let me = self.clone();
        let id2 = id.clone();
        let err_task = tauri::async_runtime::spawn(async move {
            let mut last_err = None;
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                me.log(&id2, &l);
                if l.starts_with("ERROR:") {
                    last_err = Some(l);
                }
            }
            last_err
        });

        let mut p = Progress::new(mode == "audio");
        let mut lines = BufReader::new(stdout).lines();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    if !line.starts_with("YGP ") {
                        self.log(&id, &line);
                    }
                    let changed_stage = p.feed(&line);
                    if changed_stage || last_emit.elapsed() > Duration::from_millis(250) {
                        last_emit = Instant::now();
                        self.with_item(&id, |it| p.apply(it));
                    }
                }
                // Keep the bar moving while ffmpeg works (it reports no progress).
                _ = tick.tick() => {
                    if p.processing_since.is_some() && last_emit.elapsed() > Duration::from_millis(400) {
                        last_emit = Instant::now();
                        self.with_item(&id, |it| p.apply(it));
                    }
                }
            }
        }
        let status = child.wait().await;
        let last_err = err_task.await.ok().flatten();

        let stop = self.inner.lock().unwrap().running.remove(&id).and_then(|r| r.stop);
        let ok = status.map(|s| s.success()).unwrap_or(false);
        let (notify_title, notify_body) = match stop {
            Some(Stop::Remove) => (None, None),
            Some(Stop::Shutdown) => (None, None),
            Some(Stop::Pause) => {
                self.with_item(&id, |it| {
                    it.status = "paused".into();
                    it.stage = "Paused".into();
                    it.speed = None;
                    it.eta = None;
                });
                (None, None)
            }
            Some(Stop::Cancel) => {
                if let Some(tmp) = &p.tmpfile {
                    let dir = self.output_dir_for(&id);
                    cleanup_partial(dir.as_deref(), tmp);
                }
                self.with_item(&id, |it| {
                    it.status = "canceled".into();
                    it.stage = String::new();
                    it.speed = None;
                    it.eta = None;
                });
                (None, None)
            }
            None if ok => {
                self.with_item(&id, |it| {
                    it.status = "done".into();
                    it.attempts = 0;
                    it.retry_at = 0;
                    it.stage = if p.skipped.is_empty() { String::new() } else { p.skipped.clone() };
                    it.percent = 100.0;
                    it.speed = None;
                    it.eta = None;
                    if it.total.is_none() {
                        it.total = Some(it.downloaded);
                    }
                    if p.filepath.is_some() {
                        it.filepath = p.filepath.clone();
                    }
                });
                self.inner.lock().unwrap().completed_since_idle += 1;
                (None, None)
            }
            None if is_transient(last_err.as_deref().unwrap_or(""))
                && self.with_item(&id, |it| (it.attempts as usize) < RETRY_DELAYS.len()).unwrap_or(false) =>
            {
                let reason = short_reason(last_err.as_deref().unwrap_or(""));
                self.with_item(&id, |it| {
                    let delay = RETRY_DELAYS[it.attempts as usize];
                    it.attempts += 1;
                    it.retry_at = now() + delay;
                    it.status = "queued".into();
                    it.stage = reason.clone();
                    it.percent = 0.0;
                    it.downloaded = 0;
                    it.speed = None;
                    it.eta = None;
                });
                self.log(&id, &format!("YouGet: {reason}; retrying automatically"));
                (None, None)
            }
            None => {
                let msg = last_err.unwrap_or_else(|| "yt-dlp exited with an error".into());
                let mut friendly = friendly_error(&msg);
                if is_transient(&msg) {
                    friendly = format!("{friendly} (gave up after {} automatic retries)", RETRY_DELAYS.len());
                }
                let title = self.with_item(&id, |it| {
                    it.status = "error".into();
                    it.stage = String::new();
                    it.speed = None;
                    it.eta = None;
                    it.error = Some(friendly.clone());
                    it.title.clone()
                });
                (Some("Download failed".to_string()), Some(format!("{}: {}", title.unwrap_or_default(), friendly)))
            }
        };
        if let (Some(t), Some(b)) = (notify_title, notify_body) {
            self.notify(&t, &b);
        }
        self.check_idle();
        self.wake.notify_one();
    }

    fn check_idle(&self) {
        let mut inner = self.inner.lock().unwrap();
        let busy = !inner.running.is_empty()
            || inner.items.iter().any(|i| ["queued", "fetching", "downloading", "processing"].contains(&i.status.as_str()));
        if busy || inner.completed_since_idle == 0 {
            return;
        }
        let n = inner.completed_since_idle;
        inner.completed_since_idle = 0;
        drop(inner);
        let body = if n == 1 { "1 download finished.".to_string() } else { format!("{n} downloads finished.") };
        self.notify("All downloads complete", &body);
    }

    fn notify(&self, title: &str, body: &str) {
        if !self.inner.lock().unwrap().settings.notifications {
            return;
        }
        let _ = self.app.notification().builder().title(title).body(body).show();
    }

    pub fn shutdown(&self) {
        let mut inner = self.inner.lock().unwrap();
        for r in inner.running.values_mut() {
            r.stop = Some(Stop::Shutdown);
            interrupt(r.pid);
        }
        drop(inner);
        self.persist(true);
    }

    // ------------------------------------------------------------ arguments

    fn output_dir_for(&self, id: &str) -> Option<PathBuf> {
        let inner = self.inner.lock().unwrap();
        let it = inner.items.iter().find(|i| i.id == id)?;
        Some(output_dir(&inner.settings, it))
    }

    fn build_args(&self, s: &Settings, it: &Item, for_run: bool) -> Vec<String> {
        let o = &it.opts;
        let mut a: Vec<String> = vec![];
        let push = |a: &mut Vec<String>, xs: &[&str]| a.extend(xs.iter().map(|x| x.to_string()));

        if for_run {
            push(&mut a, &["--newline", "--progress", "--color", "never", "--no-simulate"]);
            push(&mut a, &["--progress-template", PROGRESS_TEMPLATE]);
            push(&mut a, &["--print", "before_dl:YGI %(filesize,filesize_approx|NA)s"]);
            push(&mut a, &["--print", "after_move:YGF %(filepath)s"]);
            // --print implies --quiet; we still want stage messages.
            push(&mut a, &["--no-quiet"]);
        }
        push(&mut a, &["--no-playlist"]);

        // Output location
        a.push("-P".into());
        a.push(output_dir(s, it).to_string_lossy().into_owned());
        let mut tmpl = if s.filename_template.trim().is_empty() {
            "%(title)s.%(ext)s".to_string()
        } else {
            s.filename_template.trim().to_string()
        };
        if !tmpl.contains("%(ext)s") {
            tmpl.push_str(".%(ext)s");
        }
        if let (true, Some(idx)) = (s.playlist_numbering, it.group_index.filter(|_| it.group_id.is_some())) {
            tmpl = format!("{idx:03} - {tmpl}");
        }
        a.push("-o".into());
        a.push(tmpl);
        if s.restrict_filenames {
            a.push("--restrict-filenames".into());
        }
        if s.overwrite == "overwrite" {
            a.push("--force-overwrites".into());
        }

        // Format selection
        let audio = o.mode == "audio";
        if let Some(fid) = o.format_id.as_ref().filter(|f| !f.trim().is_empty()) {
            push(&mut a, &["-f", fid.trim()]);
            if !audio && fid.contains('+') {
                push(&mut a, &["--merge-output-format", &o.container]);
            }
        } else if audio {
            push(&mut a, &["-f", "ba/b"]);
        } else if o.container == "webm" {
            push(&mut a, &["-f", "bv*[ext=webm]+ba[ext=webm]/b[ext=webm]/bv*+ba/b"]);
        } else {
            push(&mut a, &["-f", "bv*+ba/b"]);
        }
        if !audio && o.format_id.as_deref().map(str::trim).unwrap_or("").is_empty() {
            let mut sort = vec![];
            if o.video_quality != "best" {
                sort.push(format!("res:{}", o.video_quality));
            }
            let vcodec = match (s.codec_pref.as_str(), o.container.as_str()) {
                ("h264", _) => Some("vcodec:h264"),
                ("vp9", _) => Some("vcodec:vp9"),
                ("av1", _) => Some("vcodec:av01"),
                (_, "mp4") => Some("vcodec:h264"),
                (_, "webm") => Some("vcodec:vp9"),
                _ => None,
            };
            if let Some(v) = vcodec {
                sort.push(v.into());
            }
            match o.container.as_str() {
                "mp4" => sort.push("acodec:aac".into()),
                "webm" => sort.push("acodec:opus".into()),
                _ => {}
            }
            if !sort.is_empty() {
                a.push("-S".into());
                a.push(sort.join(","));
            }
            push(&mut a, &["--merge-output-format", &o.container]);
            if o.container != "webm" {
                push(&mut a, &["--remux-video", &o.container]);
            }
        }
        if audio {
            push(&mut a, &["-x", "--audio-format", &o.audio_format]);
            let lossless = o.audio_format == "flac" || o.audio_format == "wav";
            if !lossless {
                let q = match o.audio_quality.as_str() {
                    "best" | "" => "0".to_string(),
                    q => format!("{q}K"),
                };
                push(&mut a, &["--audio-quality", &q]);
            }
        }

        // Clip
        let start = o.clip_start.as_deref().map(str::trim).unwrap_or("");
        let end = o.clip_end.as_deref().map(str::trim).unwrap_or("");
        if !start.is_empty() || !end.is_empty() {
            let start = if start.is_empty() { "0" } else { start };
            let end = if end.is_empty() { "inf" } else { end };
            push(&mut a, &["--download-sections", &format!("*{start}-{end}")]);
            if s.force_keyframes {
                a.push("--force-keyframes-at-cuts".into());
            }
        }

        // Embedding
        let thumb_ok = if audio { o.audio_format != "wav" } else { o.container != "webm" };
        if s.embed_thumbnail && thumb_ok {
            a.push("--embed-thumbnail".into());
        }
        if s.embed_metadata {
            a.push("--embed-metadata".into());
        }
        if s.embed_chapters {
            a.push("--embed-chapters".into());
        }
        if s.split_chapters {
            a.push("--split-chapters".into());
        }

        // Subtitles (video only)
        if s.subtitles && !audio {
            push(&mut a, &["--write-subs", "--sub-langs", if s.sub_langs.trim().is_empty() { "en" } else { s.sub_langs.trim() }]);
            if s.auto_subs {
                a.push("--write-auto-subs".into());
            }
            push(&mut a, &["--convert-subs", &s.sub_format]);
            if s.embed_subs {
                a.push("--embed-subs".into());
            }
        }

        // SponsorBlock
        if s.sponsorblock != "off" && !s.sponsorblock_categories.is_empty() {
            let flag = if s.sponsorblock == "remove" { "--sponsorblock-remove" } else { "--sponsorblock-mark" };
            push(&mut a, &[flag, &s.sponsorblock_categories.join(",")]);
        }

        // Network
        if !s.rate_limit.trim().is_empty() {
            push(&mut a, &["-r", s.rate_limit.trim()]);
        }
        let n = s.concurrent_fragments.clamp(1, 32).to_string();
        let r = s.retries.to_string();
        push(&mut a, &["-N", &n, "--retries", &r, "--fragment-retries", &r]);

        if s.use_archive {
            a.push("--download-archive".into());
            a.push(self.data_dir.join("archive.txt").to_string_lossy().into_owned());
        }

        a.extend(self.access_args(s));
        if let Some(ff) = self.tools.ffmpeg_dir() {
            a.push("--ffmpeg-location".into());
            a.push(ff.to_string_lossy().into_owned());
        }
        a.extend(split_args(&s.extra_args));
        a.push("--".into());
        a.push(it.url.clone());
        a
    }
}

const PROGRESS_TEMPLATE: &str = "download:YGP %(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(progress.tmpfilename)s";

fn output_dir(s: &Settings, it: &Item) -> PathBuf {
    let base = PathBuf::from(&s.output_dir);
    match (&it.group_title, s.playlist_subfolder) {
        (Some(g), true) => base.join(sanitize_dir(g)),
        _ => base,
    }
}

fn set_status(inner: &mut Inner, id: &str, from: &[&str], to: &str) {
    if let Some(it) = inner.items.iter_mut().find(|i| i.id == id) {
        if from.contains(&it.status.as_str()) {
            it.status = to.into();
            it.stage = String::new();
        }
    }
}

/// SIGINT the whole process group so ffmpeg children stop too; yt-dlp keeps
/// its .part files, which makes pause/resume work.
fn interrupt(pid: i32) {
    if pid > 0 {
        unsafe {
            libc::kill(-pid, libc::SIGINT);
        }
    }
}

fn cleanup_partial(dir: Option<&Path>, tmp: &str) {
    let p = Path::new(tmp);
    let p = if p.is_absolute() { p.to_path_buf() } else { dir.map(|d| d.join(p)).unwrap_or_else(|| p.to_path_buf()) };
    // Give yt-dlp a moment to release the file.
    std::thread::sleep(Duration::from_millis(200));
    let _ = std::fs::remove_file(&p);
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        let _ = std::fs::remove_file(p.with_file_name(format!("{name}.ytdl")));
        if let Some(stem) = name.strip_suffix(".part") {
            let _ = std::fs::remove_file(p.with_file_name(format!("{stem}.ytdl")));
        }
    }
}

fn str_of(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn entry_url(e: &Value) -> Option<String> {
    let ie = str_of(e, "ie_key").unwrap_or_default();
    if let Some(id) = str_of(e, "id").filter(|_| ie == "Youtube") {
        return Some(format!("https://www.youtube.com/watch?v={id}"));
    }
    str_of(e, "webpage_url").or_else(|| str_of(e, "url")).or_else(|| str_of(e, "original_url"))
}

fn fill_from_info(it: &mut Item, v: &Value) {
    if let Some(t) = str_of(v, "title") {
        it.title = t;
    }
    it.uploader = str_of(v, "uploader").or_else(|| str_of(v, "channel")).or(it.uploader.take());
    it.duration = v.get("duration").and_then(Value::as_f64).or(it.duration);
    let ie = str_of(v, "ie_key").or_else(|| str_of(v, "extractor_key")).unwrap_or_default();
    let thumb = if ie == "Youtube" {
        str_of(v, "id").map(|id| format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"))
    } else {
        None
    };
    it.thumbnail = thumb
        .or_else(|| str_of(v, "thumbnail"))
        .or_else(|| {
            v.get("thumbnails")
                .and_then(Value::as_array)
                .and_then(|a| a.iter().rev().find_map(|t| str_of(t, "url")))
        })
        .or(it.thumbnail.take());
    if let Some(u) = str_of(v, "webpage_url") {
        if it.group_id.is_none() && !u.is_empty() {
            it.url = u;
        }
    }
}

fn sanitize_dir(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if "/\\:*?\"<>|\0".contains(c) { '_' } else { c })
        .collect();
    let t = cleaned.trim().trim_matches('.').trim();
    let t: String = t.chars().take(120).collect();
    if t.is_empty() {
        "Playlist".into()
    } else {
        t
    }
}

/// Errors that are usually gone on a fresh attempt (new stream URLs, new connection).
fn is_transient(e: &str) -> bool {
    let l = e.to_lowercase();
    ["http error 403", "http error 429", "http error 5", "timed out", "connection reset",
     "connection aborted", "incomplete read", "unable to download video data", "fragment", "eof occurred"]
        .iter()
        .any(|k| l.contains(k))
}

fn short_reason(e: &str) -> String {
    let l = e.to_lowercase();
    if l.contains("403") {
        "YouTube refused the stream (HTTP 403)".into()
    } else if l.contains("429") {
        "Rate limited (HTTP 429)".into()
    } else if l.contains("http error 5") {
        "Server error".into()
    } else {
        "Network error".into()
    }
}

pub fn friendly_error(e: &str) -> String {
    let l = e.to_lowercase();
    let msg = if l.contains("http error 403") {
        "YouTube refused the download (HTTP 403). Try again later, lower parallel downloads, or switch yt-dlp to Nightly in Advanced."
    } else if l.contains("private video") {
        "This video is private."
    } else if l.contains("confirm your age") || l.contains("age-restricted") {
        "Age-restricted. Enable \"Cookies from browser\" in Advanced → Access."
    } else if l.contains("not a bot") || l.contains("sign in to confirm") {
        "YouTube wants a sign-in check. Enable \"Cookies from browser\" in Advanced → Access, or update yt-dlp."
    } else if l.contains("members-only") || l.contains("join this channel") {
        "Members-only video. Enable \"Cookies from browser\" in Advanced → Access."
    } else if l.contains("video unavailable") || l.contains("has been removed") {
        "Video unavailable."
    } else if l.contains("requested format is not available") {
        "That format isn't available for this video. Try another quality or format."
    } else if l.contains("unsupported url") {
        "This link isn't supported."
    } else if l.contains("ffmpeg") && l.contains("not found") {
        "ffmpeg is missing. See Settings → Tools."
    } else if l.contains("http error 429") || l.contains("too many requests") {
        "Rate limited by the site (HTTP 429). Wait a bit or lower parallel downloads."
    } else if l.contains("unable to download webpage") || l.contains("name or service not known") || l.contains("timed out") {
        "Network error. Check your connection or proxy."
    } else {
        return e.trim_start_matches("ERROR:").trim().to_string();
    };
    msg.to_string()
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=,+@%".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Minimal shell-style splitting for the "extra arguments" field.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                has = true;
            }
            (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    has = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ---------------------------------------------------------------- progress

struct Progress {
    audio: bool,
    stream: u32,
    done_bytes: u64,
    cur_dl: u64,
    cur_total: Option<u64>,
    expected: Option<u64>,
    speed: Option<f64>,
    stage: String,
    status: &'static str,
    tmpfile: Option<String>,
    filepath: Option<String>,
    skipped: String,
    processing_since: Option<Instant>,
}

// The bar is split into phases so it always fills from left to right:
// preparing 0–5 %, downloading 5–90 %, post-processing 90–100 %.
const PREP_PCT: f64 = 3.0;
const DL_START: f64 = 5.0;
const DL_END: f64 = 90.0;

impl Progress {
    fn new(audio: bool) -> Self {
        Self {
            audio,
            stream: 0,
            done_bytes: 0,
            cur_dl: 0,
            cur_total: None,
            expected: None,
            speed: None,
            stage: "Starting…".into(),
            status: "downloading",
            tmpfile: None,
            filepath: None,
            skipped: String::new(),
            processing_since: None,
        }
    }

    /// Returns true when the stage changed (worth an immediate UI update).
    fn feed(&mut self, line: &str) -> bool {
        let before = (self.stage.clone(), self.status);
        if let Some(rest) = line.strip_prefix("YGP ") {
            let f: Vec<&str> = rest.split('|').collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
            if f.first() == Some(&"downloading") {
                self.cur_dl = num(1).unwrap_or(0.0) as u64;
                self.cur_total = num(2).or(num(3)).map(|v| v as u64);
                self.speed = num(4);
                if let Some(t) = f.get(6).filter(|t| **t != "NA") {
                    self.tmpfile = Some(t.to_string());
                }
            } else if f.first() == Some(&"finished") {
                self.cur_dl = num(1).unwrap_or(self.cur_dl as f64) as u64;
                self.cur_total = Some(self.cur_dl);
            }
        } else if let Some(rest) = line.strip_prefix("YGI ") {
            self.expected = rest.trim().parse::<f64>().ok().map(|v| v as u64);
        } else if let Some(rest) = line.strip_prefix("YGF ") {
            self.filepath = Some(rest.trim().to_string());
        } else if line.starts_with("[download] Destination:") {
            self.done_bytes += self.cur_total.unwrap_or(self.cur_dl);
            self.cur_dl = 0;
            self.cur_total = None;
            self.stream += 1;
            self.status = "downloading";
            self.stage = if self.audio {
                "Downloading audio".into()
            } else if self.stream == 1 {
                "Downloading video".into()
            } else {
                "Downloading audio".into()
            };
        } else if line.starts_with("[download] Sleeping") {
            self.stage = "Waiting for YouTube…".into();
        } else if self.stream == 0 && (line.starts_with("[youtube") || line.starts_with("[info]")) {
            self.stage = "Fetching video info".into();
        } else if line.contains("has already been recorded in the archive") {
            self.skipped = "Skipped (already in archive)".into();
        } else if line.starts_with("[download]") && line.contains("has already been downloaded") {
            self.skipped = "Already downloaded".into();
        } else if let Some(tag) = line.strip_prefix('[').and_then(|r| r.split(']').next()) {
            let stage = match tag {
                "Merger" => Some("Merging"),
                "ExtractAudio" => Some("Converting audio"),
                "VideoRemuxer" | "VideoConvertor" => Some("Remuxing"),
                "EmbedThumbnail" | "ThumbnailsConvertor" => Some("Embedding thumbnail"),
                "Metadata" => Some("Writing metadata"),
                "EmbedSubtitle" | "SubtitlesConvertor" => Some("Embedding subtitles"),
                "SponsorBlock" | "ModifyChapters" => Some("Applying SponsorBlock"),
                "SplitChapters" => Some("Splitting chapters"),
                t if t.starts_with("Fixup") => Some("Fixing up"),
                _ => None,
            };
            if let Some(s) = stage {
                self.status = "processing";
                self.stage = s.into();
                self.processing_since.get_or_insert_with(Instant::now);
            }
        }
        before != (self.stage.clone(), self.status)
    }

    fn apply(&self, it: &mut Item) {
        it.status = self.status.into();
        it.stage = self.stage.clone();
        let downloaded = self.done_bytes + self.cur_dl;
        let running_total = self.done_bytes + self.cur_total.unwrap_or(self.cur_dl);
        let total = match self.expected {
            Some(e) => e.max(running_total),
            None => running_total,
        };
        it.downloaded = downloaded;
        it.total = (total > 0).then_some(total);
        if self.status == "processing" {
            // ffmpeg gives no progress: ease from 90 % towards 99.5 %.
            let secs = self.processing_since.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
            it.percent = DL_END + (99.5 - DL_END) * (1.0 - (-secs / 6.0).exp());
            it.speed = None;
            it.eta = None;
        } else if self.stream == 0 {
            it.percent = PREP_PCT;
            it.speed = None;
            it.eta = None;
        } else {
            let frac = if total > 0 { (downloaded as f64 / total as f64).min(1.0) } else { 0.0 };
            it.percent = DL_START + (DL_END - DL_START) * frac;
            it.speed = self.speed;
            it.eta = match self.speed {
                Some(s) if s > 0.0 && total > downloaded => Some((total - downloaded) as f64 / s),
                _ => None,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_args() {
        assert_eq!(split_args(r#"--a "b c" 'd e' f\ g"#), vec!["--a", "b c", "d e", "f g"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn parses_progress() {
        let mut p = Progress::new(false);
        p.feed("YGI 1000");
        p.feed("[download] Destination: a.f137.mp4");
        p.feed("YGP downloading|300|800|NA|100.0|5|a.f137.mp4.part");
        let mut it = Item::default();
        p.apply(&mut it);
        assert_eq!(it.stage, "Downloading video");
        assert_eq!(it.downloaded, 300);
        assert_eq!(it.total, Some(1000));
        p.feed("YGP finished|800|800|NA|NA|NA|NA");
        p.feed("[download] Destination: a.f140.m4a");
        p.feed("YGP downloading|100|200|NA|100.0|1|a.f140.m4a.part");
        p.apply(&mut it);
        assert_eq!(it.stage, "Downloading audio");
        assert_eq!(it.downloaded, 900);
        assert!(p.feed("[Merger] Merging formats into \"a.mp4\""));
        p.feed("YGF /tmp/a.mp4");
        p.apply(&mut it);
        assert_eq!(it.status, "processing");
        assert!(it.percent >= 90.0 && it.percent < 100.0);
        assert_eq!(p.filepath.as_deref(), Some("/tmp/a.mp4"));
    }

    #[test]
    fn phased_percent() {
        let mut p = Progress::new(true);
        let mut it = Item::default();
        p.feed("[youtube] abc: Downloading webpage");
        p.apply(&mut it);
        assert_eq!(it.stage, "Fetching video info");
        assert_eq!(it.percent, PREP_PCT);
        p.feed("[download] Destination: a.webm");
        p.feed("YGP downloading|50|100|NA|10|5|a.webm.part");
        p.apply(&mut it);
        assert!((it.percent - 47.5).abs() < 0.01);
    }

    #[test]
    fn transient() {
        assert!(is_transient("ERROR: unable to download video data: HTTP Error 403: Forbidden"));
        assert!(!is_transient("ERROR: Private video"));
    }

    #[test]
    fn friendly() {
        assert!(friendly_error("ERROR: [youtube] x: Private video. Sign in").contains("private"));
        assert_eq!(friendly_error("ERROR: something odd"), "something odd");
    }
}

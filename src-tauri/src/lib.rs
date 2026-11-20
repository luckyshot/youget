mod queue;
mod settings;
mod tools;

use queue::{FullState, Manager};
use settings::{ItemOptions, Settings};
use std::path::Path;
use std::sync::Arc;
use tauri::{Manager as _, RunEvent, State};

type Mgr<'a> = State<'a, Arc<Manager>>;

#[tauri::command]
fn get_state(m: Mgr) -> FullState {
    m.full_state()
}

#[tauri::command]
fn add_urls(m: Mgr, urls: Vec<String>, opts: Option<ItemOptions>) {
    m.add_urls(urls, opts);
}

#[tauri::command]
fn save_settings(m: Mgr, settings: Settings) {
    m.save_settings(settings);
}

#[tauri::command]
fn item_action(m: Mgr, id: String, action: String) {
    m.item_action(&id, &action);
}

#[tauri::command]
fn set_item_options(m: Mgr, id: String, opts: ItemOptions) {
    m.set_item_options(&id, opts);
}

#[tauri::command]
fn set_pending_options(m: Mgr, opts: ItemOptions) {
    m.set_pending_options(opts);
}

#[tauri::command]
fn reorder(m: Mgr, ids: Vec<String>) {
    m.reorder(ids);
}

#[tauri::command]
fn clear_finished(m: Mgr) {
    m.clear_finished();
}

#[tauri::command]
fn pause_all(m: Mgr) {
    m.pause_all();
}

#[tauri::command]
fn resume_all(m: Mgr) {
    m.resume_all();
}

#[tauri::command]
fn get_log(m: Mgr, id: String) -> Vec<String> {
    m.get_log(&id)
}

#[tauri::command]
fn command_line(m: Mgr, id: String) -> Option<String> {
    m.command_line(&id)
}

#[tauri::command]
async fn list_formats(m: Mgr<'_>, url: String) -> Result<serde_json::Value, String> {
    m.list_formats(&url).await
}

#[tauri::command]
async fn update_ytdlp(app: tauri::AppHandle, m: Mgr<'_>) -> Result<String, String> {
    if m.is_busy() {
        return Err("Wait for the active downloads to finish (or pause them) before updating.".into());
    }
    let channel = m.settings().channel;
    let r = m.tools.update_ytdlp(&app, &channel).await;
    m.mark_update_checked();
    r
}

#[tauri::command]
async fn recheck_tools(app: tauri::AppHandle, m: Mgr<'_>) -> Result<(), String> {
    let custom = m.settings().custom_ytdlp;
    m.tools.ensure_all(&app, &custom).await;
    Ok(())
}

/// Show a downloaded file in Finder / the file manager.
#[tauri::command]
fn reveal(m: Mgr, id: String) -> Result<(), String> {
    let p = m.item_path(&id).ok_or("File not found")?;
    reveal_path(Path::new(&p))
}

#[tauri::command]
fn open_file(m: Mgr, id: String) -> Result<(), String> {
    let p = m.item_path(&id).ok_or("File not found")?;
    open_path(Path::new(&p))
}

#[tauri::command]
fn open_dir(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let _ = std::fs::create_dir_all(p);
    open_path(p)
}

fn spawn_detached(cmd: &str, args: &[&str]) -> Result<(), String> {
    std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn open_path(p: &Path) -> Result<(), String> {
    let s = p.to_string_lossy();
    if cfg!(target_os = "macos") {
        spawn_detached("open", &[&s])
    } else {
        spawn_detached("xdg-open", &[&s])
    }
}

fn reveal_path(p: &Path) -> Result<(), String> {
    if !p.exists() {
        return Err("The file no longer exists.".into());
    }
    let s = p.to_string_lossy();
    if cfg!(target_os = "macos") {
        return spawn_detached("open", &["-R", &s]);
    }
    // Ask the file manager to select the file (works on Nautilus, Dolphin, Nemo, Thunar…).
    let uri = format!("array:string:file://{}", percent_encode(&s));
    let ok = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &uri,
            "string:",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        open_path(p.parent().unwrap_or(p))
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

const UPDATE_INTERVAL_SECS: u64 = 24 * 3600;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[allow(unused_mut)]
            let mut data_dir = app.path().app_data_dir()?;
            // Development aid: isolated data folder (debug builds only).
            #[cfg(debug_assertions)]
            if let Ok(d) = std::env::var("YOUGET_DATA_DIR") {
                data_dir = std::path::PathBuf::from(d);
            }
            std::fs::create_dir_all(&data_dir)?;
            let manager = Manager::new(app.handle().clone(), data_dir);
            app.manage(manager.clone());

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let s = manager.settings();
                manager.tools.ensure_all(&handle, &s.custom_ytdlp).await;
                // Daily yt-dlp self-update, before any download starts.
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if s.auto_update && s.custom_ytdlp.trim().is_empty() && now.saturating_sub(s.last_update_check) > UPDATE_INTERVAL_SECS {
                    let _ = manager.tools.update_ytdlp(&handle, &s.channel).await;
                    manager.mark_update_checked();
                }
                // Items restored from the last session that never got metadata.
                let pending: Vec<String> = manager
                    .full_state()
                    .items
                    .iter()
                    .filter(|i| i.status == "fetching")
                    .map(|i| i.id.clone())
                    .collect();
                for id in pending {
                    manager.refetch_arc(id);
                }
                // Development aid: enqueue URLs from the environment (debug builds only).
                #[cfg(debug_assertions)]
                if let Ok(urls) = std::env::var("YOUGET_DEBUG_ADD") {
                    manager.add_urls(urls.split_whitespace().map(String::from).collect(), None);
                }
                manager.run_scheduler().await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            add_urls,
            save_settings,
            item_action,
            set_item_options,
            set_pending_options,
            reorder,
            clear_finished,
            pause_all,
            resume_all,
            get_log,
            command_line,
            list_formats,
            update_ytdlp,
            recheck_tools,
            reveal,
            open_file,
            open_dir,
        ])
        .build(tauri::generate_context!())
        .expect("error while building YouGet");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(m) = handle.try_state::<Arc<Manager>>() {
                m.shutdown();
            }
        }
    });
}

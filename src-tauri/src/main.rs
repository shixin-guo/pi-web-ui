#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pi_manager;

use pi_manager::{is_port_in_use, wait_for_endpoint, wait_for_health, PiManager};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use tauri::image::Image;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

type PiManagerState = Arc<PiManager>;

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Create a new session within the current workspace (RPC command to existing pi)
#[tauri::command]
fn cmd_new_session(port: u16, manager: State<PiManagerState>) -> Result<(), String> {
    manager.send_rpc(port, serde_json::json!({ "type": "new_session" }))
}

/// Resume (switch to) an existing session file within the current workspace
#[tauri::command]
fn cmd_switch_session(
    port: u16,
    session_path: String,
    manager: State<PiManagerState>,
) -> Result<(), String> {
    manager.send_rpc(
        port,
        serde_json::json!({ "type": "switch_session", "sessionPath": session_path }),
    )
}

/// Open a workspace directory by spawning a separate pi process.
/// When `open_window` is true (default) a new OS window is opened for the new pi.
/// When false, the pi process is spawned headlessly and the caller is expected to
/// navigate the current window to the returned port.
#[tauri::command]
async fn cmd_open_workspace(
    cwd: String,
    session_path: Option<String>,
    force_new_session: Option<bool>,
    open_window: Option<bool>,
    wait_for_sessions: Option<bool>,
    manager: State<'_, PiManagerState>,
    app: AppHandle,
) -> Result<u16, String> {
    let started_at = Instant::now();
    let port = manager.next_port();
    let spawn_started_at = Instant::now();
    manager.spawn(&cwd, port, session_path.as_deref())?;
    eprintln!(
        "[pi-desktop] open_workspace spawn complete: port={} cwd={} elapsed_ms={}",
        port,
        cwd,
        spawn_started_at.elapsed().as_millis()
    );

    let health_started_at = Instant::now();
    wait_for_health(port, 12).await?;
    eprintln!(
        "[pi-desktop] open_workspace health ready: port={} elapsed_ms={}",
        port,
        health_started_at.elapsed().as_millis()
    );
    if force_new_session.unwrap_or(false) {
        let new_session_started_at = Instant::now();
        manager.send_rpc(port, serde_json::json!({ "type": "new_session" }))?;
        eprintln!(
            "[pi-desktop] open_workspace new_session sent: port={} elapsed_ms={}",
            port,
            new_session_started_at.elapsed().as_millis()
        );
    }
    if wait_for_sessions.unwrap_or(false) {
        let sessions_started_at = Instant::now();
        match wait_for_endpoint(port, "/api/sessions", 4).await {
            Ok(_) => eprintln!(
                "[pi-desktop] open_workspace sessions ready: port={} elapsed_ms={}",
                port,
                sessions_started_at.elapsed().as_millis()
            ),
            Err(err) => eprintln!(
                "[pi-desktop] open_workspace sessions warmup skipped: port={} error={}",
                port, err
            ),
        }
    }
    if open_window.unwrap_or(true) {
        open_workspace_window(&app, port)?;
    }
    eprintln!(
        "[pi-desktop] open_workspace complete: port={} total_elapsed_ms={}",
        port,
        started_at.elapsed().as_millis()
    );
    Ok(port)
}

/// Stop (kill) a pi instance
#[tauri::command]
fn cmd_stop_instance(port: u16, manager: State<PiManagerState>) {
    manager.kill(port);
}

/// Native folder picker dialog
#[tauri::command]
async fn cmd_pick_folder(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let result = path.map(|p| match p {
            tauri_plugin_fs::FilePath::Path(pb) => pb.to_string_lossy().into_owned(),
            tauri_plugin_fs::FilePath::Url(url) => url.to_string(),
        });
        let _ = tx.send(result);
    });
    rx.await.ok().flatten()
}

/// Returns installed pi CLI version string (e.g. "pi 0.24.1")
#[tauri::command]
fn cmd_get_pi_version() -> Result<String, String> {
    let pi_bin = if let Ok(explicit) = std::env::var("PI_BIN") {
        let candidate = explicit.trim().to_string();
        if candidate.is_empty() {
            "pi".to_string()
        } else {
            candidate
        }
    } else {
        let detected = Command::new("/bin/sh")
            .arg("-lc")
            .arg("command -v pi")
            .output();
        match detected {
            Ok(out) if out.status.success() => {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if path.is_empty() {
                    "pi".to_string()
                } else {
                    path
                }
            }
            _ => "pi".to_string(),
        }
    };

    let output = Command::new(&pi_bin)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to run {} --version: {}", pi_bin, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!(
                "{} --version exited with status {}",
                pi_bin, output.status
            ));
        }
        return Err(format!(
            "{} --version exited with status {}: {}",
            pi_bin, output.status, stderr
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let version = if !stdout.is_empty() { stdout } else { stderr };
    if version.is_empty() {
        return Err("pi --version returned empty output".to_string());
    }
    Ok(version)
}

// ─── Window helpers ───────────────────────────────────────────────────────────

fn open_workspace_window(app: &AppHandle, port: u16) -> Result<(), String> {
    let label = format!("workspace-{}", port);
    let url = format!("http://localhost:{}", port);
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .map_err(|e| format!("Failed to load window icon: {}", e))?;

    WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url.parse().unwrap()))
        .title("Pi Studio")
        .inner_size(1300.0, 860.0)
        .min_inner_size(800.0, 600.0)
        .decorations(true)
        .icon(icon)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn find_static_dir(app: &tauri::App) -> PathBuf {
    // In `tauri dev`, the process cwd is often `src-tauri/`, so `./public`
    // points to a non-existent folder. Prefer the workspace root public dir.
    let workspace_public = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("public");
    if workspace_public.join("index.html").exists() {
        return fs::canonicalize(&workspace_public).unwrap_or(workspace_public);
    }

    let dev_path = std::env::current_dir().unwrap_or_default().join("public");
    if dev_path.join("index.html").exists() {
        return fs::canonicalize(&dev_path).unwrap_or(dev_path);
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("public");
        if bundled.join("index.html").exists() {
            return bundled;
        }
    }
    dev_path
}

fn list_session_files(root: &PathBuf) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let Ok(inner_entries) = fs::read_dir(path) else {
                continue;
            };
            for inner in inner_entries.flatten() {
                let session_path = inner.path();
                if session_path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                    files.push(session_path);
                }
            }
        }
    }

    files
}

fn extract_session_cwd(session_path: &PathBuf) -> Option<String> {
    let file = File::open(session_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(200).flatten() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("session") {
            continue;
        }
        let cwd = value.get("cwd").and_then(Value::as_str)?.trim();
        if cwd.is_empty() {
            return None;
        }
        return Some(cwd.to_string());
    }

    None
}

fn find_latest_session_boot_target() -> Option<(String, String)> {
    let sessions_root = dirs::home_dir()?.join(".pi/agent/sessions");
    if !sessions_root.exists() {
        eprintln!(
            "[pi-desktop] startup resume skipped: sessions dir not found at {}",
            sessions_root.display()
        );
        return None;
    }

    let session_files = list_session_files(&sessions_root);
    let latest = session_files
        .into_iter()
        .filter_map(|path| {
            let mtime = fs::metadata(&path).ok()?.modified().ok()?;
            Some((mtime, path))
        })
        .max_by_key(|(mtime, _)| *mtime)?;

    let session_path = latest.1;
    let cwd = extract_session_cwd(&session_path)?;
    Some((cwd, session_path.to_string_lossy().to_string()))
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let static_dir = find_static_dir(app);
            let manager = Arc::new(PiManager::new(static_dir));

            let home_cwd = dirs::home_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let (cwd, session_path) = match find_latest_session_boot_target() {
                Some((resolved_cwd, resolved_session_path)) => {
                    eprintln!(
                        "[pi-desktop] startup resume target selected: cwd={} session={}",
                        resolved_cwd, resolved_session_path
                    );
                    (resolved_cwd, Some(resolved_session_path))
                }
                None => {
                    eprintln!(
                        "[pi-desktop] startup resume fallback: using home directory {}",
                        home_cwd
                    );
                    (home_cwd, None)
                }
            };

            let initial_port = 3001u16;

            // Dev mode: pi may already be running (started by beforeDevCommand)
            if !is_port_in_use(initial_port) {
                manager
                    .spawn(&cwd, initial_port, session_path.as_deref())
                    .expect("Failed to start pi process");
            } else {
                eprintln!(
                    "[pi-desktop] Port {} already in use, attaching to existing pi",
                    initial_port
                );
            }

            app.manage(manager.clone());

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match wait_for_health(initial_port, 30).await {
                    Ok(_) => {
                        if let Err(e) = open_workspace_window(&app_handle, initial_port) {
                            eprintln!("Failed to open window: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Pi failed to start: {}", e),
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let label = window.label();
                if let Some(port_str) = label.strip_prefix("workspace-") {
                    if let Ok(port) = port_str.parse::<u16>() {
                        if let Some(manager) = window.try_state::<PiManagerState>() {
                            manager.kill(port);
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            cmd_new_session,
            cmd_switch_session,
            cmd_open_workspace,
            cmd_stop_instance,
            cmd_pick_folder,
            cmd_get_pi_version,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle: &tauri::AppHandle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(manager) = app_handle.try_state::<PiManagerState>() {
                    manager.kill_all();
                }
            }
        });
}

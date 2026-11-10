// Hide the extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// Metadata returned to the UI after a file is stored.
#[derive(serde::Serialize)]
struct Stored {
    file: String,
    name: String,
    size: u64,
}

/// `<app data>/attachments`, created on first use.
fn attachments_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("attachments");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Resolve a stored file name to its full path, refusing anything that
/// could point outside the attachments folder.
fn stored_path(app: &AppHandle, file: &str) -> Result<PathBuf, String> {
    if file.is_empty() || file.contains('/') || file.contains('\\') || file.contains("..") {
        return Err("invalid attachment name".into());
    }
    Ok(attachments_dir(app)?.join(file))
}

fn unique_prefix() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", n)
}

fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || "._- ".contains(c) { c } else { '_' })
        .collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() { "file".into() } else { s.chars().take(120).collect() }
}

/// Copy a file the user picked or dropped into the attachments folder.
#[tauri::command]
fn import_file(app: AppHandle, src: String) -> Result<Stored, String> {
    let src = PathBuf::from(src);
    if !src.is_file() {
        return Err("not a file".into());
    }
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let file = format!("{}-{}", unique_prefix(), safe_name(&name));
    let size = std::fs::copy(&src, attachments_dir(&app)?.join(&file)).map_err(|e| e.to_string())?;
    Ok(Stored { file, name, size })
}

/// Store raw bytes (e.g. a pasted screenshot). The body is the file content;
/// the `x-ext` header carries the file extension.
#[tauri::command]
fn save_attachment(app: AppHandle, request: Request<'_>) -> Result<Stored, String> {
    let InvokeBody::Raw(data) = request.body() else {
        return Err("expected binary body".into());
    };
    let ext: String = request
        .headers()
        .get("x-ext")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("bin")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    let ext = if ext.is_empty() { "bin".to_string() } else { ext };
    let file = format!("{}-pasted.{}", unique_prefix(), ext);
    std::fs::write(attachments_dir(&app)?.join(&file), data).map_err(|e| e.to_string())?;
    Ok(Stored { name: file.clone(), file, size: data.len() as u64 })
}

/// Return a stored file's bytes (used for image thumbnails).
#[tauri::command]
fn read_attachment(app: AppHandle, file: String) -> Result<Response, String> {
    let bytes = std::fs::read(stored_path(&app, &file)?).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

#[tauri::command]
fn delete_attachment(app: AppHandle, file: String) -> Result<(), String> {
    let p = stored_path(&app, &file)?;
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Open a stored file with the system's default program.
#[tauri::command]
fn open_attachment(app: AppHandle, file: String) -> Result<(), String> {
    let p = stored_path(&app, &file)?;
    if !p.exists() {
        return Err("file not found".into());
    }
    app.opener()
        .open_path(p.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Show a stored file in Explorer / Finder / the file manager.
#[tauri::command]
fn reveal_attachment(app: AppHandle, file: String) -> Result<(), String> {
    let p = stored_path(&app, &file)?;
    app.opener().reveal_item_in_dir(p).map_err(|e| e.to_string())
}

/// Open a web, mail or phone link in the default application.
#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let lower = url.to_lowercase();
    let ok = ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|p| lower.starts_with(p));
    if !ok {
        return Err("unsupported link".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            import_file,
            save_attachment,
            read_attachment,
            delete_attachment,
            open_attachment,
            reveal_attachment,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running Organizer");
}

// Tauri commands — the Rust equivalent of Electron's ipcMain handlers.
//
// Every function here is exposed to the frontend via `invoke("name", args)`.
// They replace the `ipcMain.handle(...)` calls in the original main.js plus
// the native implementations of the four built-in extensions.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::blocking::AdBlockState;
use crate::extensions::{
    self, ExtensionInfo, ExtensionRegistry, ToggleState,
};
use crate::helpers::{user_extension_dir, builtin_extension_dir};

// ---------------------------------------------------------------------------
// Window chrome
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn window_minimize(window: WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn window_maximize(window: WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
pub fn window_close(window: WebviewWindow) {
    let _ = window.close();
}

/// Allow the custom title-bar to drag the frameless window.
#[tauri::command]
pub fn window_start_dragging(window: WebviewWindow) {
    let _ = window.start_dragging();
}

// ---------------------------------------------------------------------------
// Extension management  (mirrors the Electron extensions:* IPC handlers)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn extensions_list(reg: State<'_, ExtensionRegistry>) -> Vec<ExtensionInfo> {
    extensions::list_info(&reg)
}

#[tauri::command]
pub fn extensions_enable(
    app: AppHandle,
    reg: State<'_, ExtensionRegistry>,
    id: String,
) -> Result<bool, String> {
    extensions::set_enabled(&app, &reg, &id, true)?;
    Ok(true)
}

#[tauri::command]
pub fn extensions_disable(
    app: AppHandle,
    reg: State<'_, ExtensionRegistry>,
    id: String,
) -> Result<bool, String> {
    extensions::set_enabled(&app, &reg, &id, false)?;
    Ok(true)
}

#[derive(Serialize)]
pub struct AddResult {
    pub ok: bool,
    pub extension: Option<ExtensionInfo>,
    pub error: Option<String>,
}

/// Open a native folder picker, copy the chosen extension in, load it.
#[tauri::command]
pub async fn extensions_add(
    app: AppHandle,
    reg: State<'_, ExtensionRegistry>,
) -> Result<AddResult, String> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("Select an unpacked extension folder")
        .blocking_pick_folder();

    match picked {
        Some(path) => {
            let src = PathBuf::from(path.to_string());
            match extensions::add_extension(&app, &reg, &src) {
                Ok(info) => Ok(AddResult {
                    ok: true,
                    extension: Some(info),
                    error: None,
                }),
                Err(e) => Ok(AddResult {
                    ok: false,
                    extension: None,
                    error: Some(e),
                }),
            }
        }
        None => Ok(AddResult {
            ok: false,
            extension: None,
            error: Some("No folder selected".into()),
        }),
    }
}

#[tauri::command]
pub fn extensions_remove(
    app: AppHandle,
    reg: State<'_, ExtensionRegistry>,
    id: String,
) -> Result<bool, String> {
    extensions::remove_extension(&app, &reg, &id)?;
    Ok(true)
}

/// Open the user extensions folder in the OS file manager.
#[tauri::command]
pub fn extensions_open_folder(app: AppHandle) -> Result<bool, String> {
    let dir = user_extension_dir(&app)?;
    crate::helpers::ensure_dir(&dir)?;
    open_in_file_manager(&dir);
    Ok(true)
}

#[tauri::command]
pub fn extensions_reload(
    app: AppHandle,
    reg: State<'_, ExtensionRegistry>,
) -> Result<usize, String> {
    let added = extensions::reload_user_extensions(&app, &reg)?;
    Ok(added)
}

/// Open a path in the platform's default file manager.
fn open_in_file_manager(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Try common Linux file managers.
        for fm in ["xdg-open", "nautilus", "thunar", "dolphin"] {
            if std::process::Command::new(fm)
                .arg(path)
                .spawn()
                .is_ok()
            {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Native built-in extension: Ad Blocker
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn adblock_toggle(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let state: State<AdBlockState> = app.state();
    *state.enabled.lock().unwrap() = enabled;
    // Persist via the toggle state helper.
    let ts: State<ToggleState> = app.state();
    *ts.adblock.lock().unwrap() = enabled;
    extensions::save_toggles(&app, enabled, *ts.darkreader.lock().unwrap());
    Ok(enabled)
}

#[tauri::command]
pub fn adblock_status(app: AppHandle) -> bool {
    let state: State<AdBlockState> = app.state();
    let enabled = *state.enabled.lock().unwrap();
    enabled
}

/// Check whether a URL should be blocked.  Called by the frontend's
/// injected resource-interception script.
#[tauri::command]
pub fn adblock_check(app: AppHandle, url: String) -> bool {
    crate::blocking::check(&app, &url)
}

// ---------------------------------------------------------------------------
// Native built-in extension: Dark Reader
// ---------------------------------------------------------------------------

/// The exact CSS the original dark-reader content script injected.
/// Kept in Rust so the frontend can request it on demand and we have a
/// single source of truth.
const DARK_READER_CSS: &str = r#"
html {
  filter: invert(0.92) hue-rotate(180deg) brightness(105%) contrast(90%) !important;
  background: #fff !important;
}
img, picture, video, iframe, canvas, svg, embed, object {
  filter: invert(1) hue-rotate(180deg) !important;
}
* {
  text-shadow: none !important;
  box-shadow: none !important;
}
html[data-surf-fed-skip] {
  filter: none !important;
}
"#;

#[tauri::command]
pub fn darkreader_toggle(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let ts: State<ToggleState> = app.state();
    *ts.darkreader.lock().unwrap() = enabled;
    extensions::save_toggles(&app, *ts.adblock.lock().unwrap(), enabled);
    Ok(enabled)
}

#[tauri::command]
pub fn darkreader_status(app: AppHandle) -> bool {
    let ts: State<ToggleState> = app.state();
    let enabled = *ts.darkreader.lock().unwrap();
    enabled
}

/// Return the dark-reader CSS so the frontend can inject it into iframes.
#[tauri::command]
pub fn darkreader_get_css() -> String {
    DARK_READER_CSS.to_string()
}

// ---------------------------------------------------------------------------
// Native built-in extension: Page Info
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct PageInfo {
    pub title: String,
    pub url: String,
    pub description: String,
    pub link_count: usize,
    pub image_count: usize,
}

/// The frontend calls this after injecting a probe script into the active
/// iframe; the probe returns the raw counts + meta and we re-package it.
/// We accept a pre-serialised probe result for flexibility.
#[tauri::command]
pub fn page_info_get(
    title: String,
    url: String,
    description: String,
    link_count: usize,
    image_count: usize,
) -> PageInfo {
    PageInfo {
        title,
        url,
        description,
        link_count,
        image_count,
    }
}

// ---------------------------------------------------------------------------
// Native built-in extension: FED-GRAM (Instagram image downloader)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct InstagramImage {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FedGramResult {
    pub ok: bool,
    pub images: Vec<InstagramImage>,
    pub error: Option<String>,
}

/// Fetch a public Instagram post page and extract image URLs from the
/// embedded JSON / meta tags.  Done in Rust to avoid CORS restrictions
/// that a browser-JS fetch would hit.
#[tauri::command]
pub async fn fedgram_extract(post_url: String) -> Result<FedGramResult, String> {
    use scraper::{Html, Selector};

    let url = post_url.trim().to_string();
    if url.is_empty() {
        return Ok(FedGramResult {
            ok: false,
            images: vec![],
            error: Some("No URL provided".into()),
        });
    }

    // Normalise: accept either www.instagram.com or instagram.com /p/ links.
    if !url.contains("instagram.com") {
        return Ok(FedGramResult {
            ok: false,
            images: vec![],
            error: Some("URL does not look like an Instagram post".into()),
        });
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    // Run the blocking request on a separate thread so we don't block the
    // async Tauri runtime.
    let url_clone = url.clone();
    let resp = tokio::task::spawn_blocking(move || {
        client.get(&url_clone).send()
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?
    .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        return Ok(FedGramResult {
            ok: false,
            images: vec![],
            error: Some(format!("Instagram returned HTTP {}", resp.status())),
        });
    }

    let html_text = tokio::task::spawn_blocking(move || resp.text())
        .await
        .map_err(|e| format!("Body read error: {e}"))?
        .map_err(|e| format!("Body decode error: {e}"))?;

    let document = Html::parse_document(&html_text);

    let mut images: Vec<InstagramImage> = Vec::new();

    // 1. Try og:image meta tags (works for single-image posts).
    if let Ok(sel) = Selector::parse(r#"meta[property="og:image"]"#) {
        for el in document.select(&sel) {
            if let Some(content) = el.value().attr("content") {
                if !content.is_empty() && !images.iter().any(|i| i.url == content) {
                    images.push(InstagramImage {
                        url: content.to_string(),
                        width: None,
                        height: None,
                    });
                }
            }
        }
    }

    // 2. Try to find <img> tags pointing at CDN image hosts (carousels).
    if let Ok(sel) = Selector::parse("img[src]") {
        for el in document.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                if (src.contains("cdninstagram.com") || src.contains("fbcdn.net"))
                    && !images.iter().any(|i| i.url == src)
                {
                    images.push(InstagramImage {
                        url: src.to_string(),
                        width: el.value().attr("width").and_then(|w| w.parse().ok()),
                        height: el.value().attr("height").and_then(|h| h.parse().ok()),
                    });
                }
            }
        }
    }

    if images.is_empty() {
        Ok(FedGramResult {
            ok: false,
            images: vec![],
            error: Some("No images found. The post may be private or login-walled.".into()),
        })
    } else {
        // De-duplicate by URL just in case.
        images.dedup_by(|a, b| a.url == b.url);
        Ok(FedGramResult {
            ok: true,
            images,
            error: None,
        })
    }
}

/// Download an image to the user's chosen location.
#[tauri::command]
pub async fn fedgram_download(
    app: AppHandle,
    image_url: String,
    suggested_name: String,
) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;

    // Pick a save path.
    let default_name = if suggested_name.is_empty() {
        "instagram_image.jpg".to_string()
    } else {
        suggested_name
    };

    let picked = app
        .dialog()
        .file()
        .set_title("Save image")
        .set_file_name(&default_name)
        .add_filter("Image", &["jpg", "png", "webp", "jpeg"])
        .blocking_save_file();

    let save_path = match picked {
        Some(p) => PathBuf::from(p.to_string()),
        None => return Ok(false), // user cancelled
    };

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let url_clone = image_url.clone();
    let save_path_clone = save_path.clone();
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let resp = client
            .get(&url_clone)
            .send()
            .map_err(|e| format!("Download failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Server returned HTTP {}", resp.status()));
        }
        let bytes = resp
            .bytes()
            .map_err(|e| format!("Read failed: {e}"))?;
        fs::write(&save_path_clone, &bytes).map_err(|e| format!("Write failed: {e}"))?;
        Ok(bytes.to_vec())
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?;

    match bytes {
        Ok(_) => Ok(true),
        Err(e) => Err(e),
    }
}

// Silence unused-import warnings for things used only on some platforms.
#[allow(dead_code)]
fn _unused() {
    let _ = builtin_extension_dir;
}

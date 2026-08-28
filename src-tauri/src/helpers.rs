// Shared file-system and path helpers.
//
// These replace the Node.js `fs` / `path` calls that lived in `main.js`.
// Tauri gives us `app.path()` for resolving platform-specific data dirs,
// which replaces Electron's `app.getPath('userData')`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{App, AppHandle, Manager};

/// The per-user directory where personal (user-loaded) extensions live.
/// Survives app updates.  Equivalent to Electron's `userData/extensions`.
pub fn user_extension_dir(app: &impl Manager<tauri::Wry>) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    Ok(data.join("extensions"))
}

/// Directory that stores extension enabled/disabled state + ad-blocker toggle.
pub fn extension_state_dir(app: &impl Manager<tauri::Wry>) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    Ok(data.join("extension-state"))
}

/// Create a directory if it does not already exist (recursive).
pub fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Failed to create {}: {e}", path.display()))
}

/// Recursively copy a directory tree (skips symlinks for safety).
/// Mirrors the `copyDirSync` helper from the original Electron main.js.
pub fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ft.is_symlink() {
            // Skip symlinks for safety (matches original behaviour).
        } else if ft.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Read a manifest.json from an extension folder and parse it.
pub fn read_manifest(ext_path: &Path) -> Option<serde_json::Value> {
    let manifest_path = ext_path.join("manifest.json");
    let data = fs::read_to_string(&manifest_path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Read a JSON file, returning a default on any error.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Write a JSON value to a file, pretty-printed.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let s = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialise JSON: {e}"))?;
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, s).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Resolve the built-in extensions directory bundled with the app.
///
/// In a packaged Tauri build, `resource_dir()` points at the folder where
/// `bundle.resources` files are extracted.  In dev we fall back to a
/// `extensions/builtin` folder relative to the executable / project root.
pub fn builtin_extension_dir(app: &AppHandle) -> PathBuf {
    // 1. Try the Tauri resource dir (packaged builds).
    if let Ok(res) = app.path().resource_dir() {
        let candidate = res.join("extensions").join("builtin");
        if candidate.exists() {
            return candidate;
        }
    }

    // 2. Dev fallback: look relative to the current working directory.
    let dev1 = std::env::current_dir()
        .unwrap_or_default()
        .join("extensions")
        .join("builtin");
    if dev1.exists() {
        return dev1;
    }

    // 3. Dev fallback: relative to src-tauri (where cargo runs from).
    let dev2 = std::env::current_dir()
        .unwrap_or_default()
        .join("src-tauri")
        .join("extensions")
        .join("builtin");
    if dev2.exists() {
        return dev2;
    }

    // 4. Last resort: the resource-dir guess (even if it doesn't exist)
    //    so error messages are still informative.
    app.path()
        .resource_dir()
        .map(|r| r.join("extensions").join("builtin"))
        .unwrap_or(dev1)
}

/// Convenience wrapper for the `App` type used in `setup()`.
pub fn builtin_extension_dir_app(app: &App) -> PathBuf {
    builtin_extension_dir(&app.handle())
}

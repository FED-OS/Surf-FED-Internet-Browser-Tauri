// Extension registry — discovery, enable/disable state, persistence.
//
// Unlike Electron (which loads real Chrome extensions via the Chromium
// runtime), Tauri has no browser-extension loader.  Instead we treat
// "extensions" as a registry of installed modules whose *functionality*
// is implemented natively in Rust (see `commands` + `blocking`).
//
// The registry still reads `manifest.json` files and presents them in the
// Extensions Manager UI exactly like the Electron version, so the user
// experience is preserved.  What changed is *how* each extension's features
// are executed: natively, rather than through Chromium's extension APIs.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle, Manager};

use crate::helpers::{
    builtin_extension_dir_app, ensure_dir, copy_dir_recursive, read_manifest, read_json,
    user_extension_dir, write_json, extension_state_dir,
};

/// One installed extension as the frontend sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub builtin: bool,
    pub load_error: Option<String>,
    /// The manifest_kind tells the frontend which native feature group to
    /// activate (e.g. "ad-blocker", "dark-reader", "page-info", "fed-gram").
    pub kind: String,
}

/// Internal registry entry (keeps the on-disk path too).
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub info: ExtensionInfo,
    pub path: PathBuf,
}

/// The shared state managed by Tauri.
pub struct ExtensionRegistry {
    pub entries: Mutex<HashMap<String, RegistryEntry>>,
}

/// Persisted enabled/disabled state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedState {
    #[serde(default)]
    disabled: Vec<String>,
    #[serde(default)]
    adblock_enabled: bool,
    #[serde(default)]
    darkreader_enabled: bool,
}

fn state_file(app: &AppHandle) -> PathBuf {
    extension_state_dir(app).unwrap_or_default().join("state.json")
}

fn load_state(app: &AppHandle) -> PersistedState {
    read_json::<PersistedState>(&state_file(app)).unwrap_or_default()
}

fn save_state(app: &AppHandle, state: &PersistedState) {
    let _ = write_json(&state_file(app), state);
}

/// Guess the native "kind" of an extension from its manifest name / id.
/// This lets the frontend know which native feature to wire up.
fn guess_kind(id: &str, manifest: &serde_json::Value) -> String {
    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let id_lower = id.to_lowercase();

    if id_lower.contains("ad-blocker") || name.contains("ad block") {
        return "ad-blocker".into();
    }
    if id_lower.contains("dark-reader") || name.contains("dark reader") {
        return "dark-reader".into();
    }
    if id_lower.contains("page-info") || name.contains("page info") {
        return "page-info".into();
    }
    if id_lower.contains("fed-gram") || name.contains("fed-gram") || name.contains("instagram") {
        return "fed-gram".into();
    }
    "custom".into()
}

/// Discover every sub-folder in `dir` that contains a manifest.json.
fn discover(dir: &PathBuf, builtin: bool) -> Vec<RegistryEntry> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return found,
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let ext_path = entry.path();
        let manifest = match read_manifest(&ext_path) {
            Some(m) => m,
            None => continue,
        };
        let id = entry.file_name().to_string_lossy().to_string();
        let name = manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0")
            .to_string();
        let description = manifest
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kind = guess_kind(&id, &manifest);
        found.push(RegistryEntry {
            info: ExtensionInfo {
                id,
                name,
                version,
                description,
                enabled: true,
                builtin,
                load_error: None,
                kind,
            },
            path: ext_path,
        });
    }
    found
}

/// Initialise the registry at startup: discover built-in + user extensions,
/// apply persisted enabled/disabled state, and return the managed state.
pub fn init_registry(app: &App) -> Result<ExtensionRegistry, String> {
    let handle = app.handle();
    let state = load_state(handle);
    let disabled: std::collections::HashSet<String> =
        state.disabled.iter().cloned().collect();

    let user_dir = user_extension_dir(handle)?;
    ensure_dir(&user_dir)?;

    let builtin_dir = builtin_extension_dir_app(app);
    log::info!("[extensions] built-in dir: {} (exists={})", builtin_dir.display(), builtin_dir.exists());

    let mut map = HashMap::new();

    for entry in discover(&builtin_dir, true) {
        let mut e = entry;
        e.info.enabled = !disabled.contains(&e.info.id);
        map.insert(e.info.id.clone(), e);
    }
    for entry in discover(&user_dir, false) {
        let mut e = entry;
        e.info.enabled = !disabled.contains(&e.info.id);
        map.insert(e.info.id.clone(), e);
    }

    let count = map.len();
    let enabled_count = map.values().filter(|e| e.info.enabled).count();
    log::info!("[extensions] {count} discovered, {enabled_count} enabled.");

    // Stash the ad-block / dark-reader toggles in a separate piece of state
    // (kept inside the same registry file for convenience).
    app.manage(ToggleState {
        adblock: Mutex::new(state.adblock_enabled),
        darkreader: Mutex::new(state.darkreader_enabled),
    });

    Ok(ExtensionRegistry {
        entries: Mutex::new(map),
    })
}

/// Separate managed state for the two always-available native toggles
/// (ad-blocker, dark-reader) so the frontend can query them quickly.
pub struct ToggleState {
    pub adblock: Mutex<bool>,
    pub darkreader: Mutex<bool>,
}

// ---- functions used by the command layer ----

pub fn list_info(reg: &ExtensionRegistry) -> Vec<ExtensionInfo> {
    let map = reg.entries.lock().unwrap();
    let mut v: Vec<ExtensionInfo> = map.values().map(|e| e.info.clone()).collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

pub fn set_enabled(
    app: &AppHandle,
    reg: &ExtensionRegistry,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut map = reg.entries.lock().unwrap();
    let entry = map
        .get_mut(id)
        .ok_or_else(|| format!("Extension not found: {id}"))?;
    entry.info.enabled = enabled;

    let mut state = load_state(app);
    if enabled {
        state.disabled.retain(|d| d != id);
    } else if !state.disabled.contains(&id.to_string()) {
        state.disabled.push(id.to_string());
    }
    save_state(app, &state);
    Ok(())
}

pub fn add_extension(
    app: &AppHandle,
    reg: &ExtensionRegistry,
    src: &PathBuf,
) -> Result<ExtensionInfo, String> {
    let manifest =
        read_manifest(src).ok_or_else(|| "The selected folder does not contain a valid manifest.json".to_string())?;

    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "extension".into());
    let dest = user_extension_dir(app)?.join(&name);
    if dest.exists() {
        let _ = fs::remove_dir_all(&dest);
    }
    ensure_dir(&dest)?;
    copy_dir_recursive(src, &dest)
        .map_err(|e| format!("Could not copy extension: {e}"))?;

    let id = name.clone();
    let kind = guess_kind(&id, &manifest);
    let info = ExtensionInfo {
        id: id.clone(),
        name: manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&name)
            .to_string(),
        version: manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0")
            .to_string(),
        description: manifest
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        enabled: true,
        builtin: false,
        load_error: None,
        kind,
    };

    let mut map = reg.entries.lock().unwrap();
    map.insert(
        id.clone(),
        RegistryEntry {
            info: info.clone(),
            path: dest,
        },
    );
    Ok(info)
}

pub fn remove_extension(
    app: &AppHandle,
    reg: &ExtensionRegistry,
    id: &str,
) -> Result<(), String> {
    let mut map = reg.entries.lock().unwrap();
    let entry = map
        .get(id)
        .ok_or_else(|| format!("Extension not found: {id}"))?;
    if entry.info.builtin {
        return Err("Built-in extensions cannot be removed".into());
    }
    let path = entry.path.clone();
    map.remove(id);
    drop(map);

    let _ = fs::remove_dir_all(&path);

    let mut state = load_state(app);
    state.disabled.retain(|d| d != id);
    save_state(app, &state);
    Ok(())
}

pub fn reload_user_extensions(app: &AppHandle, reg: &ExtensionRegistry) -> Result<usize, String> {
    let user_dir = user_extension_dir(app)?;
    ensure_dir(&user_dir)?;
    let state = load_state(app);
    let disabled: std::collections::HashSet<String> =
        state.disabled.iter().cloned().collect();

    let discovered = discover(&user_dir, false);
    let mut added = 0;
    let mut map = reg.entries.lock().unwrap();
    for entry in discovered {
        if map.contains_key(&entry.info.id) {
            continue;
        }
        let mut e = entry;
        e.info.enabled = !disabled.contains(&e.info.id);
        map.insert(e.info.id.clone(), e);
        added += 1;
    }
    Ok(added)
}

/// Persist the ad-blocker / dark-reader toggle booleans.
pub fn save_toggles(app: &AppHandle, adblock: bool, darkreader: bool) {
    let mut state = load_state(app);
    state.adblock_enabled = adblock;
    state.darkreader_enabled = darkreader;
    save_state(app, &state);
}

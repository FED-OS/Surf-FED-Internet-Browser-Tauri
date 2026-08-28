// Surf FED — Tauri backend (lib)
//
// This is the Rust equivalent of Electron's `main.js`.  It:
//   * creates the main browser window (decorations off → custom title bar)
//   * registers all IPC commands the frontend calls via `invoke()`
//   * wires up the dialog / shell / fs plugins
//   * initialises the extension system at startup
//
// The commands are split into modules for readability:
//   * `commands`   — the #[tauri::command] functions exposed to the frontend
//   * `extensions` — extension discovery, loading-state, persistence
//   * `blocking`   — the native ad-blocker (replaces declarativeNetRequest)
//   * `helpers`    — shared file/path utilities

mod commands;
mod extensions;
mod blocking;
mod helpers;

use tauri::Manager;

/// The Tauri-managed state key for the extension registry.
pub const EXT_STATE_KEY: &str = "surf-fed-ext-state";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init()
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Resolve and create the user-data directories we need.
            let user_ext_dir = helpers::user_extension_dir(app)?;
            let ext_state_dir = helpers::extension_state_dir(app)?;
            helpers::ensure_dir(&user_ext_dir)?;
            helpers::ensure_dir(&ext_state_dir)?;

            // Discover + register built-in and user extensions.
            let registry = extensions::init_registry(app)?;
            app.manage(registry);

            // Install the native ad-blocker request filter.
            blocking::install(app)?;

            log::info!("Surf FED backend initialised.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Window chrome
            commands::window_minimize,
            commands::window_maximize,
            commands::window_close,
            commands::window_start_dragging,
            // Extension management
            commands::extensions_list,
            commands::extensions_enable,
            commands::extensions_disable,
            commands::extensions_add,
            commands::extensions_remove,
            commands::extensions_open_folder,
            commands::extensions_reload,
            // Native built-in extension features
            commands::adblock_toggle,
            commands::adblock_status,
            commands::adblock_check,
            commands::darkreader_toggle,
            commands::darkreader_status,
            commands::darkreader_get_css,
            commands::page_info_get,
            commands::fedgram_extract,
            commands::fedgram_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Surf FED");
}

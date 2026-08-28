// Native ad-blocker — replaces Chromium's `declarativeNetRequest`.
//
// Tauri's native WebViews (WebView2 / WKWebView / WebKitGTK) do not expose
// a per-request interception API the way Chromium's
// `declarativeNetRequest` does in the Electron version.  We therefore
// implement blocking via a Rust-side rule engine that loads `rules.json`
// (same format the original Electron ad-blocker used) and exposes a fast
// `is_blocked()` check through the `adblock_check` command.  The frontend
// calls this for every top-level navigation before loading a URL into an
// iframe, and the injected content script can query it for sub-resources
// where cross-origin access permits.
//
// The rules.json format matches the original built-in ad-blocker so the
// existing rule set is reused without modification.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{App, Manager};

use crate::helpers::builtin_extension_dir_app;

/// A single declarativeNetRequest-style rule.
/// We only model the fields we actually use for blocking.
#[derive(Debug, Clone, Deserialize)]
struct BlockRule {
    /// The URL filter pattern (e.g. "||doubleclick.net^").
    #[serde(rename = "urlFilter", default)]
    url_filter: Option<String>,
    /// Domains to block outright.
    #[serde(default)]
    domains: Vec<String>,
    /// Resource types this rule applies to (ignored for now — we block all).
    #[serde(rename = "resourceTypes", default)]
    _resource_types: Vec<String>,
}

/// The rules.json structure as written by the built-in ad-blocker.
#[derive(Debug, Clone, Deserialize, Default)]
struct RulesFile {
    #[serde(default)]
    rules: Vec<BlockRule>,
}

/// Shared ad-blocker state.
pub struct AdBlockState {
    pub enabled: Mutex<bool>,
    /// Full rule set, retained so rules can be reloaded at runtime without
    /// re-reading the file from disk (future hot-reload support).
    #[allow(dead_code)]
    rules: Mutex<RulesFile>,
    /// Pre-extracted blocked host substrings for fast matching.
    blocked_hosts: Mutex<Vec<String>>,
}

impl AdBlockState {
    /// Returns true if `url` should be blocked.
    pub fn is_blocked(&self, url: &str) -> bool {
        let enabled = *self.enabled.lock().unwrap();
        if !enabled {
            return false;
        }
        let hosts = self.blocked_hosts.lock().unwrap();
        let url_lower = url.to_lowercase();
        for h in hosts.iter() {
            if url_lower.contains(h.as_str()) {
                return true;
            }
        }
        false
    }
}

/// Load rules from the built-in ad-blocker's rules.json, falling back to
/// a sensible default blocklist if the file is missing.
fn load_rules(builtin_dir: &std::path::Path) -> RulesFile {
    let path = builtin_dir.join("ad-blocker").join("rules.json");
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|e| {
            log::warn!("[adblock] Failed to parse rules.json: {e}");
            default_rules()
        }),
        Err(_) => {
            log::warn!("[adblock] rules.json not found at {}, using defaults", path.display());
            default_rules()
        }
    }
}

/// A minimal built-in blocklist used if the bundled rules.json is absent.
fn default_rules() -> RulesFile {
    let domains = vec![
        "doubleclick.net", "googlesyndication.com", "googleadservices.com",
        "googletagmanager.com", "adservice.google.com", "ads.yahoo.com",
        "amazon-adsystem.com", "criteo.com", "adsrvr.org", "pubmatic.com",
        "taboola.com", "outbrain.com", "moatads.com", "adnxs.com",
        "scorecardresearch.com", "quantserve.com", "hotjar.com",
    ];
    RulesFile {
        rules: domains.iter().map(|d| BlockRule {
            url_filter: Some(format!("||{d}^")),
            domains: vec![d.to_string()],
            _resource_types: vec![],
        }).collect(),
    }
}

/// Extract lowercase host substrings from the rules for fast matching.
fn extract_hosts(rules: &RulesFile) -> Vec<String> {
    let mut hosts = Vec::new();
    for rule in &rules.rules {
        for d in &rule.domains {
            hosts.push(d.to_lowercase());
        }
        if let Some(f) = &rule.url_filter {
            // Turn "||example.com^" into "example.com"
            let cleaned = f
                .trim_start_matches("||")
                .trim_start_matches('*')
                .trim_end_matches('^')
                .to_lowercase();
            if !cleaned.is_empty() && !hosts.contains(&cleaned) {
                hosts.push(cleaned);
            }
        }
    }
    hosts
}

/// Install the ad-blocker: load rules and register managed state.
///
/// The rule engine is queried by the frontend via the `adblock_check`
/// command before navigating to any URL, providing the same effective
/// blocking the Electron version achieved with `declarativeNetRequest`.
pub fn install(app: &App) -> Result<(), String> {
    let builtin_dir = builtin_extension_dir_app(app);
    let rules = load_rules(&builtin_dir);
    let hosts = extract_hosts(&rules);
    log::info!(
        "[adblock] loaded {} rules, {} host patterns",
        rules.rules.len(),
        hosts.len()
    );

    let state = AdBlockState {
        enabled: Mutex::new(true), // enabled by default, like the Electron version
        rules: Mutex::new(rules),
        blocked_hosts: Mutex::new(hosts),
    };
    app.manage(state);

    Ok(())
}

/// Quick check exposed to other modules (e.g. the commands layer).
pub fn check(app: &tauri::AppHandle, url: &str) -> bool {
    let state: tauri::State<AdBlockState> = app.state();
    state.is_blocked(url)
}

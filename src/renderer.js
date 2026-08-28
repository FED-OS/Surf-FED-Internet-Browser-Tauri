// Surf FED — Tauri frontend renderer
//
// Ported from the original Electron renderer.js.  Key changes:
//   * Electron <webview> tags  ->  <iframe> elements for web content
//   * window.electronAPI.*     ->  Tauri invoke() + plugin APIs
//   * Custom frameless title bar with min/max/close + drag
//   * Native built-in extensions (ad-blocker, dark-reader, page-info,
//     fed-gram) wired to Rust commands
//
// The tab / navigation logic is otherwise faithful to the original.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let tabs = [];
let activeTabId = null;
let tabIdCounter = 0;
let darkReaderCss = null;   // cached CSS from Rust
let darkReaderOn = false;

const urlBar            = document.getElementById("urlBar");
const tabsContainer     = document.getElementById("tabsContainer");
const webviewContainer  = document.getElementById("webviewContainer");
const newTabBtn         = document.getElementById("newTabBtn");
const backBtn           = document.getElementById("backBtn");
const forwardBtn        = document.getElementById("forwardBtn");
const reloadBtn         = document.getElementById("reloadBtn");
const darkModeBtn       = document.getElementById("darkModeBtn");
const pageInfoBtn       = document.getElementById("pageInfoBtn");
const fedgramBtn        = document.getElementById("fedgramBtn");
const extensionsBtn     = document.getElementById("extensionsBtn");
const extensionsPanel   = document.getElementById("extensionsPanel");
const extCloseBtn       = document.getElementById("extCloseBtn");
const extAddBtn         = document.getElementById("extAddBtn");
const extOpenFolderBtn  = document.getElementById("extOpenFolderBtn");
const extReloadBtn      = document.getElementById("extReloadBtn");
const extList           = document.getElementById("extList");
const titlebarTitle     = document.getElementById("titlebarTitle");

// ---------------------------------------------------------------------------
// Title bar (frameless window controls)
// ---------------------------------------------------------------------------

document.getElementById("minBtn").addEventListener("click", () => invoke("window_minimize"));
document.getElementById("maxBtn").addEventListener("click", () => invoke("window_maximize"));
document.getElementById("closeBtn").addEventListener("click", () => invoke("window_close"));

// Make the titlebar draggable (Tauri start_dragging)
document.getElementById("titlebar").addEventListener("mousedown", async (e) => {
  // Only drag when clicking the empty title area, not the buttons.
  if (e.target.id === "titlebar" || e.target.classList.contains("title")) {
    await invoke("window_start_dragging");
  }
});

// ---------------------------------------------------------------------------
// Tab management
// ---------------------------------------------------------------------------

function createTab(url = "about:blank", isActive = true) {
  const id = ++tabIdCounter;
  const tab = { id, title: "New Tab", frame: null, fallback: null, history: [], histIndex: -1 };
  tabs.push(tab);

  // Create the iframe that will render web content.
  const frame = document.createElement("iframe");
  frame.className = "browse-frame";
  frame.setAttribute("sandbox", "allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads allow-modals");
  frame.setAttribute("referrerpolicy", "no-referrer");
  webviewContainer.appendChild(frame);
  tab.frame = frame;

  // Fallback element shown when a site refuses to be framed.
  const fallback = document.createElement("div");
  fallback.className = "frame-fallback";
  webviewContainer.appendChild(fallback);
  tab.fallback = fallback;

  frame.addEventListener("load", () => {
    try {
      // Cross-origin iframes throw on .contentWindow.document access, which
      // is expected — we can still read .src for the URL bar.
      tab.title = "Loading…";
    } catch (e) { /* cross-origin, expected */ }
    try { urlBar.value = frame.src && frame.src !== "about:blank" ? frame.src : ""; } catch (e) {}
    updateTabUI();
    maybeInjectDarkReader(tab);
  });

  // Detect sites that block framing via load timeout / blank content.
  frame.addEventListener("error", () => showFallback(tab, url));

  const tabEl = document.createElement("div");
  tabEl.className = "tab";
  tabEl.dataset.id = id;
  tabEl.innerHTML = `<span>${escapeHtml(tab.title)}</span><button class="close-tab">\u00d7</button>`;
  tabEl.addEventListener("click", (e) => {
    if (e.target.classList.contains("close-tab")) return;
    activateTab(id);
  });
  tabEl.querySelector(".close-tab").addEventListener("click", (e) => {
    e.stopPropagation();
    closeTab(id);
  });
  tabsContainer.appendChild(tabEl);

  if (isActive) activateTab(id);
  else { tab.frame.style.display = "none"; tab.fallback.style.display = "none"; }

  if (url && url !== "about:blank") navigateTo(url, tab);
  updateTabUI();
  return tab;
}

function activateTab(id) {
  activeTabId = id;
  tabs.forEach(t => {
    const isActive = t.id === id;
    t.frame.classList.toggle("active", isActive);
    t.fallback.classList.toggle("active", false);
    t.frame.style.display = isActive ? "block" : "none";
    t.fallback.style.display = "none";
    if (isActive) {
      try { urlBar.value = t.frame.src && t.frame.src !== "about:blank" ? t.frame.src : ""; } catch (e) {}
      titlebarTitle.textContent = "Surf FED — " + (t.title || "New Tab");
    }
  });
  updateTabUI();
}

function closeTab(id) {
  const idx = tabs.findIndex(t => t.id === id);
  if (idx === -1) return;
  tabs[idx].frame.remove();
  tabs[idx].fallback.remove();
  tabs.splice(idx, 1);
  tabsContainer.children[idx]?.remove();
  if (tabs.length === 0) createTab();
  else if (activeTabId === id) activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
  updateTabUI();
}

function updateTabUI() {
  const tabEls = tabsContainer.querySelectorAll(".tab");
  tabEls.forEach((el, i) => {
    const tab = tabs[i];
    if (!tab) return;
    el.classList.toggle("active", tab.id === activeTabId);
    el.querySelector("span").textContent = tab.title;
  });
}

function getActiveTab() {
  return tabs.find(t => t.id === activeTabId);
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

function normalizeUrl(input) {
  if (!input) return null;
  let url = input.trim();
  if (!url) return null;
  // Bare search query -> Google
  if (!url.includes(".") && !url.startsWith("http")) {
    return "https://www.google.com/search?q=" + encodeURIComponent(url);
  }
  if (!/^https?:\/\//i.test(url)) {
    url = "https://" + url;
  }
  return url;
}

async function navigateTo(rawUrl, tab) {
  const url = normalizeUrl(rawUrl);
  if (!url) return;
  tab = tab || getActiveTab();
  if (!tab) return;

  // Ad-blocker: check top-level URL via Rust.
  try {
    const blocked = await invoke("adblock_check", { url });
    if (blocked) {
      showBlocked(tab, url);
      return;
    }
  } catch (e) { /* adblock may not be ready; proceed */ }

  // Push to history
  if (tab.histIndex < tab.history.length - 1) {
    tab.history = tab.history.slice(0, tab.histIndex + 1);
  }
  tab.history.push(url);
  tab.histIndex = tab.history.length - 1;

  tab.title = hostnameOf(url) || "Loading…";
  tab.frame.src = url;
  urlBar.value = url;
  titlebarTitle.textContent = "Surf FED — " + tab.title;
  updateTabUI();

  // Some sites send X-Frame-Options: DENY and the iframe stays blank.
  // We detect this with a short timeout and show a fallback link.
  setTimeout(() => {
    if (tab.id !== activeTabId) return;
    try {
      const w = tab.frame.contentWindow;
      // If we got a blank or error state, show fallback.
      if (w && w.location && (w.location === "about:blank" || w.location.href === "about:blank")) {
        // could not load (framing blocked) — only show fallback if src was set
        if (tab.frame.src && tab.frame.src !== "about:blank") {
          showFallback(tab, url);
        }
      }
    } catch (e) {
      // cross-origin means it DID load — that's fine, hide fallback.
      tab.fallback.classList.remove("active");
      tab.fallback.style.display = "none";
      tab.frame.style.display = "block";
    }
  }, 1500);
}

function showFallback(tab, url) {
  tab.fallback.innerHTML = `
    <h3>This site can't be embedded</h3>
    <p><strong>${escapeHtml(hostnameOf(url) || url)}</strong> sends headers that prevent it
    from being displayed inside another page (X-Frame-Options / Content-Security-Policy).</p>
    <a href="${escapeAttr(url)}" target="_blank" rel="noopener">Open in new window &#8599;</a>
  `;
  tab.fallback.classList.add("active");
  tab.fallback.style.display = "flex";
  tab.frame.style.display = "none";
}

function showBlocked(tab, url) {
  tab.fallback.innerHTML = `
    <h3>&#9940; Blocked by Ad Blocker</h3>
    <p>${escapeHtml(hostnameOf(url) || url)} was blocked by the built-in ad blocker.
    You can disable it in the Extensions panel.</p>
  `;
  tab.fallback.classList.add("active");
  tab.fallback.style.display = "flex";
  tab.frame.style.display = "none";
}

function hostnameOf(url) {
  try { return new URL(url).hostname.replace(/^www\./, ""); }
  catch { return ""; }
}

// Toolbar button handlers
newTabBtn.addEventListener("click", () => createTab());
backBtn.addEventListener("click", () => {
  const tab = getActiveTab();
  if (!tab || tab.histIndex <= 0) return;
  tab.histIndex--;
  const url = tab.history[tab.histIndex];
  tab.frame.src = url;
  urlBar.value = url;
});
forwardBtn.addEventListener("click", () => {
  const tab = getActiveTab();
  if (!tab || tab.histIndex >= tab.history.length - 1) return;
  tab.histIndex++;
  const url = tab.history[tab.histIndex];
  tab.frame.src = url;
  urlBar.value = url;
});
reloadBtn.addEventListener("click", () => {
  const tab = getActiveTab();
  if (tab && tab.frame.src) tab.frame.src = tab.frame.src;
});
urlBar.addEventListener("keydown", (e) => {
  if (e.key === "Enter") navigateTo(urlBar.value);
});

// ---------------------------------------------------------------------------
// Dark mode (browser UI)
// ---------------------------------------------------------------------------

darkModeBtn.addEventListener("click", () => {
  const isDark = document.body.classList.toggle("dark-mode");
  darkModeBtn.textContent = isDark ? "\u2600\ufe0f" : "\ud83c\udf19";
});

// ---------------------------------------------------------------------------
// Dark Reader (native built-in extension — CSS injection into iframes)
// ---------------------------------------------------------------------------

async function getDarkReaderCss() {
  if (darkReaderCss) return darkReaderCss;
  try { darkReaderCss = await invoke("darkreader_get_css"); }
  catch (e) { darkReaderCss = null; }
  return darkReaderCss;
}

async function maybeInjectDarkReader(tab) {
  if (!darkReaderOn) return;
  const css = await getDarkReaderCss();
  if (!css) return;
  try {
    // Attempt to inject into the iframe. Cross-origin iframes will throw,
    // which is expected — the dark-reader can only style same-origin pages.
    const doc = tab.frame.contentDocument;
    if (!doc) return;
    if (doc.getElementById("surf-fed-dark-reader-style")) return;
    const style = doc.createElement("style");
    style.id = "surf-fed-dark-reader-style";
    style.textContent = css;
    (doc.head || doc.documentElement).appendChild(style);
  } catch (e) { /* cross-origin: silently skip */ }
}

async function toggleDarkReader(on) {
  darkReaderOn = on;
  await invoke("darkreader_toggle", { enabled: on });
  // Apply to all existing tabs (best-effort, same-origin only).
  for (const tab of tabs) {
    if (on) { await maybeInjectDarkReader(tab); }
    else {
      try {
        const doc = tab.frame.contentDocument;
        doc?.getElementById("surf-fed-dark-reader-style")?.remove();
      } catch (e) {}
    }
  }
}

// ---------------------------------------------------------------------------
// Page Info (native built-in extension)
// ---------------------------------------------------------------------------

const pageInfoOverlay = document.getElementById("pageInfoOverlay");
const pageInfoContent = document.getElementById("pageInfoContent");
document.getElementById("pageInfoClose").addEventListener("click", () => {
  pageInfoOverlay.classList.remove("active");
});
pageInfoOverlay.addEventListener("click", (e) => {
  if (e.target === pageInfoOverlay) pageInfoOverlay.classList.remove("active");
});

pageInfoBtn.addEventListener("click", () => {
  const tab = getActiveTab();
  pageInfoOverlay.classList.add("active");
  pageInfoContent.innerHTML = '<p class="ext-loading">Loading…</p>';
  if (!tab || !tab.frame.src || tab.frame.src === "about:blank") {
    pageInfoContent.innerHTML = "<p>No page loaded.</p>";
    return;
  }
  // Try to extract metadata from the iframe (same-origin only).
  let info = { title: tab.title, url: tab.frame.src, description: "", linkCount: 0, imageCount: 0 };
  try {
    const doc = tab.frame.contentDocument;
    if (doc) {
      info.title = doc.title || info.title;
      info.description = doc.querySelector('meta[name="description"]')?.getAttribute("content") || "";
      info.linkCount = doc.links ? doc.links.length : 0;
      info.imageCount = doc.images ? doc.images.length : 0;
    }
  } catch (e) { /* cross-origin — use what we have */ }

  // Route through the Rust command (keeps a single source of truth).
  invoke("page_info_get", {
    title: info.title, url: info.url, description: info.description,
    linkCount: info.linkCount, imageCount: info.imageCount
  }).then((pi) => {
    pageInfoContent.innerHTML = `
      <div class="pi-field"><div class="pi-label">Title</div><div class="pi-value">${escapeHtml(pi.title)}</div></div>
      <div class="pi-field"><div class="pi-label">URL</div><div class="pi-value">${escapeHtml(pi.url)}</div></div>
      <div class="pi-field"><div class="pi-label">Description</div><div class="pi-value">${escapeHtml(pi.description || "—")}</div></div>
      <div class="pi-field"><div class="pi-label">Links</div><div class="pi-value">${pi.link_count}</div></div>
      <div class="pi-field"><div class="pi-label">Images</div><div class="pi-value">${pi.image_count}</div></div>
    `;
  }).catch((e) => {
    pageInfoContent.innerHTML = `<p class="ext-error">Error: ${escapeHtml(String(e))}</p>`;
  });
});

// ---------------------------------------------------------------------------
// FED-GRAM (native built-in extension — Instagram image downloader)
// ---------------------------------------------------------------------------

const fedgramOverlay   = document.getElementById("fedgramOverlay");
const fedgramUrlInput  = document.getElementById("fedgramUrl");
const fedgramExtractBtn= document.getElementById("fedgramExtractBtn");
const fedgramStatus    = document.getElementById("fedgramStatus");
const fedgramImages    = document.getElementById("fedgramImages");

fedgramBtn.addEventListener("click", () => {
  fedgramOverlay.classList.add("active");
  // Pre-fill with the active tab's URL if it's an Instagram post.
  const tab = getActiveTab();
  if (tab && tab.frame.src && tab.frame.src.includes("instagram.com")) {
    fedgramUrlInput.value = tab.frame.src;
  }
});
document.getElementById("fedgramClose").addEventListener("click", () => {
  fedgramOverlay.classList.remove("active");
});
fedgramOverlay.addEventListener("click", (e) => {
  if (e.target === fedgramOverlay) fedgramOverlay.classList.remove("active");
});

fedgramExtractBtn.addEventListener("click", async () => {
  const url = fedgramUrlInput.value.trim();
  if (!url) { fedgramStatus.textContent = "Please paste an Instagram post URL."; return; }
  fedgramExtractBtn.disabled = true;
  fedgramStatus.textContent = "Extracting images…";
  fedgramStatus.className = "fedgram-status";
  fedgramImages.innerHTML = "";
  try {
    const result = await invoke("fedgram_extract", { postUrl: url });
    if (result.ok && result.images.length) {
      fedgramStatus.textContent = `Found ${result.images.length} image(s).`;
      fedgramStatus.className = "fedgram-status success";
      result.images.forEach((img, i) => {
        const item = document.createElement("div");
        item.className = "fedgram-img-item";
        item.innerHTML = `
          <img src="${escapeAttr(img.url)}" alt="image ${i+1}" onerror="this.style.opacity=0.2">
          <button data-url="${escapeAttr(img.url)}">💾 Download</button>
        `;
        item.querySelector("button").addEventListener("click", async () => {
          const name = `instagram_${Date.now()}_${i+1}.jpg`;
          try {
            const ok = await invoke("fedgram_download", { imageUrl: img.url, suggestedName: name });
            if (ok) { fedgramStatus.textContent = "Image saved!"; fedgramStatus.className = "fedgram-status success"; }
            else { fedgramStatus.textContent = "Download cancelled."; }
          } catch (e) {
            fedgramStatus.textContent = "Download failed: " + String(e);
            fedgramStatus.className = "fedgram-status error";
          }
        });
        fedgramImages.appendChild(item);
      });
    } else {
      fedgramStatus.textContent = result.error || "No images found.";
      fedgramStatus.className = "fedgram-status error";
    }
  } catch (e) {
    fedgramStatus.textContent = "Error: " + String(e);
    fedgramStatus.className = "fedgram-status error";
  } finally {
    fedgramExtractBtn.disabled = false;
  }
});

// ---------------------------------------------------------------------------
// Extensions Manager
// ---------------------------------------------------------------------------

function toggleExtensionsPanel() {
  const willOpen = extensionsPanel.classList.contains("hidden");
  extensionsPanel.classList.toggle("hidden");
  if (willOpen) refreshExtensions();
}

extensionsBtn.addEventListener("click", toggleExtensionsPanel);
extCloseBtn.addEventListener("click", () => extensionsPanel.classList.add("hidden"));

async function refreshExtensions() {
  extList.innerHTML = '<p class="ext-loading">Loading extensions…</p>';
  try {
    const list = await invoke("extensions_list");
    renderExtensionList(list);
  } catch (e) {
    extList.innerHTML = '<p class="ext-error">Could not load extensions: ' + escapeHtml(String(e)) + '</p>';
  }
}

function renderExtensionList(list) {
  if (!list || !list.length) {
    extList.innerHTML = '<p class="ext-empty">No extensions installed yet. Click “Load unpacked extension…” to add one.</p>';
    return;
  }
  extList.innerHTML = "";
  list.forEach(ext => {
    const row = document.createElement("div");
    row.className = "ext-row" + (ext.enabled ? " enabled" : "");

    const info = document.createElement("div");
    info.className = "ext-info";
    info.innerHTML = `
      <div class="ext-name">
        ${escapeHtml(ext.name)}
        <span class="ext-version">v${escapeHtml(ext.version)}</span>
        ${ext.builtin ? '<span class="ext-badge">built-in</span>' : ''}
        <span class="ext-kind">${escapeHtml(ext.kind)}</span>
      </div>
      <div class="ext-desc">${escapeHtml(ext.description || "No description")}</div>
      ${ext.loadError ? `<div class="ext-error">⚠ ${escapeHtml(ext.loadError)}</div>` : ""}
    `;

    const controls = document.createElement("div");
    controls.className = "ext-controls";

    const toggle = document.createElement("label");
    toggle.className = "ext-switch";
    toggle.title = ext.enabled ? "Disable" : "Enable";
    toggle.innerHTML = `<input type="checkbox" ${ext.enabled ? "checked" : ""}><span class="ext-slider"></span>`;
    const checkbox = toggle.querySelector("input");
    checkbox.addEventListener("change", async () => {
      checkbox.disabled = true;
      const cmd = checkbox.checked ? "extensions_enable" : "extensions_disable";
      // For the ad-blocker / dark-reader built-ins, also toggle the native feature.
      if (ext.kind === "ad-blocker") { await invoke("adblock_toggle", { enabled: checkbox.checked }); }
      if (ext.kind === "dark-reader") { await toggleDarkReader(checkbox.checked); }
      await invoke(cmd, { id: ext.id }).catch(() => {});
      checkbox.disabled = false;
      refreshExtensions();
    });
    controls.appendChild(toggle);

    if (!ext.builtin) {
      const removeBtn = document.createElement("button");
      removeBtn.className = "ext-remove-btn";
      removeBtn.textContent = "Remove";
      removeBtn.addEventListener("click", async () => {
        if (!confirm(`Remove extension "${ext.name}"?`)) return;
        await invoke("extensions_remove", { id: ext.id });
        refreshExtensions();
      });
      controls.appendChild(removeBtn);
    }

    row.appendChild(info);
    row.appendChild(controls);
    extList.appendChild(row);
  });
}

extAddBtn.addEventListener("click", async () => {
  const res = await invoke("extensions_add");
  if (res && res.ok) refreshExtensions();
  else if (res && res.error && res.error !== "No folder selected") {
    alert("Could not load extension:\n" + res.error);
  }
});

extOpenFolderBtn.addEventListener("click", () => invoke("extensions_openFolder"));
extReloadBtn.addEventListener("click", async () => {
  await invoke("extensions_reload");
  refreshExtensions();
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({
    "&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"
  }[c]));
}
function escapeAttr(s) {
  return String(s).replace(/"/g, "&quot;");
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

async function init() {
  // Sync the dark-reader toggle state from the backend.
  try {
    darkReaderOn = await invoke("darkreader_status");
  } catch (e) { darkReaderOn = false; }
  createTab("https://www.google.com", true);
}

init();

// Surf FED Page Info - popup script
// Queries the active tab for page metadata and displays it.

function setText(id, text) {
  const el = document.getElementById(id);
  if (!el) return;
  el.textContent = text;
}

function truncate(s, n) {
  if (!s) return s;
  return s.length > n ? s.slice(0, n) + '…' : s;
}

chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
  const tab = tabs && tabs[0];
  if (!tab) {
    setText('title', 'No active tab');
    return;
  }
  setText('title', truncate(tab.title, 60));
  const urlEl = document.getElementById('url');
  urlEl.textContent = truncate(tab.url, 60);

  // Run a function in the page to gather metadata.
  chrome.scripting.executeScript(
    {
      target: { tabId: tab.id },
      func: () => {
        const meta = document.querySelector('meta[name="description"]');
        return {
          title: document.title,
          url: location.href,
          description: meta ? meta.content : '',
          links: document.querySelectorAll('a').length,
          images: document.querySelectorAll('img').length,
        };
      },
    },
    (results) => {
      if (chrome.runtime.lastError) {
        setText('desc', '(not available on this page)');
        return;
      }
      const info = results && results[0] && results[0].result;
      if (!info) return;
      setText('desc', truncate(info.description, 80) || '(none)');
      setText('links', info.links);
      setText('images', info.images);
      setText('title', truncate(info.title, 60));
      urlEl.textContent = truncate(info.url, 60);
    }
  );
});

// FED-GRAM popup — download images from public Instagram posts.
// Mirrors the behaviour of the original Streamlit app (app.py):
//   - paste a post URL containing "/p/"
//   - fetch the post metadata
//   - preview image(s) and offer download (supports carousels)

const urlInput   = document.getElementById('url');
const extractBtn = document.getElementById('extractBtn');
const currentBtn = document.getElementById('useCurrentTabBtn');
const statusEl   = document.getElementById('status');
const resultsEl  = document.getElementById('results');

// ---------- UI helpers ----------

function setStatus(type, msg) {
  statusEl.hidden = !msg;
  statusEl.className = 'status ' + type;
  statusEl.textContent = msg || '';
}

function clearResults() {
  resultsEl.innerHTML = '';
}

function showImage(url, filename, index, total) {
  const card = document.createElement('div');
  card.className = 'result-card';

  const img = document.createElement('img');
  img.src = url;
  img.alt = filename;
  img.loading = 'lazy';
  card.appendChild(img);

  const bar = document.createElement('div');
  bar.className = 'card-bar';

  const label = document.createElement('span');
  label.className = 'label';
  label.textContent = total > 1 ? `Image ${index + 1} of ${total}` : filename;
  bar.appendChild(label);

  const dl = document.createElement('a');
  dl.className = 'dl';
  dl.textContent = '💾 Download';
  dl.href = url;
  dl.download = filename;
  // For cross-origin images the download attribute is often ignored; we fall
  // back to fetching the blob and triggering a save via the downloads API.
  dl.addEventListener('click', (e) => {
    e.preventDefault();
    downloadImage(url, filename);
  });
  bar.appendChild(dl);

  card.appendChild(bar);
  resultsEl.appendChild(card);
}

// Robust download: fetch the image as a blob, create an object URL, and
// trigger a save. We try chrome.downloads first (Chrome), then fall back to
// a programmatic anchor click (Electron / other browsers).
async function downloadImage(url, filename) {
  try {
    setStatus('busy', 'Preparing download…');
    const resp = await fetch(url, { mode: 'cors' });
    if (!resp.ok) throw new Error('HTTP ' + resp.status);
    const blob = await resp.blob();
    const objectUrl = URL.createObjectURL(blob);

    if (chrome && chrome.downloads && chrome.downloads.download) {
      chrome.downloads.download({ url: objectUrl, filename, saveAs: true });
    } else {
      const a = document.createElement('a');
      a.href = objectUrl;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
    }
    setStatus('ok', 'Download started: ' + filename);
    // Revoke a little later so the download can read the blob.
    setTimeout(() => URL.revokeObjectURL(objectUrl), 60000);
  } catch (e) {
    // Last-resort fallback: open the image in a new tab so the user can
    // right-click > Save image as…
    try {
      window.open(url, '_blank');
      setStatus('info', 'Opened image in a new tab — right-click to save.');
    } catch (_) {
      setStatus('error', 'Download failed: ' + e.message);
    }
  }
}

// ---------- URL validation (matches app.py's "/p/" check) ----------

function getShortcode(url) {
  if (!url) return null;
  const m = url.match(/\/p\/([^/?#]+)/);
  return m ? m[1] : null;
}

urlInput.addEventListener('input', () => {
  const sc = getShortcode(urlInput.value);
  extractBtn.disabled = !sc;
  if (urlInput.value && !sc) {
    setStatus('info', 'Paste a valid Instagram post URL containing "/p/"');
  } else {
    setStatus('', '');
  }
});

// ---------- "Use current tab" convenience ----------

currentBtn.addEventListener('click', () => {
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    const tab = tabs && tabs[0];
    if (tab && tab.url) {
      urlInput.value = tab.url;
      urlInput.dispatchEvent(new Event('input'));
    }
  });
});

// ---------- Instagram extraction ----------

async function extractPost() {
  const url = urlInput.value.trim();
  const shortcode = getShortcode(url);
  if (!shortcode) {
    setStatus('info', 'Paste a valid Instagram post URL containing "/p/"');
    return;
  }

  extractBtn.disabled = true;
  clearResults();
  setStatus('busy', 'Fetching post…');

  try {
    const post = await fetchInstagramPost(shortcode);
    renderPost(post, shortcode);
  } catch (e) {
    console.error('[FED-GRAM]', e);
    const msg = String(e.message || e);
    if (/private|not found|404/i.test(msg)) {
      setStatus('error', 'Failed. The post may be private, deleted, or the link is invalid.');
    } else if (/rate|429|connection|network/i.test(msg)) {
      setStatus('error', "Couldn't reach Instagram. Try again in a moment (rate limited).");
    } else {
      setStatus('error', 'Failed: ' + msg);
    }
  } finally {
    extractBtn.disabled = false;
  }
}

// Fetch a public Instagram post's media by reading the embed/og metadata.
// We try two strategies:
//   1) the public embed page  (instagram.com/p/<code>/embed/)
//   2) the post page's shared-data / og:image meta tags
// Both work for public posts without authentication.
async function fetchInstagramPost(shortcode) {
  const postUrl = `https://www.instagram.com/p/${shortcode}/`;

  // Strategy 1: embed page. It contains the main image (and for carousels,
  // the first image) in a predictable <img> tag.
  let images = [];
  try {
    images = await extractFromEmbed(shortcode);
  } catch (e) { /* fall through to strategy 2 */ }

  if (images.length === 0) {
    images = await extractFromPostPage(postUrl);
  }

  if (images.length === 0) {
    throw new Error('No images found (post may be private or deleted).');
  }
  return { images };
}

async function extractFromEmbed(shortcode) {
  const embedUrl = `https://www.instagram.com/p/${shortcode}/embed/captioned/`;
  const html = await fetchText(embedUrl);
  // The embed renders <img src="...cdninstagram.../...jpg">
  const matches = [...html.matchAll(/<img[^>]+src=["'](https:\/\/[^"']+(?:cdninstagram|fbcdn)[^"']+\.(?:jpg|png|webp)[^"']*)["']/gi)];
  const seen = new Set();
  const imgs = [];
  for (const m of matches) {
    const u = m[1];
    if (!seen.has(u)) { seen.add(u); imgs.push(u); }
  }
  // Drop tiny tracking/logo images by size hint if present.
  return imgs.filter(u => !/\/s150x150\//.test(u));
}

async function extractFromPostPage(postUrl) {
  const html = await fetchText(postUrl);
  // og:image is reliable for the primary image.
  const og = html.match(/<meta[^>]+property=["']og:image["'][^>]+content=["'](https:\/\/[^"']+)["']/i);
  const images = [];
  if (og) images.push(og[1]);

  // Some pages embed a JSON blob with all carousel images.
  const jsonMatches = [...html.matchAll(/"display_url":"(https:\\\/\\\/[^"]+)"/g)];
  const seen = new Set(images);
  for (const m of jsonMatches) {
    const u = m[1].replace(/\\\//g, '/');
    if (!seen.has(u)) { seen.add(u); images.push(u); }
  }
  return images;
}

async function fetchText(url) {
  const resp = await fetch(url, {
    headers: {
      // A normal browser UA helps avoid empty responses.
      'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ' +
                    '(KHTML, like Gecko) Chrome/120.0 Safari/537.36',
      'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
      'Accept-Language': 'en-US,en;q=0.9',
    },
  });
  if (!resp.ok) throw new Error('HTTP ' + resp.status);
  return resp.text();
}

function renderPost(post, shortcode) {
  clearResults();
  const total = post.images.length;
  if (total > 1) {
    const header = document.createElement('div');
    header.className = 'carousel-header';
    header.textContent = `📷 Carousel post (${total} images)`;
    resultsEl.appendChild(header);
  }
  post.images.forEach((imgUrl, i) => {
    const filename = total > 1
      ? `fedgram_${shortcode}_${i + 1}.jpg`
      : `fedgram_${shortcode}.jpg`;
    showImage(imgUrl, filename, i, total);
  });
  setStatus('ok', 'Done.');
}

extractBtn.addEventListener('click', extractPost);
urlInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !extractBtn.disabled) extractPost();
});

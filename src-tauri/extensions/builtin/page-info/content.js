// Surf FED Page Info - content script
// Lightweight; the popup does most of the work via chrome.scripting.
// This file exists so the extension registers a content script presence
// and could be extended later (e.g. to highlight links on hover).

(function () {
  // Mark the page as having the page-info extension present.
  document.documentElement.setAttribute('data-surf-fed-pageinfo', 'true');
})();

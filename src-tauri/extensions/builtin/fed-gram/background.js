// FED-GRAM background service worker.
// Currently no-op; kept so MV3 permissions (downloads) register correctly
// and so the extension has a long-lived context if we add features later.

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));

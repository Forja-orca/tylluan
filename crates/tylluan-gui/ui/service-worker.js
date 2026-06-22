// TylluanNexus Service Worker
const CACHE_NAME = 'tylluan-nexus-v3';
const ASSETS = [
  '/dashboard',
  '/style.css',
  '/app.js',
  '/favicon.svg'
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(ASSETS))
  );
});

self.addEventListener('fetch', (event) => {
  event.respondWith(
    caches.match(event.request).then((response) => {
      return response || fetch(event.request);
    })
  );
});

// Psst Service Worker — handles push notifications
const CACHE_NAME = 'psst-v1';

self.addEventListener('install', (event) => {
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(clients.claim());
});

self.addEventListener('push', (event) => {
  let data = { title: 'Psst', body: 'Quota update' };
  try {
    data = event.data ? event.data.json() : data;
  } catch (_) {
    data.body = event.data ? event.data.text() : data.body;
  }

  const options = {
    body: data.body || 'Quota update',
    icon: data.icon || '/manifest.json',
    badge: data.badge || '',
    tag: data.tag || 'psst-quota',
    requireInteraction: data.requireInteraction || false,
    data: data.url ? { url: data.url } : {},
  };

  event.waitUntil(
    self.registration.showNotification(data.title || 'Psst', options)
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const url = event.notification.data && event.notification.data.url
    ? event.notification.data.url
    : '/';
  event.waitUntil(
    clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clientList) => {
      for (const client of clientList) {
        if (client.url === url && 'focus' in client) return client.focus();
      }
      if (clients.openWindow) return clients.openWindow(url);
    })
  );
});

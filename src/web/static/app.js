// Psst Dashboard — frontend logic

(function () {
  'use strict';

  // Extract token from URL query params (persisted across refreshes)
  const params = new URLSearchParams(window.location.search);
  const TOKEN = params.get('token') || '';

  function apiUrl(path) {
    return TOKEN ? `${path}?token=${encodeURIComponent(TOKEN)}` : path;
  }

  // ── Rendering ─────────────────────────────────────────────────────────────

  function renderBar(pct) {
    const clamped = Math.min(100, Math.max(0, pct));
    let color = '#22c55e'; // green
    if (clamped >= 80) color = '#ef4444'; // red
    else if (clamped >= 50) color = '#f59e0b'; // amber
    return `
      <div class="bar-track">
        <div class="bar-fill" style="width:${clamped}%;background:${color}"></div>
        <span class="bar-label">${Math.round(clamped)}%</span>
      </div>`;
  }

  function renderWindow(name, w) {
    const pct = (w.utilization || 0) * 100;
    const tokens = w.used_tokens != null ? `${w.used_tokens.toLocaleString()} tokens` : '';
    const count  = w.used_count  != null ? `${w.used_count.toLocaleString()} requests` : '';
    const detail = [tokens, count].filter(Boolean).join(' · ');
    const resets = w.resets_at
      ? `Resets ${new Date(w.resets_at).toLocaleString()}`
      : '';
    return `
      <div class="window-card">
        <div class="window-header">
          <span class="window-name">${escHtml(name)}</span>
          ${detail ? `<span class="window-detail">${escHtml(detail)}</span>` : ''}
        </div>
        ${renderBar(pct)}
        ${resets ? `<div class="window-resets">${escHtml(resets)}</div>` : ''}
      </div>`;
  }

  function renderProvider(id, p) {
    const windows = Object.entries(p.windows || {});
    if (windows.length === 0) return '';
    return `
      <div class="provider-card">
        <h3 class="provider-name">${escHtml(id)}</h3>
        ${windows.map(([n, w]) => renderWindow(n, w)).join('')}
      </div>`;
  }

  function renderStatus(data) {
    const el = document.getElementById('status-section');
    if (!el) return;

    const providers = Object.entries(data.providers || {});
    const lastCheck = data.last_check_at
      ? new Date(data.last_check_at).toLocaleString()
      : 'Never';
    const tools = (data.discovered_tools || []).join(', ') || 'None detected';

    el.innerHTML = `
      <div class="meta-row">
        <span>Last check: <strong>${escHtml(lastCheck)}</strong></span>
        <span>Tools: <strong>${escHtml(tools)}</strong></span>
      </div>
      ${providers.length === 0
        ? '<p class="empty-msg">No provider data yet. Waiting for first check…</p>'
        : providers.map(([id, p]) => renderProvider(id, p)).join('')
      }`;
  }

  function escHtml(str) {
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  // ── Fetch ─────────────────────────────────────────────────────────────────

  async function fetchStatus() {
    try {
      const res = await fetch(apiUrl('/api/status'));
      if (!res.ok) {
        if (res.status === 401) {
          document.getElementById('status-section').innerHTML =
            '<p class="error-msg">Unauthorized. Add ?token=YOUR_TOKEN to the URL.</p>';
        }
        return;
      }
      const data = await res.json();
      renderStatus(data);
      document.getElementById('last-updated').textContent =
        'Updated ' + new Date().toLocaleTimeString();
    } catch (err) {
      console.error('Failed to fetch status:', err);
    }
  }

  // ── Push Notifications ────────────────────────────────────────────────────

  async function registerServiceWorker() {
    if (!('serviceWorker' in navigator)) return null;
    return navigator.serviceWorker.register('/sw.js');
  }

  async function subscribePush() {
    const btn = document.getElementById('push-btn');
    if (!btn) return;
    btn.disabled = true;
    btn.textContent = 'Enabling…';

    try {
      if (!('PushManager' in window)) {
        btn.textContent = 'Push not supported';
        return;
      }

      const reg = await registerServiceWorker();
      if (!reg) { btn.textContent = 'SW not supported'; return; }

      const permission = await Notification.requestPermission();
      if (permission !== 'granted') {
        btn.textContent = 'Permission denied';
        btn.disabled = false;
        return;
      }

      // Use a dummy VAPID key (real key would come from server config).
      // For now we just store the subscription endpoint; actual push delivery
      // requires a VAPID key pair configured in the server.
      const subscription = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        // applicationServerKey will be provided in a future release with VAPID support
      }).catch(() => null);

      if (!subscription) {
        btn.textContent = 'Subscription failed';
        btn.disabled = false;
        return;
      }

      const subJson = subscription.toJSON();
      const body = {
        endpoint: subJson.endpoint,
        keys: {
          p256dh: subJson.keys ? subJson.keys.p256dh : '',
          auth:   subJson.keys ? subJson.keys.auth   : '',
        },
      };

      const res = await fetch(apiUrl('/api/subscribe'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });

      if (res.ok) {
        btn.textContent = 'Notifications enabled';
        btn.classList.add('btn-success');
      } else {
        btn.textContent = 'Subscribe failed';
        btn.disabled = false;
      }
    } catch (err) {
      console.error('Push subscribe error:', err);
      btn.textContent = 'Error — see console';
      btn.disabled = false;
    }
  }

  // ── Init ──────────────────────────────────────────────────────────────────

  function init() {
    fetchStatus();
    setInterval(fetchStatus, 60_000);

    registerServiceWorker().catch(() => {});

    const btn = document.getElementById('push-btn');
    if (btn) btn.addEventListener('click', subscribePush);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();

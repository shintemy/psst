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

  function windowDisplayName(name) {
    const map = {
      'monthly_requests': 'Monthly Requests',
      'weekly_requests': 'Weekly Budget (est.)',
      'daily_requests': 'Daily Budget (est.)',
      'daily_tokens': 'Daily Tokens',
      'five_hour': '5-Hour Window',
      'seven_day': '7-Day Window',
    };
    return map[name] || name;
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
          <span class="window-name">${escHtml(windowDisplayName(name))}</span>
          ${detail ? `<span class="window-detail">${escHtml(detail)}</span>` : ''}
        </div>
        ${renderBar(pct)}
        ${resets ? `<div class="window-resets">${escHtml(resets)}</div>` : ''}
      </div>`;
  }

  function renderProvider(id, p) {
    const windows = Object.entries(p.windows || {});
    const errorHtml = p.last_error
      ? `<div class="provider-error">⚠️ ${escHtml(p.last_error)}</div>`
      : '';
    return `
      <div class="provider-card">
        <h3 class="provider-name">${escHtml(id)}</h3>
        ${errorHtml}
        ${windows.length === 0 && !p.last_error ? '<p class="empty-msg">No data yet</p>' : ''}
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

  // ── Settings Panel ────────────────────────────────────────────────────────

  let settingsOpen = false;

  function toggleSettings() {
    settingsOpen = !settingsOpen;
    const panel = document.getElementById('settings-panel');
    const btn = document.getElementById('settings-btn');
    if (settingsOpen) {
      panel.style.display = 'block';
      btn.textContent = 'Hide Settings';
      fetchConfig();
    } else {
      panel.style.display = 'none';
      btn.textContent = 'Settings';
    }
  }

  async function fetchConfig() {
    const panel = document.getElementById('settings-panel');
    try {
      const res = await fetch(apiUrl('/api/config'));
      if (!res.ok) { panel.innerHTML = '<p class="error-msg">Failed to load config</p>'; return; }
      const data = await res.json();
      renderSettings(data);
    } catch (err) {
      panel.innerHTML = '<p class="error-msg">Failed to load config</p>';
    }
  }

  function renderSettings(data) {
    const panel = document.getElementById('settings-panel');
    const providers = Object.entries(data.providers || {});

    let html = '<div class="settings-form">';
    html += '<h3 class="settings-title">Provider Limits</h3>';
    html += '<p class="settings-hint">Set your monthly request budget for each tool. Most platforms don\'t publish exact limits — set a number that matches your plan.</p>';

    if (providers.length === 0) {
      html += '<p class="empty-msg">No providers configured.</p>';
      html += '<div class="settings-add-section">';
      html += '<p class="settings-hint">Add a tool to monitor:</p>';
      html += renderAddProvider();
      html += '</div>';
    } else {
      for (const [id, pc] of providers) {
        html += `
          <div class="settings-row" data-provider="${escHtml(id)}">
            <label class="settings-label">${escHtml(id)}</label>
            <div class="settings-fields">
              <div class="settings-field">
                <span class="field-label">Monthly requests</span>
                <input type="number" class="settings-input" data-key="monthly_fast_requests"
                  value="${pc.monthly_fast_requests || ''}" placeholder="e.g. 1000" min="0">
              </div>
              <div class="settings-field">
                <span class="field-label">Billing day</span>
                <input type="number" class="settings-input" data-key="billing_day"
                  value="${pc.billing_day || 1}" placeholder="1" min="1" max="28">
              </div>
            </div>
          </div>`;
      }
      html += '<div class="settings-add-section">';
      html += renderAddProvider();
      html += '</div>';
    }

    html += '<div class="settings-actions">';
    html += '<button id="save-settings-btn" class="btn-save">Save</button>';
    html += '<span id="save-status" class="save-status"></span>';
    html += '</div>';
    html += '</div>';

    panel.innerHTML = html;

    document.getElementById('save-settings-btn').addEventListener('click', saveSettings);
    const addBtn = document.getElementById('add-provider-btn');
    if (addBtn) addBtn.addEventListener('click', addProvider);
  }

  function renderAddProvider() {
    return `
      <div class="settings-add-row">
        <input type="text" id="new-provider-name" class="settings-input" placeholder="Tool name (e.g. windsurf)">
        <button id="add-provider-btn" class="btn-add">+ Add</button>
      </div>`;
  }

  function addProvider() {
    const nameInput = document.getElementById('new-provider-name');
    const name = (nameInput.value || '').trim().toLowerCase();
    if (!name) return;

    // Check if already exists
    if (document.querySelector(`.settings-row[data-provider="${name}"]`)) {
      nameInput.style.borderColor = '#ef4444';
      return;
    }

    const addSection = document.querySelector('.settings-add-section');
    const newRow = document.createElement('div');
    newRow.className = 'settings-row';
    newRow.dataset.provider = name;
    newRow.innerHTML = `
      <label class="settings-label">${escHtml(name)}</label>
      <div class="settings-fields">
        <div class="settings-field">
          <span class="field-label">Monthly requests</span>
          <input type="number" class="settings-input" data-key="monthly_fast_requests"
            value="500" placeholder="e.g. 1000" min="0">
        </div>
        <div class="settings-field">
          <span class="field-label">Billing day</span>
          <input type="number" class="settings-input" data-key="billing_day"
            value="1" placeholder="1" min="1" max="28">
        </div>
      </div>`;

    addSection.parentNode.insertBefore(newRow, addSection);
    nameInput.value = '';
  }

  async function saveSettings() {
    const btn = document.getElementById('save-settings-btn');
    const status = document.getElementById('save-status');
    btn.disabled = true;
    status.textContent = 'Saving…';
    status.className = 'save-status';

    const providers = {};
    document.querySelectorAll('.settings-row').forEach(row => {
      const id = row.dataset.provider;
      const monthly = row.querySelector('[data-key="monthly_fast_requests"]');
      const billing = row.querySelector('[data-key="billing_day"]');
      providers[id] = {
        monthly_fast_requests: monthly.value ? parseInt(monthly.value, 10) : null,
        billing_day: billing.value ? parseInt(billing.value, 10) : 1,
        daily_token_limit: null,
      };
    });

    try {
      const res = await fetch(apiUrl('/api/config'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ providers }),
      });

      if (res.ok) {
        status.textContent = 'Saved! Changes take effect on next check cycle.';
        status.className = 'save-status save-ok';
      } else {
        const err = await res.json().catch(() => ({}));
        status.textContent = err.error || 'Save failed';
        status.className = 'save-status save-err';
      }
    } catch (err) {
      status.textContent = 'Network error';
      status.className = 'save-status save-err';
    }

    btn.disabled = false;
  }

  // ── Push Notifications ────────────────────────────────────────────────────

  async function registerServiceWorker() {
    if (!('serviceWorker' in navigator)) return null;
    return navigator.serviceWorker.register('/sw.js');
  }

  // Convert a base64url string to a Uint8Array (for applicationServerKey).
  function urlBase64ToUint8Array(base64String) {
    const padding = '='.repeat((4 - (base64String.length % 4)) % 4);
    const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/');
    const raw = atob(base64);
    const arr = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
    return arr;
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

      // Fetch the VAPID public key from the server.
      const keyRes = await fetch('/api/vapid-public-key');
      if (!keyRes.ok) {
        btn.textContent = 'VAPID key unavailable';
        btn.disabled = false;
        return;
      }
      const { publicKey } = await keyRes.json();
      const applicationServerKey = urlBase64ToUint8Array(publicKey);

      const subscription = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey,
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

  // Check if we already have an active push subscription and update button state.
  async function checkPushState() {
    const btn = document.getElementById('push-btn');
    if (!btn || !('serviceWorker' in navigator) || !('PushManager' in window)) return;

    try {
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) {
        btn.textContent = 'Notifications enabled';
        btn.classList.add('btn-success');
        btn.disabled = true;
      }
    } catch (_) { /* ignore */ }
  }

  function init() {
    fetchStatus();
    setInterval(fetchStatus, 60_000);

    registerServiceWorker().then(() => checkPushState()).catch(() => {});

    const pushBtn = document.getElementById('push-btn');
    if (pushBtn) pushBtn.addEventListener('click', subscribePush);

    const settingsBtn = document.getElementById('settings-btn');
    if (settingsBtn) settingsBtn.addEventListener('click', toggleSettings);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();

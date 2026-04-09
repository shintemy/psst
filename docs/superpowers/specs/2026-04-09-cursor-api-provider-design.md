# Cursor API Provider Design

## Problem

Psst's `CursorLocalProvider` counts distinct `requestId` values from Cursor's local SQLite DB (`~/.cursor/ai-tracking/ai-code-tracking.db`). This DB only records composer-generated code hashes, missing chat, tab completion, and other request types. Cursor uses weighted billing ("1% Auto + 21% API"), not simple request counts. Result: Psst shows 4% while Cursor shows 7%.

## Solution

Replace `CursorLocalProvider` with `CursorApiProvider` that calls Cursor's internal gRPC-over-HTTP endpoint for exact usage data. Fall back to `CursorLocalProvider` when API auth is unavailable.

## API Details

### Authentication

Cursor stores JWT credentials in macOS SQLite DB:
- **Path**: `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`
- **Table**: `ItemTable` (key-value)
- **Keys**: `cursorAuth/accessToken` (JWT), `cursorAuth/refreshToken` (JWT)

JWT `exp` field determines expiry. Refresh via:

```
POST https://api2.cursor.sh/oauth/token
Content-Type: application/json

{
  "grant_type": "refresh_token",
  "client_id": "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB",
  "refresh_token": "<refreshToken>"
}

Response: { "access_token": "...", "id_token": "...", "shouldLogout": false }
```

If `shouldLogout` is true, tokens are fully invalidated — fall back to SQLite provider.

### Usage Endpoint

```
POST https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage
Authorization: Bearer <accessToken>
Content-Type: application/json

{}
```

Response:
```json
{
  "billingCycleStart": "1773133068000",
  "billingCycleEnd": "1775811468000",
  "planUsage": {
    "totalSpend": 10455,
    "includedSpend": 10455,
    "remaining": 29545,
    "limit": 40000,
    "autoPercentUsed": 0.175,
    "apiPercentUsed": 20.56,
    "totalPercentUsed": 6.97
  },
  "displayMessage": "You've used 26% of your included usage"
}
```

## Architecture

### New Module: `src/data_sources/cursor_api.rs`

`CursorApiProvider` implements `QuotaProvider` and produces 3 windows:

| Window Name | Source Field | Description |
|---|---|---|
| `monthly_requests` | `totalPercentUsed / 100` | Total usage (matches Cursor's "Total: 7%") |
| `auto_requests` | `autoPercentUsed / 100` | Auto model bucket usage |
| `api_requests` | `apiPercentUsed / 100` | Named/API model usage |

All windows share the same `resets_at` derived from `billingCycleEnd` (millisecond timestamp).

`used_count` is set to `None` for all windows (API returns spend amounts, not request counts). All downstream consumers (frontend, notifications, thresholds) handle `None` gracefully — verified in code review.

### Token Management

1. Read `accessToken` from `state.vscdb`
2. Decode JWT payload (base64, no signature verification) to check `exp`
3. If `exp < now + 5min`, refresh using `refreshToken`
4. On refresh success: use new `access_token` (do NOT write back to state.vscdb — that's Cursor IDE's responsibility)
5. On refresh failure (`shouldLogout: true`, network error, etc.): return error, scheduler falls back

### Fallback Strategy

In `scheduler.rs` `build_providers()`:

```
cursor → try CursorApiProvider::new()
           → if state.vscdb not found or unreadable → CursorLocalProvider (SQLite)
```

At runtime in `fetch_quota()`:
```
CursorApiProvider::fetch_quota()
  → token expired? → refresh
    → refresh failed? → return Err (scheduler records last_error, state keeps previous data)
  → API call failed? → return Err (same)
```

The scheduler already handles provider errors gracefully: it records `last_error` on the provider state and preserves the previous window data until the next successful fetch.

### Stale Window Handling

CursorApiProvider produces: `monthly_requests`, `auto_requests`, `api_requests`
CursorLocalProvider produces: `monthly_requests`, `weekly_requests`, `daily_requests`

These are different window name sets. The stale window cleanup (added in scheduler.rs) removes windows not in the current provider's report. When switching providers, the old provider's windows get cleaned up, and the new provider's windows are created fresh. Alert state (`alerts_sent`) on the old windows is intentionally discarded — correct behavior since the utilization source changed.

## Changes Required

### New Files
- `src/data_sources/cursor_api.rs`

### Modified Files
- `src/data_sources/mod.rs` — add `pub mod cursor_api;`
- `src/scheduler.rs` — update `build_providers()` cursor branch
- `src/web/static/app.js` — add `auto_requests` and `api_requests` to `WINDOW_META` and `WINDOW_ORDER`
- `src/notifiers/mod.rs` — add Chinese display names for new window types
- `Cargo.toml` — add `base64` crate (for JWT payload decoding); `serde_json` already present; `reqwest` already present

### Unchanged
- `config.toml` — no new fields needed. `monthly_fast_requests` and `billing_day` are still used by fallback `CursorLocalProvider`
- `src/data_sources/cursor_local.rs` — kept as-is for fallback
- `src/threshold.rs` — only uses `utilization` and `resets_at`, unaffected
- `src/state.rs` — `QuotaWindowState` already supports all needed fields

## Platform Note

Token path uses `~/Library/Application Support/Cursor/` which is macOS-specific. On Linux it would be `~/.config/Cursor/`. The provider constructor should accept a configurable path or detect the platform.

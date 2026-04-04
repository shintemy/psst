<p align="center">
  <img src="assets/psst_banner.png" alt="Psst — Every token counts, it counts for you." width="100%" />
</p>

<h1 align="center">Psst</h1>

<p align="center">
  <em>Every token counts, it counts for you.</em>
</p>

<p align="center">
  <strong>Runs entirely on your machine. No data leaves your computer.</strong>
</p>

<p align="center">
  <strong>English</strong> | <a href="README.zh-CN.md">中文</a>
</p>

---

## What is Psst?

Psst is a local background service that monitors your AI coding tools' usage quotas. When your quota is running low or about to reset, it sends you alerts through multiple channels — so you never waste your allowance or get caught off guard.

## Features

- **Auto-discovery** — Automatically detects AI coding tools installed on your machine
- **16+ tools supported** — Claude Code, Cursor, Copilot, Codex CLI, Gemini CLI, Windsurf, Amp, and more
- **4 notification channels** — macOS desktop, Telegram, ServerChan (WeChat), PWA Web Push (mobile)
- **Smart alerts** — Warns at 50%/80% usage; countdowns at 24h/12h/1h before quota reset
- **Web dashboard** — View usage and edit settings from your browser
- **Fully local** — All usage data is read from local files (only notifications go to external services)

## Supported Tools

| Tool | Data Source |
|------|------------|
| Claude Code | Local JSONL via tokscale-core |
| Cursor | Local SQLite (`~/.cursor/ai-tracking/`) |
| GitHub Copilot | Local logs via tokscale-core |
| Codex CLI | Local JSONL via tokscale-core |
| Gemini CLI | Local JSON via tokscale-core |
| Windsurf, Amp, RooCode, KiloCode, Droid, OpenClaw, Pi, Kimi, Qwen, Mux, Kilo, Crush | Local logs via tokscale-core |

---

## Prerequisites

Before installing Psst, you need the Rust toolchain and OpenSSL on your system.

### Install Rust (if not already installed)

Open your terminal and run:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the on-screen instructions and choose the default installation. After it finishes, restart your terminal or run:

```bash
source $HOME/.cargo/env
```

Verify the installation:

```bash
rustc --version
cargo --version
```

You should see version numbers printed (e.g., `rustc 1.XX.0`).

### Install OpenSSL (macOS)

OpenSSL is needed to generate VAPID keys for web push notifications. On macOS, install it via Homebrew:

```bash
# Install Homebrew if you don't have it:
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Then install OpenSSL:
brew install openssl
```

---

## Installation

### Step 1: Clone the repository

```bash
git clone --recursive https://github.com/shintemy/psst.git
```

> **Important:** The `--recursive` flag downloads the [tokscale-core](https://github.com/junhoyeo/tokscale) submodule, which Psst uses to parse local usage data from AI tools. You do NOT need to install tokscale separately — it is compiled into Psst automatically during the build step.

### Step 2: Enter the project directory

```bash
cd psst
```

### Step 3: Build and install

```bash
cargo install --path .
```

This compiles Psst and installs the `psst` binary to `~/.cargo/bin/`. Make sure this directory is in your PATH:

```bash
# Add to your shell profile (~/.zshrc or ~/.bashrc) if not already present:
export PATH="$HOME/.cargo/bin:$PATH"
```

### Step 4: Verify the installation

```bash
psst --help
```

You should see:

```
AI coding tool usage monitor & notifier

Usage: psst <COMMAND>

Commands:
  init          Create config + state files in ~/.config/psst/
  run           Start the monitoring daemon (foreground)
  status        Print current usage status
  test-notify   Send a test notification to all enabled channels
  install       Install macOS LaunchAgent
  uninstall     Remove macOS LaunchAgent
  help          Print this message or the help of the given subcommand(s)
```

---

## Quick Start

### Step 1: Initialize configuration

```bash
psst init
```

This will:
1. Scan your machine for installed AI coding tools
2. Create a configuration file at `~/.config/psst/config.toml` with default quota limits for each detected tool
3. Generate a random access token for the web dashboard
4. Generate VAPID keys for web push notifications

Example output:

```
Scanning for AI coding tools...
  Found: claude, cursor
Created config: /Users/you/.config/psst/config.toml

  → Edit this file to adjust quota limits for your plan.

State file: /Users/you/.config/psst/state.json

Access token: a1b2c3d4-e5f6-7890-abcd-ef1234567890
Use this token to authenticate with the web UI.

Generating VAPID keys...
VAPID keys generated.

Done! Run `psst run` to start monitoring.
```

### Step 2: Edit your quota limits (optional but recommended)

Open the config file in your preferred text editor:

```bash
# Using vim:
vim ~/.config/psst/config.toml

# Or using VS Code:
code ~/.config/psst/config.toml

# Or using nano (beginner-friendly):
nano ~/.config/psst/config.toml
```

Find the `[providers]` section and adjust the limits to match your subscription plan:

```toml
[providers.claude]
# How many requests you estimate per month for your plan:
#   Pro $20/mo  → ~1000
#   Max $100/mo → ~5000
#   Max $200/mo → ~20000
monthly_fast_requests = 5000
billing_day = 1    # Day of month when your billing cycle resets (1-28)

[providers.cursor]
# How many fast requests per month for your plan:
#   Pro $20/mo   → ~500
#   Pro+ $60/mo  → ~1500
#   Ultra $200/mo → ~10000
monthly_fast_requests = 500
billing_day = 1
```

> **Tip:** Most AI tools don't publish exact quota numbers. Set a number that feels right for your plan — you can always adjust it later from the web dashboard.

Save and close the file.

### Step 3: Start the monitoring service

```bash
psst run
```

You will see output like:

```
INFO psst: Dashboard: http://127.0.0.1:3377?token=a1b2c3d4-e5f6-7890-abcd-ef1234567890

  🔗 Dashboard: http://127.0.0.1:3377?token=a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

Open this URL in your browser to see the dashboard.

The service will check your usage every 20 minutes (configurable) and send alerts when thresholds are crossed.

**To stop the service:** Press `Ctrl + C` in the terminal.

### Step 4: Set up auto-start on login (macOS only)

If you want Psst to run automatically every time you log in:

```bash
psst install
```

This creates a macOS LaunchAgent that:
- Starts Psst automatically when you log in
- Restarts it automatically if it crashes
- Runs in the background (no terminal window needed)

To verify it's running:

```bash
launchctl list | grep psst
```

To remove auto-start later:

```bash
psst uninstall
```

---

## Commands Reference

### `psst init` — Initialize configuration

Creates the config directory and files. Safe to run multiple times — it won't overwrite existing config.

```bash
psst init
```

### `psst run` — Start the monitoring service

Runs the service in the foreground. It will:
- Check usage for all configured tools every 20 minutes
- Send notifications when thresholds are crossed
- Serve the web dashboard

```bash
psst run
```

### `psst status` — Check current usage

Prints a snapshot of all tool usage without starting the service.

```bash
psst status
```

Example output:

```
=== Psst Status ===
Last check: 2026-04-04 10:40:00 UTC
Discovered tools: claude, cursor

Provider: claude
  monthly_requests: 42% used (420 requests) — resets at 2026-05-01T00:00:00Z

Provider: cursor
  monthly_requests: 15% used (75 requests) — resets at 2026-05-01T00:00:00Z
```

### `psst test-notify` — Send a test notification

Sends a test message to all enabled notification channels. Use this to verify your notification setup is working.

```bash
psst test-notify
```

### `psst install` — Enable auto-start (macOS)

```bash
psst install
```

### `psst uninstall` — Disable auto-start (macOS)

```bash
psst uninstall
```

---

## Notification Channels

Psst supports 4 notification channels. You can enable any combination of them.

All notification settings are in `~/.config/psst/config.toml`.

### 1. macOS Desktop Notifications

**Enabled by default.** No setup needed.

Shows native macOS notification banners when alerts trigger.

```toml
[notifications]
desktop = true    # Set to false to disable
```

### 2. Telegram

Send alerts to your phone via a Telegram bot.

#### Step 1: Create a Telegram Bot

1. Open Telegram on your phone or desktop
2. Search for **@BotFather** and start a chat
3. Send the command: `/newbot`
4. BotFather will ask you to choose a name and username for your bot
5. After creation, BotFather gives you a **bot token** like:
   ```
   123456789:ABCdefGHIjklMNOpqrsTUVwxyz
   ```
   Save this token.

#### Step 2: Get your Chat ID

1. Open a chat with your newly created bot in Telegram
2. Send any message to the bot (e.g., type "hello" and press send)
3. Open this URL in your browser (replace `YOUR_BOT_TOKEN` with your actual token):
   ```
   https://api.telegram.org/botYOUR_BOT_TOKEN/getUpdates
   ```
4. In the JSON response, find the `"chat"` object and note the `"id"` number. For example:
   ```json
   "chat": {
     "id": 1234567890,
     "first_name": "Your Name",
     "type": "private"
   }
   ```
   In this example, the chat ID is `1234567890`.

#### Step 3: Add to config

Open your config file:

```bash
nano ~/.config/psst/config.toml
```

Find the `[notifications.telegram]` section and update it:

```toml
[notifications.telegram]
enabled = true
bot_token = "YOUR_BOT_TOKEN"
chat_id = "YOUR_CHAT_ID"
```

#### Step 4: Test it

```bash
psst test-notify
```

You should receive a test message from your bot in Telegram.

### 3. ServerChan (Server酱 — WeChat)

Send alerts to your WeChat via the ServerChan service.

#### Step 1: Get a SendKey

1. Visit [sct.ftqq.com](https://sct.ftqq.com/)
2. Log in with your GitHub account
3. Go to "SendKey" page and copy your key

#### Step 2: Add to config

```toml
[notifications.serverchan]
enabled = true
send_key = "YOUR_SEND_KEY_HERE"
```

#### Step 3: Test it

```bash
psst test-notify
```

### 4. PWA Web Push (Mobile Push Notifications)

Send push notifications to your phone — works on iOS (16.4+) and Android, no app install required.

**Enabled by default.**

```toml
[notifications.web_push]
enabled = true
```

#### Setup on iPhone

1. Make sure your phone and computer are on the **same Wi-Fi network**
2. Find your computer's local IP address:
   ```bash
   # On macOS:
   ipconfig getifaddr en0
   ```
   This will print something like `192.168.1.100`.

3. Change Psst's bind address to allow network access. Edit `~/.config/psst/config.toml`:
   ```toml
   [server]
   bind = "0.0.0.0:3377"    # Listen on all interfaces (was 127.0.0.1)
   ```

4. Restart Psst if it's running (`Ctrl+C`, then `psst run`)

5. On your iPhone, open **Safari** and go to:
   ```
   http://192.168.1.100:3377?token=YOUR_ACCESS_TOKEN
   ```
   (Replace the IP and token with your actual values. The token was shown when you ran `psst init`.)

6. Tap the **Share** button (square with arrow) → **Add to Home Screen** → **Add**

7. Open the app from your Home Screen

8. Tap the **"Enable Push Notifications"** button and allow notifications when prompted

9. Done! Your phone will now receive push notifications even when you're away from home.

#### Setup on Android

Same steps, but use Chrome instead of Safari. Chrome supports PWA push notifications natively.

> **Note:** After the initial setup, your phone does NOT need to be on the same network as your computer. Notifications are delivered through Apple/Google's push services.

---

## Alert Thresholds

Psst uses two types of alerts:

### Usage Alerts

Triggered when your usage reaches a certain percentage of your configured limit.

### Reset Countdown Alerts

Triggered before your quota resets, reminding you to use remaining quota.

### Configuration

```toml
[thresholds]
# Alert when usage reaches these percentages (0-100)
usage_alerts = [50, 80]

# Alert this many hours before quota reset
reset_alerts_hours = [24, 12, 1]

# If usage is above 95%, skip "use your remaining quota" reminders
# (because there's almost nothing left to use)
skip_reset_alert_above = 0.95
```

### How deduplication works

- Each alert fires **only once per billing cycle** for the same threshold
- When a billing cycle resets, all alert records are cleared automatically
- Example: if you get a "50% used" alert, you won't get it again until next month

---

## Web Dashboard

The dashboard is available at `http://127.0.0.1:3377` when `psst run` is active.

### What you can do:

- **View real-time usage** for all monitored tools
- **See error messages** if a data source fails
- **Edit provider limits** via the Settings panel (no need to edit config.toml manually)
- **Enable push notifications** for your browser/phone

### Access from another device on your network

1. Change the bind address in config:
   ```toml
   [server]
   bind = "0.0.0.0:3377"
   ```

2. Access via your computer's local IP:
   ```
   http://192.168.x.x:3377?token=YOUR_TOKEN
   ```

---

## File Layout

After initialization, Psst creates the following files:

```
~/.config/psst/
├── config.toml          # Your configuration (edit this)
├── state.json           # Runtime state (managed automatically)
├── vapid_private.pem    # VAPID private key (for web push)
├── vapid_public.pem     # VAPID public key (for web push)
├── psst.log             # Stdout log (LaunchAgent mode)
└── psst.err             # Stderr log (LaunchAgent mode)
```

---

## Tech Stack

| Component | Choice |
|-----------|--------|
| Language | Rust |
| Async runtime | tokio |
| Web server | axum (port 3377) |
| Desktop notifications | notify-rust |
| Mobile push | web-push crate (VAPID / AES-128-GCM) |
| Local data parsing | tokscale-core |
| Configuration | TOML |
| State storage | JSON (atomic writes) |

---

## Troubleshooting

### Build fails with `tokscale-core` not found

You probably cloned without the `--recursive` flag. Run this to fix it:

```bash
git submodule update --init --recursive
```

Then retry the build:

```bash
cargo install --path .
```

### `psst: command not found`

Make sure `~/.cargo/bin` is in your PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Add this line to your `~/.zshrc` (or `~/.bashrc`) to make it permanent.

### `No provider data yet` on the dashboard

This is normal on first start. Wait for the first check cycle (up to 20 minutes) or restart the service. Make sure you have at least one provider configured with a `monthly_fast_requests` limit in config.toml.

### Telegram notifications not arriving

1. Make sure you sent a message to your bot first (the bot can't initiate a chat)
2. Double-check `bot_token` and `chat_id` in config.toml
3. Run `psst test-notify` and check the terminal output for error messages

### Web Push not working on iPhone

1. iOS 16.4 or later is required
2. You must use **Safari** to add the page to your Home Screen
3. You must open the app **from the Home Screen icon** (not from Safari)
4. Make sure you tapped "Allow" when the notification permission prompt appeared

---

## Acknowledgements

Psst is built on top of [tokscale](https://github.com/junhoyeo/tokscale) by [@junhoyeo](https://github.com/junhoyeo). Tokscale provides the local data parsing engine that makes it possible to read usage data from 16+ AI coding tools without any remote API calls. Thank you for the excellent work!

## License

MIT

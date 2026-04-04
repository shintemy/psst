<p align="center">
  <img src="assets/psst_banner.png" alt="Psst — Every token counts, it counts for you." width="100%" />
</p>

<h1 align="center">Psst</h1>

<p align="center">
  <em>嘘，悄悄提醒你一下 —— 你的 AI 额度快用完了。</em>
</p>

<p align="center">
  <strong>纯本地运行，不上传任何数据。</strong>
</p>

<p align="center">
  <a href="README.md">English</a> | <strong>中文</strong>
</p>

---

## Psst 是什么？

Psst 是一个本地常驻的 AI 编码工具用量管家。它监控你机器上所有 AI 编码工具的使用情况，在额度快用完或即将重置时，通过多个渠道推送提醒，帮你避免额度浪费。

## 功能特性

- **自动发现** — 启动时自动扫描本机安装的 AI 编码工具，无需手动配置
- **支持 16+ 种工具** — Claude Code、Cursor、Copilot、Codex CLI、Gemini CLI、Windsurf、Amp 等
- **4 个通知渠道** — macOS 桌面通知、Telegram、Server酱（微信）、PWA Web Push（手机）
- **智能提醒** — 用量达 50%/80% 时提醒，重置前 24h/12h/1h 倒计时提醒
- **Web 仪表盘** — 在浏览器中查看用量，在线调整配置
- **零远程调用** — 所有用量数据从本地文件读取（仅通知发送会访问外部服务）

## 支持的工具

| 工具 | 数据来源 |
|------|---------|
| Claude Code | tokscale-core 解析本地 JSONL |
| Cursor | tokscale-core 解析本地日志 |
| GitHub Copilot | tokscale-core 解析本地日志 |
| Codex CLI | tokscale-core 解析本地 JSONL |
| Gemini CLI | tokscale-core 解析本地 JSON |
| Windsurf, Amp, RooCode, KiloCode, Droid, OpenClaw, Pi, Kimi, Qwen, Mux, Kilo, Crush | tokscale-core |

---

## 环境准备

在安装 Psst 之前，你需要先安装 Rust 工具链和 OpenSSL。

### 安装 Rust

打开终端（Terminal），运行以下命令：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

按照屏幕提示操作，选择默认安装即可。安装完成后，重启终端或运行：

```bash
source $HOME/.cargo/env
```

验证安装是否成功：

```bash
rustc --version
cargo --version
```

如果能看到版本号（如 `rustc 1.XX.0`），说明安装成功。

### 安装 OpenSSL（macOS）

OpenSSL 用于生成 Web Push 所需的 VAPID 密钥。在 macOS 上通过 Homebrew 安装：

```bash
# 如果还没安装 Homebrew，先安装它：
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 然后安装 OpenSSL：
brew install openssl
```

---

## 安装 Psst

### 第一步：克隆仓库

```bash
git clone --recursive https://github.com/shintemy/psst.git
```

> **重要：** `--recursive` 参数会同时下载 [tokscale-core](https://github.com/junhoyeo/tokscale) 子模块，Psst 用它来解析各 AI 工具的本地用量数据。你**不需要**单独安装 tokscale——它会在编译时自动打包进 Psst。

### 第二步：进入项目目录

```bash
cd psst
```

### 第三步：编译并安装

```bash
cargo install --path .
```

编译过程可能需要几分钟。完成后，`psst` 命令会被安装到 `~/.cargo/bin/` 目录。

确保这个目录在你的 PATH 中：

```bash
# 将以下内容添加到 ~/.zshrc 或 ~/.bashrc（如果还没有的话）：
export PATH="$HOME/.cargo/bin:$PATH"
```

添加后，运行 `source ~/.zshrc`（或 `source ~/.bashrc`）使其生效。

### 第四步：验证安装

```bash
psst --help
```

如果看到以下输出，说明安装成功：

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

## 快速开始

### 第一步：初始化配置

```bash
psst init
```

这个命令会自动完成以下操作：
1. 扫描你电脑上安装了哪些 AI 编码工具
2. 在 `~/.config/psst/` 目录下生成配置文件，并为检测到的工具预填默认限额
3. 生成一个随机的访问 token（用于 Web 仪表盘的身份验证）
4. 生成 Web Push 所需的 VAPID 密钥对

示例输出：

```
Scanning for AI coding tools...
  Found: claude, cursor
Created config: /Users/你的用户名/.config/psst/config.toml

  → Edit this file to adjust quota limits for your plan.

State file: /Users/你的用户名/.config/psst/state.json

Access token: a1b2c3d4-e5f6-7890-abcd-ef1234567890
Use this token to authenticate with the web UI.

Generating VAPID keys...
VAPID keys generated.

Done! Run `psst run` to start monitoring.
```

### 第二步：编辑配额限制（可选但推荐）

用你喜欢的编辑器打开配置文件：

```bash
# 使用 nano（最适合新手）：
nano ~/.config/psst/config.toml

# 或使用 vim：
vim ~/.config/psst/config.toml

# 或使用 VS Code：
code ~/.config/psst/config.toml
```

找到 `[providers]` 部分，根据你的订阅计划调整月度请求预算：

```toml
[providers.claude]
# Claude Code 各计划的参考值：
#   Pro $20/月  → 约 1000 次请求
#   Max $100/月 → 约 5000 次请求
#   Max $200/月 → 约 20000 次请求
monthly_fast_requests = 5000
billing_day = 1    # 每月几号重置计费周期（1-28）

[providers.cursor]
# Cursor 各计划的参考值：
#   Pro $20/月   → 约 500 次请求
#   Pro+ $60/月  → 约 1500 次请求
#   Ultra $200/月 → 约 10000 次请求
monthly_fast_requests = 500
billing_day = 1
```

> **提示：** 大多数 AI 工具不会公布精确的配额数字。设一个你觉得合理的数字就行——之后可以随时在 Web 仪表盘上调整。

编辑完成后保存并关闭文件（nano 中按 `Ctrl+O` 保存，`Ctrl+X` 退出）。

### 第三步：启动监控服务

```bash
psst run
```

启动后会看到类似输出：

```
INFO psst: Dashboard: http://127.0.0.1:3377?token=a1b2c3d4-e5f6-7890-abcd-ef1234567890

  🔗 Dashboard: http://127.0.0.1:3377?token=a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

在浏览器中打开这个链接，即可查看用量仪表盘。

服务会每 20 分钟检查一次用量（可在配置中修改），当用量触发阈值时自动发送通知。

**停止服务：** 在终端中按 `Ctrl + C`。

### 第四步：设为开机自启（仅 macOS）

如果你希望每次开机后 Psst 自动运行：

```bash
psst install
```

这会创建一个 macOS LaunchAgent，实现：
- 登录时自动启动 Psst
- 崩溃时自动重启
- 在后台运行（无需打开终端窗口）

验证是否在运行：

```bash
launchctl list | grep psst
```

以后想取消开机自启：

```bash
psst uninstall
```

---

## 命令详解

### `psst init` — 初始化配置

创建配置目录和文件。可以多次运行，不会覆盖已有配置。

```bash
psst init
```

### `psst run` — 启动监控服务

在前台运行服务。它会：
- 每 20 分钟检查所有已配置工具的用量
- 用量触发阈值时发送通知
- 同时启动 Web 仪表盘

```bash
psst run
```

### `psst status` — 查看当前用量

打印所有工具的用量快照，不需要启动服务。

```bash
psst status
```

示例输出：

```
=== Psst Status ===
Last check: 2026-04-04 10:40:00 UTC
Discovered tools: claude, cursor

Provider: claude
  monthly_requests: 42% used (420 requests) — resets at 2026-05-01T00:00:00Z

Provider: cursor
  monthly_requests: 15% used (75 requests) — resets at 2026-05-01T00:00:00Z
```

### `psst test-notify` — 发送测试通知

向所有已启用的通知渠道发送一条测试消息。用来验证通知配置是否正确。

```bash
psst test-notify
```

### `psst install` — 开机自启（macOS）

```bash
psst install
```

### `psst uninstall` — 取消开机自启（macOS）

```bash
psst uninstall
```

---

## 通知渠道配置

Psst 支持 4 个通知渠道，你可以启用其中任意组合。

所有通知设置都在 `~/.config/psst/config.toml` 中。

### 1. macOS 桌面通知

**默认已启用，无需任何配置。**

当触发告警时，会显示 macOS 原生通知横幅。

```toml
[notifications]
desktop = true    # 设为 false 可禁用
```

### 2. Telegram

通过 Telegram Bot 将告警发送到你的手机。

#### 第一步：创建 Telegram Bot

1. 打开 Telegram（手机或电脑端均可）
2. 搜索 **@BotFather** 并开始对话
3. 发送命令：`/newbot`
4. BotFather 会要求你为 Bot 起一个名字和用户名
5. 创建成功后，BotFather 会给你一个 **Bot Token**，格式类似：
   ```
   123456789:ABCdefGHIjklMNOpqrsTUVwxyz
   ```
   请保存好这个 token。

#### 第二步：获取你的 Chat ID

1. 在 Telegram 中打开你刚创建的 Bot 的对话
2. 给 Bot **发送一条消息**（比如输入 "hello" 然后发送）— 这一步必须做，否则无法获取 Chat ID
3. 在浏览器中打开以下链接（把 `YOUR_BOT_TOKEN` 替换成你的实际 token）：
   ```
   https://api.telegram.org/botYOUR_BOT_TOKEN/getUpdates
   ```
4. 在返回的 JSON 中，找到 `"chat"` 对象里的 `"id"` 数字。例如：
   ```json
   "chat": {
     "id": 1234567890,
     "first_name": "你的名字",
     "type": "private"
   }
   ```
   这里的 `1234567890` 就是你的 Chat ID。

#### 第三步：写入配置

打开配置文件：

```bash
nano ~/.config/psst/config.toml
```

找到 `[notifications.telegram]` 部分，修改为：

```toml
[notifications.telegram]
enabled = true
bot_token = "你的Bot Token"
chat_id = "你的Chat ID"
```

#### 第四步：测试

```bash
psst test-notify
```

如果配置正确，你会在 Telegram 中收到 Bot 发来的测试消息。

### 3. Server酱（微信推送）

通过 Server酱 服务将告警发送到你的微信。

#### 第一步：获取 SendKey

1. 访问 [sct.ftqq.com](https://sct.ftqq.com/)
2. 使用 GitHub 账号登录
3. 进入"SendKey"页面，复制你的 Key

#### 第二步：写入配置

```toml
[notifications.serverchan]
enabled = true
send_key = "你的SendKey"
```

#### 第三步：测试

```bash
psst test-notify
```

### 4. PWA Web Push（手机推送通知）

通过标准 Web Push 协议将通知推送到你的手机。支持 iOS（16.4+）和 Android，无需安装任何 App。

**默认已启用。**

```toml
[notifications.web_push]
enabled = true
```

#### 在 iPhone 上设置

1. 确保你的手机和电脑连接在**同一个 Wi-Fi 网络**

2. 查看电脑的局域网 IP 地址：
   ```bash
   # macOS 上运行：
   ipconfig getifaddr en0
   ```
   会输出类似 `192.168.1.100` 的地址。

3. 修改 Psst 的监听地址以允许局域网访问。编辑 `~/.config/psst/config.toml`：
   ```toml
   [server]
   bind = "0.0.0.0:3377"    # 监听所有网络接口（原来是 127.0.0.1）
   ```

4. 如果 Psst 正在运行，重启它（`Ctrl+C` 停止，然后重新 `psst run`）

5. 在 iPhone 上打开 **Safari 浏览器**，访问：
   ```
   http://192.168.1.100:3377?token=你的访问Token
   ```
   （将 IP 和 token 替换为你的实际值。token 在运行 `psst init` 时显示过。如果忘了，可以运行 `psst status` 或查看 `~/.config/psst/state.json` 中的 `access_token` 字段。）

6. 点击 Safari 底部的**分享按钮**（方框加箭头的图标）→ 选择**"添加到主屏幕"** → 点击**"添加"**

7. 回到主屏幕，**从主屏幕图标打开**刚添加的 App

8. 点击页面上的 **"Enable Notifications"** 按钮，在弹出的权限请求中点击**"允许"**

9. 设置完成！之后即使不在同一 Wi-Fi 下，手机也能收到推送通知。

#### 在 Android 上设置

步骤相同，但使用 Chrome 浏览器代替 Safari。Chrome 原生支持 PWA 推送通知。

> **说明：** 首次设置需要在同一局域网内完成。之后推送通知通过 Apple/Google 的推送服务送达，手机不需要和电脑在同一网络。

---

## 阈值策略

Psst 有两种告警类型：

### 用量告警

当用量达到配置的百分比时触发。例如，当 Claude 用量达到月度预算的 50% 时发出提醒。

### 重置倒计时告警

在配额即将重置前触发，提醒你使用剩余额度。如果额度已经用了 95% 以上，则不会发送此类提醒（因为已经没什么可用的了）。

### 配置方式

```toml
[thresholds]
# 用量达到以下百分比时发送提醒（0-100）
usage_alerts = [50, 80]

# 在配额重置前多少小时发送提醒
reset_alerts_hours = [24, 12, 1]

# 用量超过此比例时，不再发送"赶紧用剩余额度"的提醒
# （因为已经快用完了，没必要再提醒）
skip_reset_alert_above = 0.95
```

### 去重机制

- 同一阈值在同一计费周期内**只通知一次**
- 计费周期重置后，通知记录自动清除
- 例如：你收到了"已用 50%"的通知后，在这个月内不会再收到同样的通知

---

## Web 仪表盘

当 `psst run` 运行时，仪表盘可通过 `http://127.0.0.1:3377` 访问。

### 主要功能：

- **查看实时用量** — 所有已监控工具的用量百分比和详情
- **查看错误信息** — 如果某个数据源读取失败，会显示错误详情
- **编辑配额限制** — 通过 Settings 面板在网页上直接修改，无需手动编辑 config.toml
- **启用推送通知** — 为当前浏览器/手机启用 Web Push 通知

### 从其他设备访问

1. 将配置中的监听地址改为所有接口：
   ```toml
   [server]
   bind = "0.0.0.0:3377"
   ```

2. 通过电脑的局域网 IP 访问：
   ```
   http://192.168.x.x:3377?token=你的Token
   ```

---

## 文件说明

初始化后，Psst 会创建以下文件：

```
~/.config/psst/
├── config.toml          # 用户配置文件（你需要编辑的文件）
├── state.json           # 运行状态（自动维护，无需手动修改）
├── vapid_private.pem    # VAPID 私钥（Web Push 加密用）
├── vapid_public.pem     # VAPID 公钥（Web Push 加密用）
├── psst.log             # 标准输出日志（LaunchAgent 模式下生成）
└── psst.err             # 错误日志（LaunchAgent 模式下生成）
```

---

## 技术栈

| 组件 | 选择 |
|------|------|
| 语言 | Rust |
| 异步运行时 | tokio |
| Web 服务器 | axum（端口 3377） |
| 桌面通知 | notify-rust |
| 手机推送 | web-push crate（VAPID / AES-128-GCM） |
| 本地数据解析 | tokscale-core |
| 配置格式 | TOML |
| 状态存储 | JSON（原子写入） |

---

## 常见问题

### 编译失败，提示找不到 `tokscale-core`

你可能在克隆时忘记了 `--recursive` 参数。运行以下命令修复：

```bash
git submodule update --init --recursive
```

然后重新编译：

```bash
cargo install --path .
```

### 运行 `psst` 提示 `command not found`

确保 `~/.cargo/bin` 在你的 PATH 中：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

将这行添加到你的 `~/.zshrc`（或 `~/.bashrc`）中以永久生效，然后运行 `source ~/.zshrc`。

### 仪表盘显示"No provider data yet"

这在首次启动时是正常的。等待第一次检查周期完成（最多 20 分钟），或重启服务。确保 config.toml 中至少有一个 provider 配置了 `monthly_fast_requests` 限额。

### Telegram 收不到通知

1. 确保你先给 Bot 发送了一条消息（Bot 无法主动发起对话）
2. 仔细检查 config.toml 中的 `bot_token` 和 `chat_id` 是否正确
3. 运行 `psst test-notify` 并查看终端输出是否有错误信息

### iPhone 上 Web Push 不工作

1. 需要 iOS 16.4 或更高版本
2. 必须使用 **Safari** 添加到主屏幕（其他浏览器不支持）
3. 必须**从主屏幕图标打开**（不是从 Safari 中打开）
4. 确保在弹出的权限请求中点击了"允许"

---

## 致谢

Psst 基于 [@junhoyeo](https://github.com/junhoyeo) 开发的 [tokscale](https://github.com/junhoyeo/tokscale) 构建。tokscale 提供了本地数据解析引擎，使 Psst 能够在不发起任何远程 API 调用的情况下，读取 16+ 种 AI 编码工具的用量数据。感谢这个优秀的开源项目！

## 许可证

MIT

# Psst — AI 编码工具用量管家设计文档

## 概述

Psst 是一个开源的本地常驻服务，监控主流 AI 编码工具的用量配额，在关键节点通过多渠道推送提醒用户，避免额度浪费或超限。

基于 tokscale-core 自动发现并追踪本机安装的所有 AI 编码工具，支持 16+ 种工具。

**名称含义：** "Psst" — 悄悄提醒你一下。

---

## 目标用户

使用 AI 编码工具（Claude Code、Cursor、Codex CLI、Gemini CLI 等）的开发者，希望：
- 随时了解所有 AI 工具的剩余用量
- 在额度快用完时收到提醒
- 在重置窗口前收到提醒，避免额度清零浪费

---

## 支持的工具

### 两层支持体系

**第一层：精确配额（有 API 或已知限制模型）**

| 工具 | 配额来源 | 精确度 |
|---|---|---|
| Claude Code | OAuth API (`/api/oauth/usage`) 实时查询 | 精确（百分比 + 重置时间） |
| Cursor | 用户配置上限 + tokscale 消耗推算 | 推算（依赖用户配置正确） |

后续可扩展：Codex CLI（OpenAI 有用量 API）、Gemini CLI（Google 可能有类似 API）等。

**第二层：消耗监控（所有 tokscale 支持的工具）**

自动发现本机安装的 AI 工具，追踪 token 消耗。用户可为任意工具配置自定义上限，达到阈值同样触发提醒。

| # | 工具 | 数据格式 | 本地路径 |
|---|---|---|---|
| 0 | OpenCode | JSON | `~/.local/share/opencode/storage/message/` |
| 1 | Claude Code | JSONL | `~/.claude/projects/` |
| 2 | Codex CLI | JSONL | `~/.codex/sessions/` |
| 3 | Cursor | CSV | `~/.config/tokscale/cursor-cache/` |
| 4 | Gemini CLI | JSON | `~/.gemini/tmp/` |
| 5 | Amp | JSON | `~/.local/share/amp/threads/` |
| 6 | Droid (Factory) | JSON | `~/.factory/sessions/` |
| 7 | OpenClaw | JSONL | `~/.openclaw/agents/` |
| 8 | Pi | JSONL | `~/.pi/agent/sessions/` |
| 9 | Kimi | JSONL | `~/.kimi/sessions/` |
| 10 | Qwen | JSONL | `~/.qwen/projects/` |
| 11 | RooCode | JSON | VS Code globalStorage |
| 12 | KiloCode | JSON | VS Code globalStorage |
| 13 | Mux | JSON | `~/.mux/sessions/` |
| 14 | Kilo | SQLite | `~/.local/share/kilo/` |
| 15 | Crush | JSON | `~/.local/share/crush/` |

未配置上限的工具 → 仅在仪表盘展示消耗数据，不触发提醒。

---

## 技术选型

| 项目 | 选择 | 理由 |
|---|---|---|
| 语言 | Rust | 复用 tokscale-core crate；常驻进程内存低（2-5MB） |
| 异步运行时 | tokio | tokscale 已在使用 |
| HTTP 客户端 | reqwest | tokscale 已在使用 |
| 本地数据解析 | tokscale-core | 直接依赖，复用 JSONL/CSV 解析器 |
| 桌面通知 | notify-rust | macOS Notification Center |
| PWA 推送 | web-push crate | 标准 Web Push 协议，支持 iOS 16.4+ |
| Web 服务器 | axum | 轻量、tokio 生态 |
| 配置格式 | TOML | Rust 生态标准 |
| 状态存储 | JSON 文件 | 简单可靠，原子写入 |

---

## 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                        Psst Daemon                          │
│                   (macOS LaunchAgent)                        │
│                                                             │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │ Data Sources  │───▶│   Checker    │───▶│  Dispatcher   │  │
│  └──────────────┘    └──────────────┘    └───────────────┘  │
│         │                   │                    │           │
│         ▼                   ▼                    ▼           │
│  ┌────────────┐     ┌─────────────┐    ┌────────────────┐   │
│  │ tokscale   │     │  Threshold   │    │  Notifiers     │   │
│  │  -core     │     │  Engine      │    │  ┌──────────┐  │   │
│  │ (消耗数据)  │     │             │    │  │ Desktop  │  │   │
│  ├────────────┤     │ • 50% used  │    │  │ Telegram │  │   │
│  │ Claude     │     │ • 80% used  │    │  │ Server酱 │  │   │
│  │  OAuth API │     │ • 重置前1天  │    │  │ PWA Push │  │   │
│  │ (剩余配额)  │     │ • 重置前12h │    │  └──────────┘  │   │
│  ├────────────┤     │ • 重置前1h  │    └────────────────┘   │
│  │ Cursor     │     └─────────────┘                         │
│  │  推算配额   │           ▲                                  │
│  └────────────┘           │                                  │
│                    ┌──────┴──────┐    ┌──────────────────┐   │
│                    │ State Store │    │  Axum HTTP Server │   │
│                    │(已发通知记录)│    │  (PWA + 仪表盘)    │   │
│                    └─────────────┘    └──────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 核心流程

1. **Scheduler** 每 20 分钟触发一次检查（可配置）
2. **Data Sources** 同时拉取三类数据
3. **Threshold Engine** 判断是否触发通知
4. **State Store** 防止重复通知
5. **Dispatcher** 分发到所有启用的渠道
6. **Axum HTTP Server** 提供 PWA 页面 + 推送订阅接口 + 用量仪表盘

---

## 模块详细设计

### 1. 数据源模块 (data_sources)

#### 1.1 自动发现 (auto_discover)

启动时和每次检查时，调用 tokscale-core 的 scanner 模块扫描本机：

```rust
// 伪代码
let scan_result = tokscale_core::scanner::scan_all_clients();
// 返回每个 ClientId 对应的文件列表
// 有文件 = 用户在用这个工具
```

自动发现的工具列表保存到 state.json，仪表盘展示所有发现的工具。

#### 1.2 消耗数据采集（通用，所有工具）

**方式：** tokscale-core 的各工具 parser

- 对每个发现的工具，调用对应 parser 解析本地文件
- 提取 `UnifiedMessage`（时间戳、input/output tokens、model、cost）
- 按时间窗口聚合（当前小时、当天、当周、当月）
- 用于仪表盘展示 + 配额推算

#### 1.3 Claude 精确配额（第一层 — API）

**方式：** 调用未公开的 OAuth 端点

```
GET https://api.anthropic.com/api/oauth/usage
Headers:
  Authorization: Bearer <accessToken>
  anthropic-beta: oauth-2025-04-20
```

**凭证来源：** `~/.claude/.credentials.json` → `claudeAiOauth.accessToken`

**响应格式：**
```json
{
  "five_hour": {
    "utilization": 0.65,
    "resets_at": "2026-04-03T22:00:00Z"
  },
  "seven_day": {
    "utilization": 0.30,
    "resets_at": "2026-04-07T00:00:00Z"
  }
}
```

**容错策略：**
- 不主动刷新 OAuth token（避免干扰 Claude Code 的会话）
- 遇到 HTTP 429 → 跳过本次检查，等下一轮（20 分钟后）
- Token 过期 → 记日志提示用户重新登录 Claude Code
- 网络错误 → 记日志，使用上次缓存的数据

#### 1.4 Cursor 配额推算（第一层 — 推算）

**方式：** 已知上限 - 已消耗 = 剩余

- tokscale-core 解析 `~/.config/tokscale/cursor-cache/` 获取已消耗数据
- 用户在 config.toml 中配置计划上限
- 推算利用率和剩余额度

**局限性：** 精确度依赖用户正确配置上限值。在配置文件和文档中明确说明。

#### 1.5 自定义配额（第二层 — 用户配置）

任意工具均可在 config.toml 中配置上限，实现配额推算：

```toml
[providers.codex]
daily_token_limit = 1000000

[providers.gemini]
daily_token_limit = 500000
```

配置了上限的工具 → 用 tokscale 消耗数据推算利用率 → 达到阈值触发提醒。
未配置上限的工具 → 仅在仪表盘展示消耗，不触发提醒。

#### 1.6 可扩展的精确配额接口

为将来接入更多工具的官方 API 预留 trait：

```rust
#[async_trait]
trait QuotaProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_quota(&self) -> Result<QuotaInfo>;
}

struct QuotaInfo {
    windows: Vec<QuotaWindow>,  // 可能有多个时间窗口
}

struct QuotaWindow {
    name: String,               // e.g. "five_hour", "seven_day", "monthly"
    utilization: f64,           // 0.0 ~ 1.0
    resets_at: DateTime<Utc>,
}
```

目前实现：`ClaudeQuotaProvider`（API）、`EstimatedQuotaProvider`（推算，适用于 Cursor 及所有自定义配额的工具）。后续可添加 `CodexQuotaProvider`、`GeminiQuotaProvider` 等。

---

### 2. 阈值引擎 (threshold_engine)

#### 两类规则并行判断

**规则 A — 用量阈值：**
```
for threshold in config.usage_alerts:  # 默认 [50, 80]
  if utilization >= threshold%
     AND threshold NOT in alerts_sent:
    → 触发通知
    → 将 threshold 加入 alerts_sent
```

**规则 B — 重置倒计时：**
```
remaining = resets_at - now

for hours in config.reset_alerts_hours:  # 默认 [24, 12, 1]
  if remaining <= hours
     AND hours NOT in reset_alerts_sent
     AND utilization < 0.95:  # 已用 95% 以上不再提醒"赶紧用"
    → 触发通知
    → 将 hours 加入 reset_alerts_sent
```

**周期重置检测：**
```
if now > resets_at:
  → 清空 alerts_sent 和 reset_alerts_sent
  → 拉取新的 resets_at
```

`utilization < 0.95` 阈值可在配置文件中调整。

---

### 3. 通知系统 (notifiers)

#### Notifier Trait

```rust
#[async_trait]
trait Notifier: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<()>;
    fn name(&self) -> &str;
    fn is_enabled(&self) -> bool;
}
```

#### 四个渠道实现

| 渠道 | 实现方式 | 依赖 |
|---|---|---|
| 桌面通知 | `notify-rust` → macOS Notification Center | 无 |
| Telegram | `POST api.telegram.org/bot{token}/sendMessage` | bot_token + chat_id |
| Server酱 | `POST sctapi.ftqq.com/send` with title + desp | send_key |
| PWA Push | `web-push` crate → Apple/Google Push Service | VAPID 密钥 + 订阅信息 |

#### 通知消息模板

**用量阈值通知：**
```
Psst! Claude 5小时窗口已用 80%

- 当前用量：80%
- 剩余额度：约 20%
- 重置时间：2小时15分钟后 (14:00)
```

**重置倒计时通知：**
```
Psst! Claude 7天窗口 12小时后重置

- 当前用量：仅 30%
- 剩余额度：约 70% 未使用
- 重置时间：明天 00:00
- 建议在重置前充分利用剩余额度
```

**Cursor 通知：**
```
Psst! Cursor Pro 本月已用 50%

- 已使用：250 / 500 fast requests
- 重置时间：12天后 (4月15日)
```

#### 防骚扰机制

- **防重复：** 同一阈值同一周期只通知一次（由 state.json 的 alerts_sent 控制）
- **静默时段（可选）：** 配置 `quiet_hours = "23:00-08:00"`，深夜不发通知，攒到早上一次性发
- **降级策略：** 某渠道发送失败 → 记日志，不影响其他渠道；连续失败 3 次 → 暂停该渠道 1 小时后重试

---

### 4. PWA 与 Web 服务器 (web)

#### HTTP Server (axum)

监听 `127.0.0.1:3377`（可配置）

**路由：**

| 路径 | 方法 | 功能 |
|---|---|---|
| `/` | GET | PWA 仪表盘页面（显示用量数据） |
| `/manifest.json` | GET | PWA Manifest |
| `/sw.js` | GET | Service Worker |
| `/api/subscribe` | POST | 接收 PWA 推送订阅信息 |
| `/api/status` | GET | 返回当前用量 JSON |
| `/api/health` | GET | 健康检查 |

**安全措施：**

- 默认只绑定 `127.0.0.1`（仅本机访问）
- 如需局域网访问（手机设置 PWA），切换为 `0.0.0.0` 并启用 token 验证
- 首次启动自动生成随机 access token，打印在终端
- 手机通过 `http://192.168.x.x:3377?token=xxx` 访问

#### PWA 推送流程

**首次设置（一次性）：**
1. 手机在局域网内打开 Psst PWA 页面
2. 添加到主屏幕
3. 点击"启用推送"按钮 → 授权通知权限
4. 浏览器生成推送订阅信息（endpoint + 加密密钥）
5. 发送到 `/api/subscribe` → 保存到 state.json

**后续推送（手机在任何网络）：**
1. Psst 用 `web-push` crate 加密消息
2. POST 到订阅信息中的 endpoint（Apple/Google 服务器）
3. Apple/Google 将通知推送到手机
4. 手机不需要和 Psst 在同一网络

#### HTTPS 问题

PWA 的 Service Worker 要求 HTTPS。解决方案：
- `localhost` 在大多数浏览器中豁免 HTTPS 要求
- 局域网访问时，推荐 Tailscale（免费 mesh VPN，自带 HTTPS 证书）
- 或 Cloudflare Tunnel（免费，临时公网域名）
- 在文档中提供三种方案的配置指南

---

### 5. 状态持久化 (state)

#### 文件布局

```
~/.config/psst/
├── config.toml          # 用户配置（手动编辑）
├── state.json           # 运行状态（自动维护）
├── vapid_private.pem    # VAPID 私钥（首次启动自动生成）
├── vapid_public.pem     # VAPID 公钥
└── logs/
    └── psst.log         # 运行日志（带轮转）
```

#### state.json 结构

```json
{
  "version": 1,
  "last_check_at": "2026-04-03T10:40:00Z",
  "access_token": "random-generated-token-for-web",

  "discovered_tools": ["claude", "cursor", "codex", "gemini"],

  "providers": {
    "claude": {
      "windows": {
        "five_hour": {
          "utilization": 0.65,
          "resets_at": "2026-04-03T14:00:00Z",
          "alerts_sent": [50],
          "reset_alerts_sent": []
        },
        "seven_day": {
          "utilization": 0.30,
          "resets_at": "2026-04-07T00:00:00Z",
          "alerts_sent": [],
          "reset_alerts_sent": [24]
        }
      }
    },
    "cursor": {
      "windows": {
        "monthly": {
          "utilization": 0.42,
          "used_count": 210,
          "resets_at": "2026-04-15T00:00:00Z",
          "alerts_sent": [],
          "reset_alerts_sent": []
        }
      }
    },
    "codex": {
      "windows": {
        "daily": {
          "utilization": 0.60,
          "used_tokens": 600000,
          "resets_at": "2026-04-04T00:00:00Z",
          "alerts_sent": [50],
          "reset_alerts_sent": []
        }
      }
    }
  },

  "push_subscriptions": [
    {
      "endpoint": "https://web.push.apple.com/xxx",
      "keys": {
        "p256dh": "...",
        "auth": "..."
      },
      "created_at": "2026-04-03T10:00:00Z"
    }
  ]
}
```

#### 容错机制

| 场景 | 处理方式 |
|---|---|
| 正常关机/重启 | 读取 state.json，恢复所有状态 |
| 崩溃（写入中断） | 原子写入（先写 .tmp → fsync → rename）保证文件完整 |
| 跨越重置点关机 | 启动时检测 resets_at 已过期 → 清空 alerts_sent |
| state.json 被删 | 等同首次运行，重新触发一轮通知 |
| state.json 损坏 | 备份损坏文件，创建空状态，记日志警告 |
| 长时间关机后启动 | 启动后立即执行一次检查 |

#### 原子写入流程

```
1. 序列化状态到 JSON 字符串
2. 写入 ~/.config/psst/state.json.tmp
3. fsync 确保落盘
4. rename state.json.tmp → state.json（原子操作）
```

---

### 6. 配置文件 (config.toml)

```toml
[general]
check_interval_minutes = 20          # 检查间隔，默认 20 分钟
auto_discover = true                 # 自动发现本机安装的 AI 工具

[thresholds]
usage_alerts = [50, 80]              # 用量达到此百分比时提醒
reset_alerts_hours = [24, 12, 1]     # 重置前多少小时提醒
skip_reset_alert_above = 0.95        # 用量超过此值不再发重置提醒

# ── 第一层：精确配额（有 API 的工具） ──

[providers.claude]
# 自动读取 ~/.claude/.credentials.json，无需手动配置

[providers.cursor]
monthly_fast_requests = 500          # 计划上限
billing_day = 15                     # 每月几号重置

# ── 第二层：自定义配额（任意工具均可配置） ──
# 配置了上限的工具会触发阈值提醒
# 未配置的工具仅在仪表盘展示消耗数据

# [providers.codex]
# daily_token_limit = 1000000

# [providers.gemini]
# daily_token_limit = 500000

# [providers.opencode]
# daily_token_limit = 800000

# ── 通知渠道 ──

[notifications]
desktop = true
quiet_hours = ""                     # 可选，例如 "23:00-08:00"

[notifications.telegram]
enabled = false
bot_token = ""
chat_id = ""

[notifications.serverchan]
enabled = false
send_key = ""

[notifications.web_push]
enabled = true                       # PWA 推送

[server]
bind = "127.0.0.1:3377"
# bind = "0.0.0.0:3377"             # 局域网访问时取消注释
```

---

### 7. 运行方式

#### 命令行

```bash
# 首次运行（生成默认配置 + VAPID 密钥）
psst init

# 前台运行（终端中看日志）
psst run

# 查看当前状态
psst status

# 安装为 macOS LaunchAgent（开机自启）
psst install

# 卸载 LaunchAgent
psst uninstall
```

#### macOS LaunchAgent

`psst install` 生成 `~/Library/LaunchAgents/com.psst.notify.plist`：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.psst.notify</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/psst</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/psst.out.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/psst.err.log</string>
</dict>
</plist>
```

---

## 项目结构

```
Psst/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE                          # MIT
├── src/
│   ├── main.rs                      # CLI 入口（clap）
│   ├── config.rs                    # 配置加载与验证
│   ├── state.rs                     # 状态持久化（原子读写）
│   ├── scheduler.rs                 # 定时检查调度器
│   ├── data_sources/
│   │   ├── mod.rs
│   │   ├── discovery.rs             # 自动发现本机 AI 工具
│   │   ├── usage_collector.rs       # 通用消耗采集（调用 tokscale-core）
│   │   ├── claude_quota.rs          # Claude OAuth API 精确配额
│   │   └── estimated_quota.rs       # 通用配额推算（Cursor + 任意自定义工具）
│   ├── threshold.rs                 # 阈值引擎
│   ├── notifiers/
│   │   ├── mod.rs                   # Notifier trait + Dispatcher
│   │   ├── desktop.rs               # macOS 桌面通知
│   │   ├── telegram.rs              # Telegram Bot API
│   │   ├── serverchan.rs            # Server酱
│   │   └── web_push.rs              # PWA Web Push
│   └── web/
│       ├── mod.rs                   # axum 路由
│       ├── api.rs                   # REST API 端点
│       └── static/                  # PWA 静态资源
│           ├── index.html           # 仪表盘页面
│           ├── manifest.json        # PWA Manifest
│           ├── sw.js                # Service Worker
│           └── app.js               # 前端逻辑
├── docs/
│   └── superpowers/
│       └── specs/
└── tests/
    ├── threshold_test.rs
    ├── state_test.rs
    └── notifier_test.rs
```

---

## 依赖清单

```toml
[dependencies]
tokscale-core = { path = "../tokscale-main/crates/tokscale-core" }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
axum = "0.7"
tower-http = { version = "0.5", features = ["fs", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
notify-rust = "4"
web-push = "0.10"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"
anyhow = "1"
async-trait = "0.1"
uuid = { version = "1", features = ["v4"] }
```

---

## 安全考量

1. **不存储敏感凭证：** Claude OAuth token 从 `~/.claude/.credentials.json` 实时读取，不复制
2. **不刷新 OAuth token：** 避免干扰 Claude Code 的会话
3. **Web 访问保护：** 默认仅 localhost，局域网模式需 token 验证
4. **原子写入：** state.json 不会因崩溃而损坏
5. **VAPID 密钥本地生成：** 不依赖任何外部服务
6. **无遥测/上报：** 不收集任何用户数据，不连接 tokscale.ai

---

## 不做的事（YAGNI）

- 不做用户系统/登录
- 不做多用户支持（每人运行自己的实例）
- 不做历史数据图表（tokscale 已经做了）
- 不做 Android 原生推送（PWA Push 在 Android 上天然支持）
- 不做自动刷新 Claude OAuth token

---

## 扩展路线

1. **更多精确配额 API** — 随着各工具开放 API，逐步实现 `QuotaProvider`（如 Codex、Gemini）
2. **更多通知渠道** — Discord、飞书、钉钉等（实现 `Notifier` trait 即可）
3. **更多平台** — Linux LaunchAgent 等效方案（systemd service）

# Psst — 逻辑参考文档

> 本文档记录各模块的运行时逻辑和交互方式，供开发新功能时比对，避免新旧逻辑冲突。
> 最后更新：2026-04-04

---

## 整体数据流

```
Scheduler.check_once()  (每 20 分钟)
  │
  ├─ 1. build_providers()       创建 QuotaProvider 实例
  │      ├─ "cursor" → CursorLocalProvider (SQLite)
  │      └─ 其他     → EstimatedQuotaProvider (tokscale-core)
  │
  ├─ 2. provider.fetch_quota()  全部本地 I/O，无远程请求
  │      → Vec<QuotaInfo>  +  Vec<(id, error)>
  │
  ├─ 3. state.clear_expired_windows()  重置过期窗口的去重记录
  │
  ├─ 4. discover_tools()        (若 auto_discover=true) 重新扫描
  │
  ├─ 5. 写入错误 → state.providers[id].last_error
  │
  ├─ 6. 处理配额结果：
  │      for each QuotaWindow:
  │        a. 更新 window state (utilization, resets_at, used_*)
  │        b. evaluate_thresholds() → Vec<AlertEvent>
  │        c. format_notification() → Notification
  │        d. dispatcher.dispatch() → 分发到所有启用的通知渠道
  │        e. record_alerts() → 去重标记写入 state
  │
  └─ 7. state.mark_checked() + save_atomic()
```

---

## 1. CLI 入口 (main.rs)

| 命令 | 函数 | 说明 |
|------|------|------|
| `psst init` | `cmd_init()` | 创建配置目录、自动发现工具、生成 config.toml + state.json + VAPID 密钥 |
| `psst run` | `cmd_run()` | 加载配置/状态 → 并发启动 WebServer + Scheduler |
| `psst status` | `cmd_status()` | 读取 state.json 打印各 provider 用量 |
| `psst install` | `cmd_install()` | 生成 LaunchAgent plist + launchctl load |
| `psst uninstall` | `cmd_uninstall()` | launchctl unload + 删除 plist |

### init 流程细节

1. `discover_tools()` 扫描本机已安装的 AI 工具
2. `generate_config_toml()` 为每个发现的工具预填默认限额
   - 默认值：Claude 1000, Cursor 500, Copilot 3000, 其他 500
3. 生成 UUID access token（用于 dashboard 认证）
4. OpenSSL 生成 VAPID EC P-256 密钥对

### run 启动顺序

1. 加载 config.toml（不存在则用默认值）
2. 加载或创建 state.json，确保 access_token 存在
3. `build_dispatcher()` → 创建 4 个 Notifier 实例
4. `tokio::spawn` WebServer（共享 state Arc）
5. `scheduler.run()` 阻塞主线程（立即执行第一次检查，之后按间隔循环）

---

## 2. 调度器 (scheduler.rs)

### Scheduler 结构

```rust
Scheduler {
    config: Config,
    state_path: PathBuf,
    state: Arc<Mutex<AppState>>,   // 与 WebServer 共享
    dispatcher: Arc<Dispatcher>,
    home_dir: String,
}
```

### build_providers() 逻辑

遍历 `config.providers`：
- **`"cursor"`** → `CursorLocalProvider`（需要 `monthly_fast_requests`）
- **其他 ID** → `EstimatedQuotaProvider`（需要 `monthly_fast_requests` 或 `daily_token_limit`）
- 未配置限额的 provider **不创建实例**（不会出现在配额检查中）

### check_once() 关键约束

- **state 锁范围**：获取锁后完成所有状态更新和通知分发，然后释放
- **错误隔离**：单个 provider 失败不影响其他 provider
- **原子保存**：所有更新完成后统一保存

---

## 3. 状态管理 (state.rs)

### AppState 结构

```rust
AppState {
    version: u32,                                  // 固定 1
    last_check_at: Option<String>,                 // RFC 3339
    access_token: Option<String>,                  // UUID
    discovered_tools: Vec<String>,
    providers: HashMap<String, ProviderState>,
    push_subscriptions: Vec<PushSubscription>,
}
```

### ProviderState

```rust
ProviderState {
    windows: HashMap<String, QuotaWindowState>,
    last_error: Option<String>,                    // 最近一次错误（成功时清除）
}
```

### QuotaWindowState — 去重核心

```rust
QuotaWindowState {
    utilization: f64,                 // 0.0 ~ 1.0+
    used_tokens: Option<i64>,
    used_count: Option<u64>,
    resets_at: Option<String>,        // RFC 3339
    alerts_sent: Vec<u32>,            // 已触发的用量阈值 (如 [50, 80])
    reset_alerts_sent: Vec<u32>,      // 已触发的倒计时阈值 (如 [24, 12])
}
```

### PushSubscription

```rust
PushSubscription {
    endpoint: String,                  // 推送服务端点 URL
    keys: PushKeys { p256dh, auth },   // base64 编码
    created_at: String,                // RFC 3339
}
```

### 关键方法

| 方法 | 逻辑 |
|------|------|
| `save_atomic()` | 写 `.tmp` → fsync → rename（防崩溃） |
| `load_or_default()` | 文件不存在/损坏 → 返回默认值（损坏时备份为 `.corrupted`） |
| `clear_expired_windows()` | `resets_at` 已过期 → 清空 `alerts_sent` + `reset_alerts_sent` |
| `ensure_access_token()` | 无 token → 生成 UUID |
| `mark_checked()` | 设置 `last_check_at` 为当前 UTC 时间 |

---

## 4. 配置 (config.rs)

### 结构层次

```
Config
├── general
│   ├── check_interval_minutes: u32    (默认 20)
│   └── auto_discover: bool            (默认 true)
├── thresholds
│   ├── usage_alerts: Vec<u32>         (默认 [50, 80])
│   ├── reset_alerts_hours: Vec<u32>   (默认 [24, 12, 1])
│   └── skip_reset_alert_above: f64    (默认 0.95)
├── providers: HashMap<String, ProviderConfig>
│   └── ProviderConfig
│       ├── monthly_fast_requests: Option<u64>
│       ├── billing_day: Option<u32>   (1-28, 默认 1)
│       └── daily_token_limit: Option<u64>
├── notifications
│   ├── desktop: bool                  (默认 true)
│   ├── quiet_hours: Option<String>    (未实现)
│   ├── telegram: { enabled, bot_token, chat_id }
│   ├── serverchan: { enabled, send_key }
│   └── web_push: { enabled }          (默认 true)
└── server
    └── bind: String                   (默认 "127.0.0.1:3377")
```

### 加载

- `Config::load_from(path)` 解析 TOML
- 缺失的段落使用 `#[serde(default)]` 回退到默认值

---

## 5. 阈值引擎 (threshold.rs)

### evaluate_thresholds() 输入/输出

**输入：**
- `provider_id`, `window_name`
- `window: &QuotaWindowState`（含去重记录）
- `usage_alerts: &[u32]`（如 `[50, 80]`）
- `reset_alerts_hours: &[u32]`（如 `[24, 12, 1]`）
- `skip_reset_alert_above: f64`（如 `0.95`）

**输出：** `Vec<AlertEvent>`

### 规则 A — 用量阈值

```
对每个 threshold ∈ usage_alerts:
  若 utilization >= threshold/100
    且 threshold 不在 window.alerts_sent 中:
      → 生成 AlertEvent::UsageThreshold(threshold)
```

### 规则 B — 重置倒计时

```
若 utilization < skip_reset_alert_above:     // 用量 >= 95% 时跳过
  对每个 hours ∈ reset_alerts_hours:
    若 距重置 <= hours 小时
      且 hours 不在 window.reset_alerts_sent 中:
        → 生成 AlertEvent::ResetCountdown(hours)
```

### 去重机制

- `record_alerts()` 将触发的阈值写入 `alerts_sent` / `reset_alerts_sent`
- 同一周期同一阈值只触发一次
- 周期重置时 `clear_expired_windows()` 清空两个向量

### 边界情况

- 用量从 40% 跳到 90%：50% 和 80% 两个阈值同时触发
- 距重置 6 小时：12h 和 1h 两个倒计时同时触发
- 用量 > 95%：不发"快重置了赶紧用"提醒

---

## 6. 数据源 (data_sources/)

### QuotaProvider trait

```rust
trait QuotaProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_quota(&self) -> Result<QuotaInfo>;
}
```

### QuotaInfo / QuotaWindow

```rust
QuotaInfo { provider_id, windows: Vec<QuotaWindow> }
QuotaWindow { name, utilization, resets_at, used_tokens, used_count }
```

### discovery.rs — 工具发现

- 调用 `tokscale_core::scanner::scan_all_clients()`
- 扫描 `$HOME` 下各工具的数据目录
- 返回 `Vec<String>`（工具 ID 列表）

### cursor_local.rs — Cursor 本地数据

**数据源：** `~/.cursor/ai-tracking/ai-code-tracking.db`（SQLite）

**逻辑：**
1. 根据 `billing_day` 计算当前计费周期起止日期
2. SQL：`COUNT(DISTINCT requestId) FROM ai_code_hashes WHERE createdAt >= ?`
3. `utilization = used / monthly_limit`
4. 返回单个 window：`"monthly_requests"`

### estimated_quota.rs — 通用估算

**数据源：** tokscale-core 解析本地日志文件

**两种窗口：**

| 窗口 | 触发条件 | 指标 |
|------|---------|------|
| `monthly_requests` | 配置了 `monthly_fast_requests` | `message_count / limit` |
| `daily_tokens` | 配置了 `daily_token_limit` | `total_tokens / limit` |

### usage_collector.rs — tokscale-core 集成

```rust
collect_usage_since(home_dir, tool_id, since) → UsageSummary
collect_usage_today(home_dir, tool_id) → UsageSummary

UsageSummary { total_tokens, total_cost, message_count }
```

- 调用 `tokscale_core::parse_local_unified_messages()`
- 聚合返回的消息列表

---

## 7. 通知系统 (notifiers/)

### Notifier trait

```rust
trait Notifier: Send + Sync {
    fn name(&self) -> &str;
    fn is_enabled(&self) -> bool;
    async fn send(&self, notification: &Notification) -> Result<()>;
}
```

### Dispatcher

```rust
for notifier in &self.notifiers {
    if notifier.is_enabled() {
        notifier.send(notification).await;  // 错误仅 warn 日志，不中断
    }
}
```

**关键约束：** 单个渠道失败不影响其他渠道。

### Notification 格式化

```rust
format_notification(event: &AlertEvent) → Notification { title, body, provider_id, window_name }
```

- 通知文案为**中文**
- 用量阈值：`"Psst! Claude 月度配额已用 80%"`
- 重置倒计时：`"Psst! Claude 月度配额 12小时后重置"`
- 时间格式化：天/小时/分钟自适应

### 四个通知渠道

| 渠道 | 实现 | 外部依赖 |
|------|------|---------|
| Desktop | `notify-rust` → macOS Notification Center | 无 |
| Telegram | POST `api.telegram.org/bot{token}/sendMessage`（Markdown） | bot_token + chat_id |
| Server酱 | POST `sctapi.ftqq.com/{key}.send`（form 编码） | send_key |
| Web Push | `web-push` crate → VAPID 签名 + AES-128-GCM 加密 | VAPID 密钥 + 订阅信息 |

### Web Push 特殊逻辑

- 从 `state.push_subscriptions` 读取所有订阅
- 无订阅 → 直接返回（静默跳过）
- VAPID 私钥从 PEM 文件读取（每次 send 时）
- 每个订阅单独构建 VAPID 签名
- payload 含 `tag` 字段（`"psst-{provider}-{window}"`）用于浏览器端去重

---

## 8. Web 服务器 (web/)

### 路由表

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/` | Token | Dashboard HTML |
| GET | `/manifest.json` | 无 | PWA manifest |
| GET | `/sw.js` | 无 | Service Worker |
| GET | `/app.js` | 无 | 前端 JS |
| GET | `/api/health` | 无 | `{ "status": "ok" }` |
| GET | `/api/status` | Token | 当前用量快照 |
| GET | `/api/config` | Token | Provider 配置 |
| POST | `/api/config` | Token | 更新 provider 限额 → 写回 config.toml |
| POST | `/api/subscribe` | Token | 保存推送订阅（按 endpoint 去重） |
| GET | `/api/vapid-public-key` | 无 | VAPID 公钥（base64url 编码） |

### Token 认证

- 通过 URL query `?token=xxx` 传递
- 无 token 配置 → 所有请求放行
- 有 token → 必须匹配才能访问受保护路由

### VAPID 公钥端点

1. 读取 `vapid_public.pem` 文件
2. 去除 PEM 头尾，base64 解码得到 DER
3. 提取末尾 65 字节（EC P-256 未压缩公钥点）
4. 返回 base64url 编码（无 padding）

### 前端推送订阅流程 (app.js)

1. 注册 Service Worker (`/sw.js`)
2. 页面加载时检查已有订阅（`checkPushState()`）
3. 用户点击按钮 → `subscribePush()`：
   a. 检查浏览器支持
   b. 请求通知权限
   c. `fetch('/api/vapid-public-key')` 获取 VAPID 公钥
   d. `urlBase64ToUint8Array()` 转换格式
   e. `pushManager.subscribe({ userVisibleOnly: true, applicationServerKey })`
   f. 将 subscription 发送到 `POST /api/subscribe`

### Service Worker (sw.js)

- `push` 事件：解析 JSON payload → `showNotification()` (含 title, body, tag)
- `notificationclick` 事件：关闭通知 → 聚焦窗口或打开 app

---

## 9. 安全策略

| 策略 | 说明 |
|------|------|
| 不存储凭证 | Claude OAuth token 实时从 Keychain 读取 |
| 不刷新 token | 避免干扰 Claude Code 会话 |
| 默认 localhost | Web 服务器绑定 127.0.0.1 |
| Token 认证 | 局域网访问需 access_token |
| 原子写入 | state.json 防崩溃 |
| VAPID 本地生成 | 不依赖外部服务 |
| 无遥测 | 不上报任何数据 |
| 无远程 API | 配额数据全部本地读取 |

---

## 10. 文件布局

```
~/.config/psst/
├── config.toml          用户配置
├── state.json           运行状态（自动维护）
├── vapid_private.pem    VAPID 私钥
├── vapid_public.pem     VAPID 公钥
└── psst.log / psst.err  日志（LaunchAgent 模式）
```

---

## 11. 尚未实现

| 功能 | 状态 |
|------|------|
| `quiet_hours`（静默时段） | 配置字段已定义，逻辑未实现 |
| 通知失败降级（连续 3 次失败暂停 1 小时） | 设计文档中提及，未实现 |
| Telegram/Server酱 实际调通 | 代码就绪，需实机配置测试 |
| Web Push 端到端 | VAPID 密钥链路已完成，需实机测试 |
| Desktop 通知实机验证 | notify-rust 代码就绪，需验证 macOS 权限 |

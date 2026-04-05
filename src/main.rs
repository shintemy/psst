use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;

use std::sync::Arc;

use psst::config::Config;
use psst::data_sources::discovery::discover_tools;
use psst::notifiers::{
    desktop::DesktopNotifier,
    serverchan::ServerChanNotifier,
    telegram::TelegramNotifier,
    web_push_notifier::WebPushNotifier,
    Dispatcher, Notifier,
};
use psst::scheduler::Scheduler;
use psst::state::AppState;
use psst::web;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "psst", about = "AI coding tool usage monitor & notifier")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create config + state files in ~/.config/psst/
    Init,
    /// Start the monitoring daemon (foreground)
    Run,
    /// Print current usage status
    Status,
    /// Send a test notification to all enabled channels
    TestNotify,
    /// Install macOS LaunchAgent
    Install,
    /// Remove macOS LaunchAgent
    Uninstall,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn psst_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".config").join("psst"))
}

fn config_path() -> Result<PathBuf> {
    Ok(psst_config_dir()?.join("config.toml"))
}

fn state_path() -> Result<PathBuf> {
    Ok(psst_config_dir()?.join("state.json"))
}

fn home_dir_str() -> Result<String> {
    dirs::home_dir()
        .context("Cannot determine home directory")
        .map(|p| p.to_string_lossy().to_string())
}

fn build_dispatcher(config: &Config, state_arc: Arc<Mutex<AppState>>) -> Dispatcher {
    let vapid_key_path = psst_config_dir()
        .map(|d| d.join("vapid_private.pem").to_string_lossy().to_string())
        .unwrap_or_else(|_| String::from("vapid_private.pem"));

    let notifiers: Vec<Box<dyn Notifier>> = vec![
        Box::new(DesktopNotifier::new(config.notifications.desktop)),
        Box::new(TelegramNotifier::new(
            config.notifications.telegram.bot_token.clone(),
            config.notifications.telegram.chat_id.clone(),
            config.notifications.telegram.enabled,
        )),
        Box::new(ServerChanNotifier::new(
            config.notifications.serverchan.send_key.clone(),
            config.notifications.serverchan.enabled,
        )),
        Box::new(WebPushNotifier::new(
            config.notifications.web_push.enabled,
            state_arc,
            vapid_key_path,
        )),
    ];
    Dispatcher::new(notifiers)
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

/// Sensible default monthly request budget for each tool.
/// These are rough estimates — most platforms don't publish exact numbers.
/// Users should adjust to match their actual plan.
fn default_monthly_requests(tool: &str) -> u64 {
    match tool {
        "claude" => 1000,
        "cursor" => 500,
        "windsurf" => 500,
        "copilot" => 3000,
        "augment" => 500,
        "aider" => 1000,
        "roo" => 500,
        "kilo" | "kilo-code" => 500,
        "cline" => 500,
        "continue" => 500,
        _ => 500,
    }
}

/// Hint comment for each tool in generated config.
fn tool_hint(tool: &str) -> &'static str {
    match tool {
        "claude" => "Claude Code — Pro $20: 1x, Max $100: 5x, Max $200: 20x (exact limits not published, set your own budget)",
        "cursor" => "Cursor — Pro $20: 1x, Pro+ $60: 3x, Ultra $200: 20x (exact limits not published, set your own budget)",
        "copilot" => "GitHub Copilot — unlimited plan, set a personal budget to track usage",
        "windsurf" => "Windsurf — adjust to match your plan",
        "augment" => "Augment Code — adjust to match your plan",
        "aider" => "Aider — depends on your API key budget",
        _ => "Adjust to match your plan",
    }
}

fn generate_config_toml(discovered: &[String]) -> String {
    let mut s = String::new();

    s.push_str("# Psst - AI coding tool usage monitor & notifier\n");
    s.push_str("#\n");
    s.push_str("# monthly_fast_requests = your personal usage budget (requests/month)\n");
    s.push_str("# Most platforms don't publish exact limits, so set a number that\n");
    s.push_str("# makes sense for your plan. Psst will alert you at 50% and 80%.\n");
    s.push_str("# billing_day = day of month your billing cycle resets (1-28)\n\n");

    s.push_str("[general]\n");
    s.push_str("check_interval_minutes = 20\n");
    s.push_str("auto_discover = true\n\n");

    s.push_str("[thresholds]\n");
    s.push_str("usage_alerts = [50, 80]\n");
    s.push_str("reset_alerts_hours = [24, 12, 1]\n");
    s.push_str("skip_reset_alert_above = 0.95\n\n");

    // ── Provider section ──
    if discovered.is_empty() {
        s.push_str("# No AI coding tools detected. Add providers manually:\n");
        s.push_str("# [providers.claude]\n");
        s.push_str("# monthly_fast_requests = 1000\n");
        s.push_str("# billing_day = 1\n\n");
    } else {
        s.push_str(&format!(
            "# ── Detected {} tool(s) — adjust limits to match your plan ──\n\n",
            discovered.len()
        ));
        for tool in discovered {
            let limit = default_monthly_requests(tool);
            let hint = tool_hint(tool);
            s.push_str(&format!("[providers.{}]\n", tool));
            s.push_str(&format!("# {}\n", hint));
            s.push_str(&format!("monthly_fast_requests = {}\n", limit));
            s.push_str("billing_day = 1\n\n");
        }
    }

    s.push_str("[notifications]\n");
    s.push_str("desktop = true\n");
    s.push_str("# quiet_hours = \"23:00-08:00\"\n\n");

    s.push_str("[notifications.telegram]\n");
    s.push_str("enabled = false\n");
    s.push_str("bot_token = \"\"\n");
    s.push_str("chat_id = \"\"\n\n");

    s.push_str("[notifications.serverchan]\n");
    s.push_str("enabled = false\n");
    s.push_str("send_key = \"\"\n\n");

    s.push_str("[notifications.web_push]\n");
    s.push_str("enabled = true\n\n");

    s.push_str("[server]\n");
    s.push_str("bind = \"127.0.0.1:3377\"\n");

    s
}

fn cmd_init() -> Result<()> {
    let config_dir = psst_config_dir()?;
    std::fs::create_dir_all(&config_dir)
        .with_context(|| format!("Failed to create config dir: {}", config_dir.display()))?;

    let cfg_path = config_dir.join("config.toml");
    let state_path = config_dir.join("state.json");
    let home_dir = home_dir_str()?;

    // Write config — auto-discover tools and pre-fill defaults.
    if !cfg_path.exists() {
        println!("Scanning for AI coding tools...");
        let discovered = discover_tools(&home_dir);
        if discovered.is_empty() {
            println!("  No tools detected. You can add providers manually in config.toml.");
        } else {
            println!("  Found: {}", discovered.join(", "));
        }

        let config_content = generate_config_toml(&discovered);
        std::fs::write(&cfg_path, &config_content)
            .with_context(|| format!("Failed to write {}", cfg_path.display()))?;
        println!("Created config: {}", cfg_path.display());
        println!("\n  → Edit this file to adjust quota limits for your plan.");
    } else {
        println!("Config already exists: {}", cfg_path.display());
    }

    // Load or create state, ensure access token, then save.
    let mut state = AppState::load_or_default(&state_path);
    state.ensure_access_token();
    state
        .save_atomic(&state_path)
        .with_context(|| format!("Failed to write {}", state_path.display()))?;

    println!("State file: {}", state_path.display());

    if let Some(token) = &state.access_token {
        println!("\nAccess token: {}", token);
        println!("Use this token to authenticate with the web UI.");
    }

    // Generate VAPID keys for web push if not already present.
    let vapid_private = config_dir.join("vapid_private.pem");
    if !vapid_private.exists() {
        println!("Generating VAPID keys...");
        let status = std::process::Command::new("openssl")
            .args(["ecparam", "-genkey", "-name", "prime256v1", "-out"])
            .arg(&vapid_private)
            .status();
        match status {
            Ok(s) if s.success() => {
                let _ = std::process::Command::new("openssl")
                    .args(["ec", "-in"])
                    .arg(&vapid_private)
                    .args(["-pubout", "-out"])
                    .arg(&config_dir.join("vapid_public.pem"))
                    .status();
                println!("VAPID keys generated.");
            }
            _ => {
                println!("Warning: Failed to generate VAPID keys (openssl not found?)");
            }
        }
    }

    println!("\nDone! Run `psst run` to start monitoring.");

    Ok(())
}

#[tokio::main]
async fn cmd_run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg_path = config_path()?;
    let state_path = state_path()?;

    let config = if cfg_path.exists() {
        Config::load_from(&cfg_path)
            .with_context(|| format!("Failed to load config from {}", cfg_path.display()))?
    } else {
        info!("No config file found at {}; using defaults", cfg_path.display());
        Config::default()
    };

    let mut state = AppState::load_or_default(&state_path);
    state.ensure_access_token();
    state.save_atomic(&state_path)?;

    let state_arc = Arc::new(Mutex::new(state));
    let dispatcher = build_dispatcher(&config, Arc::clone(&state_arc));
    let home_dir = home_dir_str()?;

    let scheduler = Scheduler::new(config.clone(), state_path, state_arc, dispatcher, home_dir);

    let shared_state = scheduler.shared_state();
    let access_token = { shared_state.lock().await.access_token.clone() };
    let web_bind = config.server.bind.clone();
    // Print dashboard URL hint for the user.
    let dashboard_url = if let Some(ref token) = access_token {
        format!("http://{}?token={}", web_bind, token)
    } else {
        format!("http://{}", web_bind)
    };
    print!("  ██      ██\n  ██████████\n  ██  ██  ██    Psst\n  ██████████\n  ████████        ██\n  ██████████████████\n  ████████████████████\n████████████████████████\n\n");
    println!("  🔗 Dashboard: {}\n", dashboard_url);

    let web_config_path = cfg_path.clone();
    let vapid_public_key_path = psst_config_dir()?.join("vapid_public.pem");
    tokio::spawn(async move {
        let server = web::WebServer::new(web_bind, shared_state, access_token, web_config_path, vapid_public_key_path);
        if let Err(e) = server.run().await {
            tracing::error!("Web server error: {}", e);
        }
    });

    scheduler.run().await;

    Ok(())
}

#[tokio::main]
async fn cmd_test_notify() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg_path = config_path()?;
    let state_path = state_path()?;

    let config = if cfg_path.exists() {
        Config::load_from(&cfg_path)?
    } else {
        anyhow::bail!("Config not found. Run `psst init` first.");
    };

    let state = AppState::load_or_default(&state_path);
    let state_arc = Arc::new(Mutex::new(state));
    let dispatcher = build_dispatcher(&config, Arc::clone(&state_arc));

    let notification = psst::notifiers::Notification {
        title: "Psst! 测试通知".to_string(),
        body: "如果你能看到这条消息，说明通知渠道已配置成功。".to_string(),
        provider_id: "test".to_string(),
        window_name: "test".to_string(),
    };

    println!("正在发送测试通知到所有启用的渠道...\n");

    let enabled: Vec<&str> = vec![
        if config.notifications.desktop { Some("Desktop") } else { None },
        if config.notifications.telegram.enabled { Some("Telegram") } else { None },
        if config.notifications.serverchan.enabled { Some("Server酱") } else { None },
        if config.notifications.web_push.enabled { Some("Web Push") } else { None },
    ]
    .into_iter()
    .flatten()
    .collect();

    if enabled.is_empty() {
        println!("没有启用任何通知渠道。请编辑 config.toml 启用至少一个渠道。");
        return Ok(());
    }

    println!("启用的渠道: {}", enabled.join(", "));
    dispatcher.dispatch(&notification).await;
    println!("\n发送完毕！请检查各渠道是否收到通知。");

    Ok(())
}

fn cmd_status() -> Result<()> {
    let state_path = state_path()?;
    let state = AppState::load_or_default(&state_path);

    println!("=== Psst Status ===");

    if let Some(last_check) = &state.last_check_at {
        println!("Last check: {}", last_check);
    } else {
        println!("Last check: (never)");
    }

    if state.discovered_tools.is_empty() {
        println!("Discovered tools: (none)");
    } else {
        println!("Discovered tools: {}", state.discovered_tools.join(", "));
    }

    println!();

    if state.providers.is_empty() {
        println!("No provider data yet. Run `psst run` to start monitoring.");
        return Ok(());
    }

    for (provider_id, provider_state) in &state.providers {
        println!("Provider: {}", provider_id);
        for (window_name, window) in &provider_state.windows {
            let pct = (window.utilization * 100.0).round() as u32;
            let reset_str = window
                .resets_at
                .as_deref()
                .unwrap_or("unknown");
            print!("  {}: {}% used", window_name, pct);
            if let Some(tokens) = window.used_tokens {
                print!(" ({} tokens)", tokens);
            }
            if let Some(count) = window.used_count {
                print!(" ({} requests)", count);
            }
            println!(" — resets at {}", reset_str);
        }
        println!();
    }

    Ok(())
}

fn cmd_install() -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let launch_agents_dir = home.join("Library").join("LaunchAgents");
    std::fs::create_dir_all(&launch_agents_dir)?;

    let plist_path = launch_agents_dir.join("com.psst.notify.plist");

    // Get the path to the current binary.
    let binary_path = std::env::current_exe()
        .context("Cannot determine current executable path")?
        .to_string_lossy()
        .to_string();

    let log_dir = home.join(".config").join("psst");
    let log_path = log_dir.join("psst.log");
    let err_path = log_dir.join("psst.err");

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.psst.notify</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{err}</string>
</dict>
</plist>
"#,
        binary = binary_path,
        log = log_path.display(),
        err = err_path.display(),
    );

    std::fs::write(&plist_path, &plist_content)
        .with_context(|| format!("Failed to write plist to {}", plist_path.display()))?;

    println!("Wrote LaunchAgent plist: {}", plist_path.display());

    let status = std::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()
        .context("Failed to run launchctl")?;

    if status.success() {
        println!("LaunchAgent loaded. Psst will start automatically on login.");
    } else {
        eprintln!("Warning: launchctl load returned non-zero status. You may need to run it manually.");
    }

    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let plist_path = home
        .join("Library")
        .join("LaunchAgents")
        .join("com.psst.notify.plist");

    if !plist_path.exists() {
        println!("LaunchAgent plist not found — nothing to uninstall.");
        return Ok(());
    }

    let status = std::process::Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .status()
        .context("Failed to run launchctl")?;

    if !status.success() {
        eprintln!("Warning: launchctl unload returned non-zero status.");
    }

    std::fs::remove_file(&plist_path)
        .with_context(|| format!("Failed to remove {}", plist_path.display()))?;

    println!("LaunchAgent removed: {}", plist_path.display());

    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => cmd_init(),
        Commands::Run => cmd_run(),
        Commands::Status => cmd_status(),
        Commands::TestNotify => cmd_test_notify(),
        Commands::Install => cmd_install(),
        Commands::Uninstall => cmd_uninstall(),
    }
}

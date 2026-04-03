use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;

use std::sync::Arc;

use psst::config::Config;
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

fn cmd_init() -> Result<()> {
    let config_dir = psst_config_dir()?;
    std::fs::create_dir_all(&config_dir)
        .with_context(|| format!("Failed to create config dir: {}", config_dir.display()))?;

    let cfg_path = config_dir.join("config.toml");
    let state_path = config_dir.join("state.json");

    // Write default config only if it doesn't already exist.
    if !cfg_path.exists() {
        std::fs::write(&cfg_path, Config::default_config_toml())
            .with_context(|| format!("Failed to write {}", cfg_path.display()))?;
        println!("Created config: {}", cfg_path.display());
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
    tokio::spawn(async move {
        let server = web::WebServer::new(web_bind, shared_state, access_token);
        if let Err(e) = server.run().await {
            tracing::error!("Web server error: {}", e);
        }
    });

    scheduler.run().await;

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
        Commands::Run => {
            // cmd_run creates its own tokio runtime via #[tokio::main]
            cmd_run()
        }
        Commands::Status => cmd_status(),
        Commands::Install => cmd_install(),
        Commands::Uninstall => cmd_uninstall(),
    }
}

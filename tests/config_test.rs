#[test]
fn test_parse_default_config() {
    let config = psst::config::Config::default();
    assert_eq!(config.general.check_interval_minutes, 20);
    assert!(config.general.auto_discover);
    assert_eq!(config.thresholds.usage_alerts, vec![50, 80]);
    assert_eq!(config.thresholds.reset_alerts_hours, vec![24, 12, 1]);
    assert!((config.thresholds.skip_reset_alert_above - 0.95).abs() < f64::EPSILON);
}

#[test]
fn test_parse_config_from_toml() {
    let toml_str = r#"
[general]
check_interval_minutes = 10
auto_discover = false

[thresholds]
usage_alerts = [30, 60, 90]
reset_alerts_hours = [48, 24]
skip_reset_alert_above = 0.90

[providers.claude]

[providers.cursor]
monthly_fast_requests = 500
billing_day = 15

[providers.codex]
daily_token_limit = 1000000

[notifications]
desktop = true
quiet_hours = "23:00-08:00"

[notifications.telegram]
enabled = true
bot_token = "123:ABC"
chat_id = "456"

[notifications.serverchan]
enabled = false
send_key = ""

[notifications.web_push]
enabled = true

[server]
bind = "0.0.0.0:3377"
"#;
    let config: psst::config::Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.general.check_interval_minutes, 10);
    assert!(!config.general.auto_discover);
    assert_eq!(config.thresholds.usage_alerts, vec![30, 60, 90]);
    assert_eq!(config.providers.get("cursor").unwrap().monthly_fast_requests, Some(500));
    assert_eq!(config.providers.get("cursor").unwrap().billing_day, Some(15));
    assert_eq!(config.providers.get("codex").unwrap().daily_token_limit, Some(1000000));
    assert!(config.providers.contains_key("claude"));
    assert!(config.notifications.desktop);
    assert_eq!(config.notifications.quiet_hours, Some("23:00-08:00".to_string()));
    assert!(config.notifications.telegram.enabled);
    assert_eq!(config.notifications.telegram.bot_token, "123:ABC");
    assert_eq!(config.server.bind, "0.0.0.0:3377");
}

#[test]
fn test_config_load_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, r#"
[general]
check_interval_minutes = 5

[thresholds]
usage_alerts = [80]
reset_alerts_hours = [1]

[notifications]
desktop = false

[server]
bind = "127.0.0.1:4000"
"#).unwrap();

    let config = psst::config::Config::load_from(&path).unwrap();
    assert_eq!(config.general.check_interval_minutes, 5);
    assert!(!config.notifications.desktop);
    assert_eq!(config.server.bind, "127.0.0.1:4000");
}

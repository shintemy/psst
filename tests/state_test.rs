use psst::state::{AppState, ProviderState, QuotaWindowState};

#[test]
fn test_default_state() {
    let state = AppState::default();
    assert_eq!(state.version, 1);
    assert!(state.providers.is_empty());
    assert!(state.push_subscriptions.is_empty());
    assert!(state.discovered_tools.is_empty());
}

#[test]
fn test_state_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");

    let mut state = AppState::default();
    state.discovered_tools = vec!["claude".to_string(), "cursor".to_string()];

    let mut window = QuotaWindowState::default();
    window.utilization = 0.65;
    window.resets_at = Some("2026-04-03T14:00:00Z".to_string());
    window.alerts_sent.push(50);

    let mut provider = ProviderState::default();
    provider.windows.insert("five_hour".to_string(), window);
    state.providers.insert("claude".to_string(), provider);

    state.save_atomic(&path).unwrap();

    let loaded = AppState::load_from(&path).unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.discovered_tools, vec!["claude", "cursor"]);
    let claude = loaded.providers.get("claude").unwrap();
    let five_hour = claude.windows.get("five_hour").unwrap();
    assert!((five_hour.utilization - 0.65).abs() < f64::EPSILON);
    assert_eq!(five_hour.alerts_sent, vec![50]);
}

#[test]
fn test_state_load_missing_file_returns_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.json");
    let state = AppState::load_or_default(&path);
    assert_eq!(state.version, 1);
    assert!(state.providers.is_empty());
}

#[test]
fn test_state_load_corrupted_file_returns_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, "NOT VALID JSON {{{").unwrap();
    let state = AppState::load_or_default(&path);
    assert_eq!(state.version, 1);
    assert!(dir.path().join("state.json.corrupted").exists());
}

#[test]
fn test_reset_expired_windows() {
    let mut state = AppState::default();

    let mut window = QuotaWindowState::default();
    window.utilization = 0.80;
    window.resets_at = Some("2020-01-01T00:00:00Z".to_string());
    window.alerts_sent = vec![50, 80];
    window.reset_alerts_sent = vec![24, 12];

    let mut provider = ProviderState::default();
    provider.windows.insert("five_hour".to_string(), window);
    state.providers.insert("claude".to_string(), provider);

    state.clear_expired_windows();

    let claude = state.providers.get("claude").unwrap();
    let five_hour = claude.windows.get("five_hour").unwrap();
    assert!(five_hour.alerts_sent.is_empty());
    assert!(five_hour.reset_alerts_sent.is_empty());
}

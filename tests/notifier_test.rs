use psst::notifiers::format_notification;
use psst::threshold::{AlertEvent, AlertKind};
use chrono::{Duration, Utc};

#[test]
fn test_format_usage_threshold_notification() {
    let event = AlertEvent {
        provider_id: "claude".to_string(),
        window_name: "five_hour".to_string(),
        kind: AlertKind::UsageThreshold(80),
        utilization: 0.82,
        resets_at: Some(Utc::now() + Duration::hours(2)),
    };
    let n = format_notification(&event);
    assert!(n.title.contains("Psst!"));
    assert!(n.title.contains("Claude"));
    assert!(n.title.contains("80%"));
    assert!(n.body.contains("82%"));
    assert!(n.body.contains("18%"));
}

#[test]
fn test_format_reset_countdown_notification() {
    let event = AlertEvent {
        provider_id: "claude".to_string(),
        window_name: "seven_day".to_string(),
        kind: AlertKind::ResetCountdown(12),
        utilization: 0.30,
        resets_at: Some(Utc::now() + Duration::hours(11)),
    };
    let n = format_notification(&event);
    assert!(n.title.contains("Psst!"));
    assert!(n.title.contains("12小时"));
    assert!(n.title.contains("重置"));
    assert!(n.body.contains("30%"));
    assert!(n.body.contains("充分利用"));
}

#[test]
fn test_format_cursor_notification() {
    let event = AlertEvent {
        provider_id: "cursor".to_string(),
        window_name: "monthly".to_string(),
        kind: AlertKind::UsageThreshold(50),
        utilization: 0.52,
        resets_at: Some(Utc::now() + Duration::days(12)),
    };
    let n = format_notification(&event);
    assert!(n.title.contains("Cursor"));
    assert!(n.title.contains("50%"));
}

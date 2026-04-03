use psst::threshold::{evaluate_thresholds, AlertEvent, AlertKind};
use psst::state::QuotaWindowState;
use chrono::{Duration, Utc};

#[test]
fn test_no_alerts_below_threshold() {
    let window = QuotaWindowState {
        utilization: 0.30,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds("claude", "five_hour", &window, &[50, 80], &[24, 12, 1], 0.95);
    assert!(alerts.is_empty());
}

#[test]
fn test_usage_alert_at_50_percent() {
    let window = QuotaWindowState {
        utilization: 0.55,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds("claude", "five_hour", &window, &[50, 80], &[24, 12, 1], 0.95);
    assert_eq!(alerts.len(), 1);
    assert!(matches!(alerts[0].kind, AlertKind::UsageThreshold(50)));
}

#[test]
fn test_no_duplicate_usage_alert() {
    let window = QuotaWindowState {
        utilization: 0.55,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![50],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds("claude", "five_hour", &window, &[50, 80], &[24, 12, 1], 0.95);
    assert!(alerts.is_empty());
}

#[test]
fn test_multiple_thresholds_crossed() {
    let window = QuotaWindowState {
        utilization: 0.85,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds("claude", "five_hour", &window, &[50, 80], &[24, 12, 1], 0.95);
    assert_eq!(alerts.len(), 2);
}

#[test]
fn test_reset_countdown_alert() {
    let window = QuotaWindowState {
        utilization: 0.30,
        resets_at: Some((Utc::now() + Duration::minutes(30)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds("claude", "seven_day", &window, &[50, 80], &[24, 12, 1], 0.95);
    assert_eq!(alerts.len(), 3);
    assert!(alerts.iter().all(|a| matches!(a.kind, AlertKind::ResetCountdown(_))));
}

#[test]
fn test_no_reset_alert_when_usage_above_skip_threshold() {
    let window = QuotaWindowState {
        utilization: 0.96,
        resets_at: Some((Utc::now() + Duration::minutes(30)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds("claude", "seven_day", &window, &[50, 80], &[24, 12, 1], 0.95);
    let reset_alerts: Vec<_> = alerts.iter().filter(|a| matches!(a.kind, AlertKind::ResetCountdown(_))).collect();
    assert!(reset_alerts.is_empty());
}

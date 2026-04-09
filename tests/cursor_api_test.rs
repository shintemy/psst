use psst::data_sources::cursor_api::read_cursor_tokens;
use tempfile::TempDir;

#[test]
fn test_read_cursor_tokens_from_vscdb() {
    let dir = TempDir::new().unwrap();
    let db_path = dir
        .path()
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', 'test-access-token');
         INSERT INTO ItemTable VALUES ('cursorAuth/refreshToken', 'test-refresh-token');",
    )
    .unwrap();

    let tokens = read_cursor_tokens(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(tokens.access_token, "test-access-token");
    assert_eq!(tokens.refresh_token, "test-refresh-token");
}

#[test]
fn test_read_cursor_tokens_missing_db() {
    let dir = TempDir::new().unwrap();
    let result = read_cursor_tokens(dir.path().to_str().unwrap());
    assert!(result.is_err());
}

use psst::data_sources::cursor_api::is_token_expired;

#[test]
fn test_expired_jwt() {
    // JWT with exp = 1000000000 (2001-09-09) — long expired
    // Header: {"alg":"HS256","typ":"JWT"}, Payload: {"sub":"test","exp":1000000000}
    let expired_jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiJ0ZXN0IiwiZXhwIjoxMDAwMDAwMDAwfQ.\
        signature";
    assert!(is_token_expired(expired_jwt));
}

#[test]
fn test_not_expired_jwt() {
    // JWT with exp = 4102444800 (2100-01-01) — far future
    // Payload: {"sub":"test","exp":4102444800}
    let future_jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiJ0ZXN0IiwiZXhwIjo0MTAyNDQ0ODAwfQ.\
        signature";
    assert!(!is_token_expired(future_jwt));
}

#[test]
fn test_malformed_jwt_treated_as_expired() {
    assert!(is_token_expired("not-a-jwt"));
    assert!(is_token_expired("only.two"));
    assert!(is_token_expired("a.b.c")); // invalid base64 payload
}

use psst::data_sources::cursor_api::parse_refresh_response;

#[test]
fn test_parse_refresh_response_success() {
    let body = r#"{"access_token":"new-token","id_token":"id","shouldLogout":false}"#;
    let result = parse_refresh_response(body).unwrap();
    assert_eq!(result, "new-token");
}

#[test]
fn test_parse_refresh_response_logout() {
    let body = r#"{"access_token":"new-token","shouldLogout":true}"#;
    let result = parse_refresh_response(body);
    assert!(result.is_err());
}

use psst::data_sources::cursor_api::parse_usage_response;

#[test]
fn test_parse_usage_response() {
    let body = r#"{
        "billingCycleStart": "1773133068000",
        "billingCycleEnd": "1775811468000",
        "planUsage": {
            "totalSpend": 10455,
            "includedSpend": 10455,
            "remaining": 29545,
            "limit": 40000,
            "autoPercentUsed": 0.175,
            "apiPercentUsed": 20.56,
            "totalPercentUsed": 6.97
        },
        "enabled": true,
        "displayMessage": "You've used 26% of your included usage"
    }"#;

    let usage = parse_usage_response(body).unwrap();
    assert!((usage.total_percent - 6.97).abs() < 0.01);
    assert!((usage.auto_percent - 0.175).abs() < 0.01);
    assert!((usage.api_percent - 20.56).abs() < 0.01);
    assert_eq!(usage.billing_cycle_end_ms, 1775811468000);
}

#[test]
fn test_parse_usage_response_missing_plan_usage() {
    let body = r#"{"billingCycleStart":"0","billingCycleEnd":"0","enabled":false}"#;
    let result = parse_usage_response(body);
    assert!(result.is_err());
}

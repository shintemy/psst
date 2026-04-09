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

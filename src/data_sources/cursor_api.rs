//! Cursor API usage provider.
//!
//! Reads JWT credentials from Cursor IDE's local state.vscdb,
//! refreshes expired tokens via OAuth, and calls the
//! aiserver.v1.DashboardService/GetCurrentPeriodUsage gRPC endpoint
//! for exact billing-cycle usage percentages.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// JWT credentials read from Cursor's state.vscdb.
#[derive(Debug, Clone)]
pub struct CursorTokens {
    pub access_token: String,
    pub refresh_token: String,
}

/// Path to Cursor's state.vscdb relative to a home directory.
fn vscdb_path(home_dir: &str) -> PathBuf {
    PathBuf::from(home_dir)
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
}

/// Read access and refresh tokens from Cursor IDE's local SQLite store.
pub fn read_cursor_tokens(home_dir: &str) -> Result<CursorTokens> {
    let db_path = vscdb_path(home_dir);
    if !db_path.exists() {
        anyhow::bail!("Cursor state.vscdb not found at {}", db_path.display());
    }

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("Failed to open Cursor state.vscdb: {}", db_path.display()))?;

    let access_token: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
            [],
            |row| row.get(0),
        )
        .with_context(|| "cursorAuth/accessToken not found in state.vscdb")?;

    let refresh_token: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/refreshToken'",
            [],
            |row| row.get(0),
        )
        .with_context(|| "cursorAuth/refreshToken not found in state.vscdb")?;

    Ok(CursorTokens {
        access_token,
        refresh_token,
    })
}

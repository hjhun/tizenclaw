//! Safe SQLite wrapper using rusqlite crate.
//!
//! Provides convenient helpers around rusqlite::Connection.

pub use rusqlite::{params, Connection, Error as SqliteError, Result as SqliteResult, Row};

/// Open (or create) a database with WAL mode enabled.
pub fn open_database(path: &str) -> SqliteResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;",
    )?;
    Ok(conn)
}

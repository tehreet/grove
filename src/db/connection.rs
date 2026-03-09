#![allow(dead_code)]

/// Open a SQLite connection with WAL mode and 5s busy timeout.
pub fn open_connection(path: &str) -> Result<rusqlite::Connection, rusqlite::Error> {
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA busy_timeout = 5000;
    ",
    )?;
    Ok(conn)
}

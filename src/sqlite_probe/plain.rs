//! SQLite alone, no migration library: the reference the two migration
//! variants are measured against.

use super::{ProbeResult, touch};

pub fn run(conn: &rusqlite::Connection) -> ProbeResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS probe_plain (
            id INTEGER PRIMARY KEY,
            note TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
    )?;
    println!("plain: probe_plain rows = {}", touch(conn, "probe_plain")?);
    Ok(())
}

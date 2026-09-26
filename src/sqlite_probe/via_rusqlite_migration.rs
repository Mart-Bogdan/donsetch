//! rusqlite_migration: migrations are an ordered in-code list, and the
//! applied position is stored in SQLite's `user_version` pragma.

use rusqlite_migration::{M, Migrations};

use super::{ProbeResult, touch};

const MIGRATIONS_SLICE: &[M<'_>] = &[M::up(
    "CREATE TABLE probe_rusqlite_migration (
        id INTEGER PRIMARY KEY,
        note TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );",
)];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

pub fn run(conn: &mut rusqlite::Connection) -> ProbeResult<()> {
    let before = MIGRATIONS.current_version(conn)?;
    MIGRATIONS.to_latest(conn)?;
    let after = MIGRATIONS.current_version(conn)?;
    println!("rusqlite_migration: version {before:?} -> {after:?}");
    println!(
        "rusqlite_migration: probe_rusqlite_migration rows = {}",
        touch(conn, "probe_rusqlite_migration")?
    );
    Ok(())
}

//! Throwaway binary-size probe: SQLite plus one migration library per
//! feature. Not for upstream.
//!
//! `donsetch sqlite-probe <db-path>` opens (or creates) the file, runs
//! every migrator compiled in, writes one row into each dummy table and
//! prints the row counts. Running it twice shows the second run applying
//! nothing.

mod plain;
#[cfg(feature = "refinery")]
mod via_refinery;
#[cfg(feature = "rusqlite_migration")]
mod via_rusqlite_migration;

type ProbeResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Entry point for `donsetch sqlite-probe <db-path>`; returns the exit code.
pub fn main(path: Option<&str>) -> i32 {
    let Some(path) = path else {
        eprintln!("usage: donsetch sqlite-probe <db-path>");
        return 2;
    };
    match run(std::path::Path::new(path)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("sqlite-probe: {e}");
            1
        }
    }
}

fn run(path: &std::path::Path) -> ProbeResult<()> {
    println!("sqlite {} at {}", rusqlite::version(), path.display());
    // Only the migration variants borrow `conn` mutably.
    #[allow(unused_mut)]
    let mut conn = rusqlite::Connection::open(path)?;
    plain::run(&conn)?;
    #[cfg(feature = "refinery")]
    via_refinery::run(&mut conn)?;
    #[cfg(feature = "rusqlite_migration")]
    via_rusqlite_migration::run(&mut conn)?;
    Ok(())
}

/// Insert one row into `table` and return its row count, proving the
/// table the migration created is usable.
fn touch(conn: &rusqlite::Connection, table: &str) -> ProbeResult<i64> {
    conn.execute(
        &format!("INSERT INTO {table} (note, created_at) VALUES (?1, unixepoch())"),
        ["probe"],
    )?;
    let n = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
        r.get(0)
    })?;
    Ok(n)
}

//! refinery: migrations embedded at compile time from
//! `src/sqlite_probe/refinery_migrations/`. The single migration is
//! unversioned (`U1__…`), the kind that tolerates out-of-order merges.

use super::{ProbeResult, touch};

refinery::embed_migrations!("src/sqlite_probe/refinery_migrations");

pub fn run(conn: &mut rusqlite::Connection) -> ProbeResult<()> {
    let report = migrations::runner().run(conn)?;
    let applied: Vec<String> = report
        .applied_migrations()
        .iter()
        .map(|m| m.to_string())
        .collect();
    println!("refinery: applied {} {:?}", applied.len(), applied);
    println!(
        "refinery: probe_refinery rows = {}",
        touch(conn, "probe_refinery")?
    );
    Ok(())
}

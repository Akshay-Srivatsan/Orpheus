//! Scans a folder into a scratch library and prints what Orpheus sees.
//! Usage: cargo run --release --example scan -- <music dir> <library dir> <local dir>
use orpheus_lib::{library, store::Store};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let store = Store::open(args[2].as_ref(), args[3].as_ref())?;
    let t0 = Instant::now();
    let p = library::scan(&store, &[args[1].clone()], &|_| {})?;
    println!("scan: added {} relinked {} removed {} in {:?}", p.added, p.relinked, p.removed, t0.elapsed());
    let t0 = Instant::now();
    let ids = store.needs_analysis();
    use rayon::prelude::*;
    ids.par_iter().for_each(|id| {
        if let Err(e) = library::analyze_track(&store, id) {
            eprintln!("analyze {id}: {e:#}");
        }
    });
    println!("analysis of {} tracks in {:?}", ids.len(), t0.elapsed());
    for t in store.all_tracks().iter().take(args.get(4).and_then(|n| n.parse().ok()).unwrap_or(usize::MAX)) {
        println!(
            "{} | {:<38.38} | {:<20.20} | {:>6} ms | bpm {:>6} | files {}",
            t.rec.id, t.rec.title, t.rec.artist, t.rec.duration_ms,
            t.rec.bpm.map(|b| format!("{b:.1}")).unwrap_or("-".into()), t.files.len()
        );
    }
    Ok(())
}

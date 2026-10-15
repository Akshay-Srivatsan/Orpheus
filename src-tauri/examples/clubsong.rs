//! Prints a club-file song entry (hash, length, fingerprint) for audio files,
//! and which of them are the same version.
//! Usage: clubsong <file>...
use orpheus_lib::{decode, fingerprint};

fn main() -> anyhow::Result<()> {
    let mut fps = vec![];
    for p in std::env::args().skip(1) {
        let path = std::path::Path::new(&p);
        let hash = decode::content_hash(path)?;
        let (mono, sr) = decode::decode_mono(path)?;
        let fp = fingerprint::compute(&mono, sr)?;
        let secs = (mono.len() as f64 / sr as f64).round() as i64;
        println!("# {p}\nhash = \"{hash}\"\nlength = \"{}:{:02}\"\nfingerprint = \"{}\"\n", secs / 60, secs % 60, fingerprint::encode(&fp));
        fps.push((p, fp));
    }
    for i in 0..fps.len() {
        for j in i + 1..fps.len() {
            if fingerprint::same_version(&fps[i].1, &fps[j].1) {
                eprintln!("same version: {} = {}", fps[i].0, fps[j].0);
            }
        }
    }
    Ok(())
}

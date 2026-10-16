//! Prints detected BPM and meter for audio files (developer tool).
//! Usage: meter <file>...   (BPM=<tempo> uses that tempo instead of detecting it)
fn main() -> anyhow::Result<()> {
    env_logger::init();
    for path in std::env::args().skip(1) {
        let Ok((mono, sr)) = orpheus_core::decode::decode_mono(std::path::Path::new(&path)) else {
            println!("decode failed\t{path}");
            continue;
        };
        let known = std::env::var("BPM").ok().and_then(|b| b.parse().ok());
        let Some(bpm) = known.or_else(|| orpheus_core::analysis::detect_bpm(&mono, sr)) else {
            println!("-\t-\t{path}");
            continue;
        };
        let (g, a) = orpheus_core::analysis::meter_evidence(&mono, sr, bpm);
        let m = orpheus_core::analysis::detect_meter(&mono, sr, bpm);
        let f = |x: Option<f64>| x.map(|v| format!("{v:+.2}")).unwrap_or("-".into());
        match m {
            Some(m) => println!("{bpm:.1}\t{}\t{:.1}\tgrouping {}\taccent {}\t{path}", m.beats_per_bar, m.bpm, f(g), f(a)),
            None => println!("{bpm:.1}\t?\t{bpm:.1}\tgrouping {}\taccent {}\t{path}", f(g), f(a)),
        }
    }
    Ok(())
}

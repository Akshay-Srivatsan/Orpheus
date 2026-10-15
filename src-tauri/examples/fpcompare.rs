//! Compares Orpheus's fingerprints with Chromaprint's `fpcalc`, and two songs.
//! Usage: fpcompare <file> [<other file>]   (needs fpcalc on the PATH)
use orpheus_lib::{decode, fingerprint};

fn fpcalc(path: &str, raw: bool) -> String {
    let mut c = std::process::Command::new("fpcalc");
    c.args(["-length", "0", "-plain"]);
    if raw {
        c.arg("-raw");
    }
    String::from_utf8(c.arg(path).output().expect("fpcalc").stdout).unwrap().trim().to_string()
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (mono, sr) = decode::decode_mono(std::path::Path::new(&a[1]))?;
    let t = std::time::Instant::now();
    let ours = fingerprint::compute(&mono, sr)?;
    println!("ours: {} values in {:.2} s", ours.len(), t.elapsed().as_secs_f64());
    let theirs_raw: Vec<u32> = fpcalc(&a[1], true).split(',').map(|v| v.trim().parse::<i64>().unwrap() as u32).collect();
    let decoded = fingerprint::decode(&fpcalc(&a[1], false))?;
    println!("fpcalc's text form decodes exactly: {}", decoded == theirs_raw);
    let s = fingerprint::similarity(&ours, &theirs_raw);
    println!("ours vs fpcalc: coverage {:.0}%, bit error {:.1}%, same version: {}", s.coverage * 100.0, s.error * 100.0, fingerprint::same_version(&ours, &theirs_raw));
    println!("our text form re-reads: {}", fingerprint::decode(&fingerprint::encode(&ours))? == ours);
    if let Some(b) = a.get(2) {
        let (m2, sr2) = decode::decode_mono(std::path::Path::new(b))?;
        let other = fingerprint::compute(&m2, sr2)?;
        let s = fingerprint::similarity(&ours, &other);
        println!("vs {b}: coverage {:.0}%, bit error {:.1}%, same version: {}", s.coverage * 100.0, s.error * 100.0, fingerprint::same_version(&ours, &other));
    }
    Ok(())
}

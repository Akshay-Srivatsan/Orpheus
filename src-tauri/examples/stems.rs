//! Separates one file and renders it through StemSource with each stem muted.
//! Usage: stems <audio file> <scratch dir>
use orpheus_lib::audio::source::{LocalSource, Source, StemSource};
use orpheus_lib::audio::AtomicF32;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let path = std::path::PathBuf::from(&a[1]);
    let stems = orpheus_lib::stems::Stems::new(std::path::Path::new(&a[2]));
    if !stems.model_ready() {
        stems.download_model();
        while !stems.model_ready() {
            if let orpheus_lib::stems::ModelStatus::Error { message } = stems.status().model {
                anyhow::bail!(message);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    let hash = orpheus_lib::decode::content_hash(&path)?;
    let t0 = Instant::now();
    stems.request("t", &hash, path.clone())?;
    let files = loop {
        if let Some(f) = stems.files(&hash) {
            break f;
        }
        if let Some(orpheus_lib::stems::JobStatus::Failed { message }) = stems.job("t") {
            anyhow::bail!(message);
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    println!("separated in {:?}", t0.elapsed());
    for mute in [None, Some(0), Some(1), Some(2), Some(3)] {
        let gains: Arc<[AtomicF32; 4]> = Arc::new(std::array::from_fn(|i| AtomicF32::new(if Some(i) == mute { 0.0 } else { 1.0 })));
        let main = Box::new(LocalSource::open(&path)?) as Box<dyn Source>;
        let mut s = StemSource::open(main, &files.paths, gains)?;
        s.seek(30.0)?;
        let mut out = vec![];
        while out.len() < 44100 * 2 * 20 && s.read(&mut out)? {}
        let mut orig = vec![];
        let mut o = LocalSource::open(&path)?;
        o.seek(30.0)?;
        while orig.len() < out.len() && o.read(&mut orig)? {}
        let n = out.len().min(orig.len());
        let diff: Vec<f32> = out[..n].iter().zip(&orig[..n]).map(|(a, b)| a - b).collect();
        println!(
            "mute {:<7} rms {:.4}  (original {:.4}, removed {:.4})",
            mute.map(|i| orpheus_lib::stems::STEMS[i]).unwrap_or("none"),
            rms(&out[..n]),
            rms(&orig[..n]),
            rms(&diff)
        );
    }
    Ok(())
}

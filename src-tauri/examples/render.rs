//! Renders a file through a deck (time-stretch + SRC) without an audio device
//! and reports duration/BPM of the output. Usage: render <file> <tempo> <out_rate>
use orpheus_lib::audio::source::{LocalSource, Source};
use orpheus_lib::audio::{Deck, LoadRequest};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let path = std::path::PathBuf::from(&a[1]);
    let tempo: f64 = a[2].parse()?;
    let out_rate: u32 = a[3].parse()?;
    let rate = Arc::new(AtomicU32::new(out_rate));
    let (tx, rx) = crossbeam_channel::unbounded();
    let deck = Deck::new(0, rate, Arc::new(AtomicU32::new(80)), Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    deck.shared.master_tap.gain.set(1.0);
    let p2 = path.clone();
    deck.load(LoadRequest {
        track_id: None, recording_id: None, version_id: None, entry_id: None,
        open: Box::new(move || Ok(Box::new(LocalSource::open(&p2)?) as Box<dyn Source>)),
        start_secs: 0.0, end_secs: Some(60.0), fade_at_secs: None, fade_secs: 5.0,
        tempo, gain_db: 0.0, play: true, cue_secs: None, has_stems: false, stems_on: [true; 4],
    });
    // Act as the output callback: drain the master tap as fast as possible.
    let tap = deck.shared.master_tap.clone();
    let mut out = Vec::new();
    let t0 = std::time::Instant::now();
    loop {
        if let Ok(ev) = rx.try_recv() {
            if let orpheus_lib::audio::DeckEvent::Ended { .. } = ev { break; }
            println!("event {ev:?}");
        }
        if let Some(ring) = tap.ring.lock().as_mut() {
            while let Ok(s) = ring.consumer.pop() { out.push(s); }
        }
        std::thread::sleep(Duration::from_millis(1));
        if t0.elapsed() > Duration::from_secs(60) { println!("timeout"); break; }
    }
    let _ = Ordering::Relaxed;
    let mono: Vec<f32> = out.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
    let secs = mono.len() as f64 / out_rate as f64;
    let peak = mono.iter().fold(0f32, |m, x| m.max(x.abs()));
    println!("rendered {secs:.2}s (expected {:.2}s) in {:?}, peak {peak:.3}", 60.0 / tempo, t0.elapsed());
    println!("output bpm: {:?}", orpheus_lib::analysis::detect_bpm(&mono, out_rate));
    let (src, sr) = orpheus_lib::decode::decode_mono(&path)?;
    println!("source bpm (first 60 s): {:?}", orpheus_lib::analysis::detect_bpm(&src[..(60 * sr) as usize], sr));
    Ok(())
}

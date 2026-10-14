//! Speaks a sentence with the announcer (Kokoro) into a WAV file (developer tool).
//! Usage: say <data dir> <voice> <out.wav> <text…>
//! The data dir is like ORPHEUS_DATA_DIR (the model is downloaded there if missing).
fn main() -> anyhow::Result<()> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let rt = tokio::runtime::Runtime::new()?;
    let tts = orpheus_lib::tts::Tts::new(std::path::Path::new(&a[1]), rt.handle().clone());
    let cfg = orpheus_lib::tts::TtsConfig { voice: a[2].clone(), model: "fp16".into(), speed: 1.0 };
    if tts.status(&cfg).installed_voices.iter().all(|v| v != &a[2]) {
        tts.install(&cfg)?;
    }
    let text = a[4..].join(" ");
    for round in 0..2 {
        let t = std::time::Instant::now();
        let (samples, rate) = tts.synthesize(&cfg, &format!("{text}{}", if round == 1 { " Thank you." } else { "" }), &[])?;
        println!("round {round}: {:.2} s of audio in {:.2} s", samples.len() as f64 / rate as f64, t.elapsed().as_secs_f64());
        if round == 0 {
            let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::create(&a[3], spec)?;
            for &x in samples.iter() {
                w.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16)?;
            }
            w.finalize()?;
        }
    }
    Ok(())
}

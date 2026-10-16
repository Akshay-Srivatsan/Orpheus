//! Renders the start of a song the way a deck does (512-frame blocks, key
//! lock, output at 48 kHz), for checking the output offline.
//! Usage: deckrender <file> <tempo> <start secs> <length secs> <out.wav> [engine]
use orpheus_core::audio::source::{LocalSource, Source};
use orpheus_core::audio::stretch::{create, Engine};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (path, tempo, start, len, out_path) = (&a[1], a[2].parse::<f64>()?, a[3].parse::<f64>()?, a[4].parse::<f64>()?, &a[5]);
    let engine = Engine::from_setting(a.get(6).map(|s| s.as_str()).unwrap_or("signalsmith"));
    let out_rate = 48000.0;
    let mut src = LocalSource::open(std::path::Path::new(path))?;
    let src_rate = src.sample_rate();
    src.seek(start)?;
    let mut s = create(engine, src_rate);
    s.set_pitch((src_rate as f64 / out_rate) as f32);
    let ratio = (out_rate / src_rate as f64) / tempo;
    let (mut inbuf, mut out, mut acc) = (vec![], vec![], 0.0);
    let total = (len * src_rate as f64) as usize;
    let mut fed = 0;
    while fed < total {
        while inbuf.len() < 512 * 2 && src.read(&mut inbuf)? {}
        let n = (inbuf.len() / 2).min(512);
        if n == 0 {
            break;
        }
        acc += n as f64 * ratio;
        let k = acc.floor() as usize;
        acc -= k as f64;
        let mut o = vec![0.0f32; k * 2];
        s.process(&inbuf[..n * 2], &mut o);
        inbuf.drain(..n * 2);
        out.extend(o);
        fed += n;
    }
    let spec = hound::WavSpec { channels: 2, sample_rate: out_rate as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(out_path, spec)?;
    for v in out {
        w.write_sample(v)?;
    }
    w.finalize()?;
    Ok(())
}

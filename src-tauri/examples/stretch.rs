//! Renders an excerpt with each time-stretch engine, for listening tests.
//! Usage: stretch <file> <tempo> <start secs> <length secs> <out dir>
use orpheus_lib::audio::source::{LocalSource, Source};
use orpheus_lib::audio::stretch::{create, Engine};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (path, tempo, start, len, dir) = (&a[1], a[2].parse::<f64>()?, a[3].parse::<f64>()?, a[4].parse::<f64>()?, &a[5]);
    let mut src = LocalSource::open(std::path::Path::new(path))?;
    let rate = src.sample_rate();
    src.seek(start)?;
    let mut input = vec![];
    while input.len() < (len * rate as f64) as usize * 2 && src.read(&mut input)? {}
    input.truncate((len * rate as f64) as usize * 2);
    for engine in [Engine::Signalsmith, Engine::SoundTouch, Engine::Bungee, Engine::RubberBand] {
        let mut s = create(engine, rate);
        s.set_pitch(1.0);
        let ratio = 1.0 / tempo;
        let (mut out, mut acc) = (vec![], 0.0);
        for block in input.chunks(512 * 2) {
            acc += (block.len() / 2) as f64 * ratio;
            let n = acc.floor() as usize;
            acc -= n as f64;
            let mut o = vec![0.0f32; n * 2];
            s.process(block, &mut o);
            out.extend(o);
        }
        let name = format!("{dir}/{}-{:.0}pct.wav", format!("{engine:?}").to_lowercase(), tempo * 100.0);
        let spec = hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&name, spec)?;
        for v in out {
            w.write_sample((v.clamp(-1.0, 1.0) * 32767.0) as i16)?;
        }
        w.finalize()?;
        println!("{name}");
    }
    Ok(())
}

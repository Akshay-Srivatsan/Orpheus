//! Local file decoding and content hashing (symphonia).

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, SampleBuffer};
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "aac", "alac", "ogg", "oga", "wav", "wave", "aif", "aiff", "caf", "mka", "webm",
];

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn open_format(path: &Path) -> Result<(Box<dyn FormatReader>, u32)> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions { enable_gapless: true, ..Default::default() },
        &MetadataOptions::default(),
    )?;
    let format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no audio track"))?;
    let id = track.id;
    Ok((format, id))
}

/// Identity of a recording that survives renames, moves and tag edits: a hash
/// of the compressed audio packets only (container metadata is excluded).
/// Falls back to hashing the whole file if the container can't be parsed.
pub fn content_hash(path: &Path) -> Result<String> {
    match packet_hash(path) {
        Ok(h) => Ok(h),
        Err(e) => {
            log::warn!("packet hash failed for {}: {e:#}; hashing whole file", path.display());
            let mut hasher = blake3::Hasher::new();
            let mut f = File::open(path)?;
            let mut buf = vec![0u8; 1 << 16];
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(format!("f{}", &hasher.finalize().to_hex()[..32]))
        }
    }
}

fn packet_hash(path: &Path) -> Result<String> {
    let (mut format, track_id) = open_format(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut packets = 0usize;
    loop {
        match format.next_packet() {
            Ok(p) => {
                if p.track_id() == track_id {
                    hasher.update(&p.data);
                    packets += 1;
                }
            }
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => {
                if packets == 0 {
                    return Err(e.into());
                }
                break;
            }
        }
    }
    if packets == 0 {
        return Err(anyhow!("no packets"));
    }
    Ok(format!("a{}", &hasher.finalize().to_hex()[..32]))
}

/// Streaming decoder producing interleaved stereo f32 at the file's sample rate.
pub struct LocalDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    pub sample_rate: u32,
    pub duration_secs: Option<f64>,
    sample_buf: Option<SampleBuffer<f32>>,
    /// Frames to drop after a seek (symphonia seeks to packet boundaries).
    skip_frames: u64,
    /// Position (in frames) of the next frame to be returned.
    pub position_frames: u64,
}

impl LocalDecoder {
    pub fn open(path: &Path) -> Result<Self> {
        let (format, track_id) = open_format(path)?;
        let track = format.tracks().iter().find(|t| t.id == track_id).unwrap();
        let params = &track.codec_params;
        let sample_rate = params.sample_rate.ok_or_else(|| anyhow!("unknown sample rate"))?;
        let duration_secs = params.n_frames.map(|n| n as f64 / sample_rate as f64);
        let decoder = symphonia::default::get_codecs().make(params, &DecoderOptions::default())?;
        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            duration_secs,
            sample_buf: None,
            skip_frames: 0,
            position_frames: 0,
        })
    }

    pub fn position_secs(&self) -> f64 {
        self.position_frames as f64 / self.sample_rate as f64
    }

    pub fn seek(&mut self, secs: f64) -> Result<()> {
        let secs = secs.max(0.0);
        let target_frame = (secs * self.sample_rate as f64) as u64;
        let seeked = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time { time: Time::from(secs), track_id: Some(self.track_id) },
        );
        match seeked {
            Ok(s) => {
                self.decoder.reset();
                self.skip_frames = target_frame.saturating_sub(s.actual_ts);
                self.position_frames = s.actual_ts;
                Ok(())
            }
            Err(e) => Err(anyhow!("seek failed: {e}")),
        }
    }

    /// Appends decoded stereo frames to `out`. Returns `false` at end of stream.
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(SymError::ResetRequired) => return Ok(false),
                Err(e) => return Err(e.into()),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(SymError::DecodeError(e)) => {
                    log::debug!("decode error (skipping packet): {e}");
                    continue;
                }
                // A truncated final frame: treat as the end of the stream.
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(e) => return Err(e.into()),
            };
            Self::push_decoded(
                decoded,
                out,
                &mut self.sample_buf,
                &mut self.skip_frames,
                &mut self.position_frames,
            );
            return Ok(true);
        }
    }

    fn push_decoded(
        decoded: AudioBufferRef,
        out: &mut Vec<f32>,
        sample_buf: &mut Option<SampleBuffer<f32>>,
        skip_frames: &mut u64,
        position_frames: &mut u64,
    ) {
        let spec = *decoded.spec();
        let frames = decoded.frames();
        if sample_buf.as_ref().map(|b| b.capacity() < frames * spec.channels.count()).unwrap_or(true) {
            *sample_buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        let ch = spec.channels.count().max(1);
        let mut start = 0usize;
        if *skip_frames > 0 {
            let skip = (*skip_frames as usize).min(frames);
            *skip_frames -= skip as u64;
            *position_frames += skip as u64;
            start = skip;
        }
        out.reserve((frames - start) * 2);
        for f in start..frames {
            let i = f * ch;
            if ch == 1 {
                out.push(samples[i]);
                out.push(samples[i]);
            } else {
                out.push(samples[i]);
                out.push(samples[i + 1]);
            }
        }
        *position_frames += (frames - start) as u64;
    }
}

/// Decodes a whole file to mono at the native rate (for analysis).
pub fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut dec = LocalDecoder::open(path)?;
    let mut stereo = Vec::new();
    let mut mono = Vec::with_capacity(dec.duration_secs.map(|d| (d * dec.sample_rate as f64) as usize).unwrap_or(0));
    while dec.read(&mut stereo)? {
        for fr in stereo.chunks_exact(2) {
            mono.push((fr[0] + fr[1]) * 0.5);
        }
        stereo.clear();
    }
    Ok((mono, dec.sample_rate))
}

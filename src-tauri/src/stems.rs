//! Stem separation (vocals / drums / bass / other) using `stem-splitter-core`
//! (HT-Demucs on ONNX Runtime).
//!
//! Separation is much slower than real time on a CPU (roughly a minute per
//! song), so stems are prepared in the background, one song at a time at low
//! thread priority, and cached per recording (content hash) in the
//! machine-local data folder. Playback mixes them instantly
//! (`audio::source::StemSource`). Only vocals, drums and bass are kept; "other"
//! is the remainder of the original, which saves a quarter of the space.
//!
//! The library decodes its input itself; to make the stems line up
//! sample-for-sample with what the deck plays, the song is first decoded with
//! Orpheus' own decoder into a temporary WAV, which is what gets separated.
//! Preparing stems needs an internet connection (the library checks its model
//! manifest online); playing prepared stems does not.

use crate::decode::LocalDecoder;
use crate::model::Id;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Stem order used everywhere in Orpheus (pads 1–4, gains, files).
pub const STEMS: [&str; 4] = ["vocals", "drums", "bass", "other"];

/// stem-splitter-core's default model (HT-Demucs, about 210 MB).
const MODEL: &str = "htdemucs_ort_v1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ModelStatus {
    Missing,
    Downloading { done: u64, total: u64 },
    Ready,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum JobStatus {
    Queued,
    Running { percent: f32 },
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemsStatus {
    pub model: ModelStatus,
    /// Track id → job state (finished jobs are removed).
    pub jobs: HashMap<Id, JobStatus>,
    pub cache_bytes: u64,
    pub cache_dir: String,
}

/// Stems stored on disk; "other" is derived as original − vocals − drums − bass.
pub const STORED: [&str; 3] = ["vocals", "drums", "bass"];

/// Paths of a recording's stored stems, in `STORED` order.
#[derive(Debug, Clone)]
pub struct StemFiles {
    pub paths: [PathBuf; 3],
}

enum Job {
    DownloadModel,
    Separate { track_id: Id, hash: String, path: PathBuf },
}

struct Inner {
    model: ModelStatus,
    jobs: HashMap<Id, JobStatus>,
    running: Option<Id>,
}

type ChangeFn = Box<dyn Fn(Option<Id>) + Send>;

pub struct Stems {
    dir: PathBuf,
    inner: Arc<Mutex<Inner>>,
    tx: Sender<Job>,
    /// Called with the track id when a job changes state (`None` = model status).
    on_change: Arc<Mutex<Option<ChangeFn>>>,
}

impl Stems {
    pub fn new(data_dir: &Path) -> Arc<Self> {
        let dir = data_dir.join("stems");
        let _ = std::fs::create_dir_all(&dir);
        // Remembered locally; if the library's model cache is cleared it just
        // downloads again the next time stems are prepared.
        let model = if dir.join(".model-ready").exists() { ModelStatus::Ready } else { ModelStatus::Missing };
        let (tx, rx) = unbounded();
        let s = Arc::new(Self {
            dir,
            inner: Arc::new(Mutex::new(Inner { model, jobs: HashMap::new(), running: None })),
            tx,
            on_change: Arc::new(Mutex::new(None)),
        });
        let w = s.clone();
        std::thread::Builder::new()
            .name("stems".into())
            .spawn(move || w.worker(rx))
            .expect("spawn stems thread");
        s
    }

    pub fn set_on_change(&self, f: impl Fn(Option<Id>) + Send + 'static) {
        *self.on_change.lock() = Some(Box::new(f));
    }

    fn changed(&self, id: Option<Id>) {
        if let Some(f) = self.on_change.lock().as_ref() {
            f(id);
        }
    }

    fn stem_dir(&self, hash: &str) -> PathBuf {
        self.dir.join(hash)
    }

    /// The cached stems for a recording, if complete.
    pub fn files(&self, hash: &str) -> Option<StemFiles> {
        let d = self.stem_dir(hash);
        let paths = STORED.map(|s| d.join(format!("{s}.wav")));
        (d.join("complete").exists() && paths.iter().all(|p| p.exists())).then_some(StemFiles { paths })
    }

    pub fn delete(&self, hash: &str) -> Result<()> {
        let d = self.stem_dir(hash);
        if d.exists() {
            std::fs::remove_dir_all(&d)?;
        }
        Ok(())
    }

    pub fn status(&self) -> StemsStatus {
        let i = self.inner.lock();
        StemsStatus {
            model: i.model.clone(),
            jobs: i.jobs.clone(),
            cache_bytes: dir_size(&self.dir),
            cache_dir: self.dir.to_string_lossy().to_string(),
        }
    }

    pub fn job(&self, track_id: &str) -> Option<JobStatus> {
        self.inner.lock().jobs.get(track_id).cloned()
    }

    pub fn model_ready(&self) -> bool {
        matches!(self.inner.lock().model, ModelStatus::Ready)
    }

    pub fn download_model(&self) {
        {
            let mut i = self.inner.lock();
            if matches!(i.model, ModelStatus::Ready | ModelStatus::Downloading { .. }) {
                return;
            }
            i.model = ModelStatus::Downloading { done: 0, total: 0 };
        }
        let _ = self.tx.send(Job::DownloadModel);
        self.changed(None);
    }

    /// Queues separation of a local file (no-op if done or already queued).
    pub fn request(&self, track_id: &str, hash: &str, path: PathBuf) -> Result<()> {
        if self.files(hash).is_some() {
            return Ok(());
        }
        {
            let mut i = self.inner.lock();
            if !matches!(i.model, ModelStatus::Ready) {
                return Err(anyhow!("Download the stem separation model first (Settings → Stems)"));
            }
            if matches!(i.jobs.get(track_id), Some(JobStatus::Queued | JobStatus::Running { .. })) {
                return Ok(());
            }
            i.jobs.insert(track_id.to_string(), JobStatus::Queued);
        }
        let _ = self.tx.send(Job::Separate { track_id: track_id.to_string(), hash: hash.to_string(), path });
        self.changed(Some(track_id.to_string()));
        Ok(())
    }

    fn worker(self: Arc<Self>, rx: Receiver<Job>) {
        // Separation saturates the CPU: keep it below the audio threads.
        // ONNX Runtime's threads are created from this thread and inherit it.
        if let Err(e) = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Min) {
            log::warn!("could not lower the stem thread's priority: {e:?}");
        }
        let (inner, changed) = (self.inner.clone(), self.on_change.clone());
        stem_splitter_core::set_download_progress_callback(move |done, total| {
            inner.lock().model = ModelStatus::Downloading { done, total };
            if done == total || done % (8 << 20) < 65536 {
                if let Some(f) = changed.lock().as_ref() {
                    f(None);
                }
            }
        });
        let (inner, changed) = (self.inner.clone(), self.on_change.clone());
        stem_splitter_core::set_split_progress_callback(move |p| {
            if let stem_splitter_core::SplitProgress::Chunks { percent, .. } = p {
                let id = {
                    let mut i = inner.lock();
                    let id = i.running.clone();
                    if let Some(id) = &id {
                        i.jobs.insert(id.clone(), JobStatus::Running { percent });
                    }
                    id
                };
                if let Some(f) = changed.lock().as_ref() {
                    f(id);
                }
            }
        });
        while let Ok(job) = rx.recv() {
            match job {
                Job::DownloadModel => {
                    let r = stem_splitter_core::prepare_model(MODEL, None).map_err(|e| anyhow!("{e}"));
                    self.inner.lock().model = match r {
                        Ok(()) => {
                            let _ = std::fs::write(self.dir.join(".model-ready"), MODEL);
                            ModelStatus::Ready
                        }
                        Err(e) => ModelStatus::Error { message: format!("{e:#}") },
                    };
                    self.changed(None);
                }
                Job::Separate { track_id, hash, path } => {
                    {
                        let mut i = self.inner.lock();
                        i.running = Some(track_id.clone());
                        i.jobs.insert(track_id.clone(), JobStatus::Running { percent: 0.0 });
                    }
                    self.changed(Some(track_id.clone()));
                    let r = self.separate(&hash, &path);
                    {
                        let mut i = self.inner.lock();
                        i.running = None;
                        match &r {
                            Ok(()) => {
                                i.jobs.remove(&track_id);
                            }
                            Err(e) => {
                                log::warn!("stem separation of {track_id} failed: {e:#}");
                                i.jobs.insert(track_id.clone(), JobStatus::Failed { message: format!("{e:#}") });
                            }
                        }
                    }
                    self.changed(Some(track_id));
                }
            }
        }
    }

    fn separate(&self, hash: &str, path: &Path) -> Result<()> {
        let work = self.dir.join(format!(".{hash}.tmp"));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work)?;
        let result = (|| -> Result<()> {
            // Decode exactly as playback does, so the stems line up with the deck.
            let input = work.join("input.wav");
            let rate = decode_to_wav(path, &input)?;
            let out = stem_splitter_core::split_file(
                &input.to_string_lossy(),
                stem_splitter_core::SplitOptions {
                    output_dir: work.to_string_lossy().to_string(),
                    model_name: MODEL.into(),
                    manifest_url_override: None,
                },
            )
            .map_err(|e| anyhow!("{e}"))?;
            // Keep vocals, drums and bass under fixed names; "other" is derived.
            for (name, wav) in STORED.iter().zip([&out.vocals_path, &out.drums_path, &out.bass_path]) {
                std::fs::rename(wav, work.join(format!("{name}.wav")))?;
            }
            let _ = std::fs::remove_file(&out.other_path);
            let _ = std::fs::remove_file(&input);
            std::fs::write(work.join("complete"), format!("{{\"rate\":{rate}}}\n"))?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&work);
            return Err(e);
        }
        let dest = self.stem_dir(hash);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&work, &dest).context("saving stems")?;
        Ok(())
    }
}

/// Decodes `src` with the playback decoder into a 32-bit float stereo WAV.
fn decode_to_wav(src: &Path, dest: &Path) -> Result<u32> {
    let mut dec = LocalDecoder::open(src)?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: dec.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dest, spec)?;
    let mut buf = Vec::new();
    while dec.read(&mut buf)? {
        for &s in &buf {
            w.write_sample(s)?;
        }
        buf.clear();
    }
    w.finalize()?;
    Ok(dec.sample_rate)
}

fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_wav_matches_playback_decoder() {
        // The separation input must be sample-identical to what the deck decodes.
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../music");
        let Some(file) = walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .find(|p| crate::decode::is_audio_file(p))
        else {
            return; // no sample music on this machine
        };
        let dir = std::env::temp_dir().join(format!("orpheus-stems-{}", crate::model::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("in.wav");
        decode_to_wav(&file, &wav).unwrap();
        let mut a = LocalDecoder::open(&file).unwrap();
        let mut b = LocalDecoder::open(&wav).unwrap();
        let (mut va, mut vb) = (vec![], vec![]);
        while a.read(&mut va).unwrap() && va.len() < 44100 * 20 {}
        while b.read(&mut vb).unwrap() && vb.len() < va.len() {}
        let n = va.len().min(vb.len());
        assert!(n > 44100);
        assert!(va[..n].iter().zip(&vb[..n]).all(|(x, y)| (x - y).abs() < 1e-6));
        let _ = std::fs::remove_dir_all(dir);
    }
}

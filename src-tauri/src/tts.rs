//! Spoken announcements with Piper (https://github.com/rhasspy/piper), an
//! open-source neural text-to-speech engine.
//!
//! Piper runs as a separate program: Orpheus downloads the release for this
//! platform and a voice (from Piper's voice catalogue) into the machine-local
//! data folder on first use, then runs `piper --output_raw` with the text on
//! stdin and reads 16-bit mono PCM from stdout. A Piper executable and voice
//! you already have can be used instead (settings `piperPath`, `voicePath`).
//! Installing needs an internet connection; speaking does not.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Piper release (standalone builds with their libraries and espeak-ng data).
const PIPER_RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";
/// Piper's voice catalogue: voices.json, and the files it lists.
const VOICES: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum TtsState {
    Missing,
    Installing { done: u64, total: u64, what: String },
    Ready,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsStatus {
    #[serde(flatten)]
    pub state: TtsState,
    /// The voice that is installed and in use.
    pub voice: String,
    /// Voices downloaded on this computer.
    pub installed_voices: Vec<String>,
    pub dir: String,
}

/// A voice from Piper's catalogue.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub key: String,
    pub name: String,
    pub language: String,
    pub quality: String,
    pub size_bytes: u64,
}

/// Where Piper and the voice come from (settings).
#[derive(Debug, Clone, Default)]
pub struct TtsConfig {
    pub voice: String,
    pub piper_path: String,
    pub voice_path: String,
    /// Speech rate: >1 slower, <1 faster (Piper's length scale).
    pub length_scale: f64,
}

type ChangeFn = Box<dyn Fn() + Send>;

pub struct Tts {
    dir: PathBuf,
    rt: tokio::runtime::Handle,
    http: reqwest::Client,
    state: Mutex<TtsState>,
    on_change: Mutex<Option<ChangeFn>>,
    /// Recent clips (text → samples, rate), so a repeated announcement is instant.
    cache: Mutex<HashMap<String, (Arc<Vec<f32>>, u32)>>,
}

impl Tts {
    pub fn new(data_dir: &Path, rt: tokio::runtime::Handle) -> Arc<Self> {
        let dir = data_dir.join("tts");
        Arc::new(Self {
            dir,
            rt,
            http: reqwest::Client::new(),
            state: Mutex::new(TtsState::Missing),
            on_change: Mutex::new(None),
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn on_change(&self, f: ChangeFn) {
        *self.on_change.lock() = Some(f);
    }

    fn changed(&self) {
        if let Some(f) = self.on_change.lock().as_ref() {
            f();
        }
    }

    fn set_state(&self, s: TtsState) {
        *self.state.lock() = s;
        self.changed();
    }

    pub fn status(&self, cfg: &TtsConfig) -> TtsStatus {
        let mut state = self.state.lock().clone();
        if matches!(state, TtsState::Missing | TtsState::Ready) {
            state = if self.executable(cfg).is_some() && self.voice_model(cfg).is_some() { TtsState::Ready } else { TtsState::Missing };
        }
        TtsStatus { state, voice: cfg.voice.clone(), installed_voices: self.installed_voices(), dir: self.dir.display().to_string() }
    }

    fn piper_dir(&self) -> PathBuf {
        self.dir.join("piper")
    }

    fn voices_dir(&self) -> PathBuf {
        self.dir.join("voices")
    }

    fn executable(&self, cfg: &TtsConfig) -> Option<PathBuf> {
        if !cfg.piper_path.trim().is_empty() {
            return Some(PathBuf::from(cfg.piper_path.trim())).filter(|p| p.exists());
        }
        let exe = self.piper_dir().join(if cfg!(windows) { "piper.exe" } else { "piper" });
        exe.exists().then_some(exe)
    }

    fn voice_model(&self, cfg: &TtsConfig) -> Option<PathBuf> {
        if !cfg.voice_path.trim().is_empty() {
            return Some(PathBuf::from(cfg.voice_path.trim())).filter(|p| p.exists());
        }
        let p = self.voices_dir().join(format!("{}.onnx", cfg.voice));
        (p.exists() && p.with_extension("onnx.json").exists()).then_some(p)
    }

    fn installed_voices(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.voices_dir())
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".onnx")).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Piper's voice catalogue.
    pub fn voices(&self) -> Result<Vec<VoiceInfo>> {
        let catalogue = self.catalogue()?;
        let mut out: Vec<VoiceInfo> = catalogue
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(key, v)| VoiceInfo {
                        key: key.clone(),
                        name: v["name"].as_str().unwrap_or(key).to_string(),
                        language: format!(
                            "{} ({})",
                            v["language"]["name_english"].as_str().unwrap_or(""),
                            v["language"]["country_english"].as_str().unwrap_or("")
                        ),
                        quality: v["quality"].as_str().unwrap_or("").to_string(),
                        size_bytes: v["files"]
                            .as_object()
                            .map(|f| f.iter().filter(|(p, _)| p.ends_with(".onnx")).filter_map(|(_, x)| x["size_bytes"].as_u64()).sum())
                            .unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| (&a.language, &a.name, &a.quality).cmp(&(&b.language, &b.name, &b.quality)));
        Ok(out)
    }

    fn catalogue(&self) -> Result<serde_json::Value> {
        let path = self.dir.join("voices.json");
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str(&text) {
                return Ok(v);
            }
        }
        let text = self.rt.block_on(async {
            let r = self.http.get(format!("{VOICES}/voices.json")).send().await?.error_for_status()?;
            r.text().await
        })?;
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(&path, &text)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Downloads Piper (if needed) and the configured voice.
    pub fn install(&self, cfg: &TtsConfig) -> Result<()> {
        let r = self.install_inner(cfg);
        match &r {
            Ok(()) => self.set_state(TtsState::Ready),
            Err(e) => self.set_state(TtsState::Error { message: format!("{e:#}") }),
        }
        r
    }

    fn install_inner(&self, cfg: &TtsConfig) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        if self.executable(cfg).is_none() {
            let asset = piper_asset().ok_or_else(|| anyhow!("Piper has no build for this platform; set a Piper executable in Settings"))?;
            let archive = self.dir.join(asset);
            self.download(&format!("{PIPER_RELEASE}/{asset}"), &archive, "Piper")?;
            let _ = std::fs::remove_dir_all(self.piper_dir());
            extract(&archive, &self.dir).context("unpacking Piper")?;
            let _ = std::fs::remove_file(&archive);
            if self.executable(cfg).is_none() {
                return Err(anyhow!("Piper's archive didn't contain the program"));
            }
        }
        if self.voice_model(cfg).is_none() {
            let catalogue = self.catalogue()?;
            let files = catalogue[cfg.voice.as_str()]["files"]
                .as_object()
                .ok_or_else(|| anyhow!("Unknown Piper voice “{}”", cfg.voice))?
                .keys()
                .filter(|p| p.ends_with(".onnx") || p.ends_with(".onnx.json"))
                .cloned()
                .collect::<Vec<_>>();
            std::fs::create_dir_all(self.voices_dir())?;
            for f in files {
                let ext = if f.ends_with(".onnx.json") { "onnx.json" } else { "onnx" };
                let dest = self.voices_dir().join(format!("{}.{ext}", cfg.voice));
                self.download(&format!("{VOICES}/{f}"), &dest, &format!("voice {}", cfg.voice))?;
            }
        }
        Ok(())
    }

    fn download(&self, url: &str, dest: &Path, what: &str) -> Result<()> {
        let tmp = dest.with_extension("part");
        self.rt.block_on(async {
            let mut r = self.http.get(url).send().await?.error_for_status()?;
            let total = r.content_length().unwrap_or(0);
            let mut f = std::fs::File::create(&tmp)?;
            let mut done = 0u64;
            while let Some(chunk) = r.chunk().await? {
                f.write_all(&chunk)?;
                done += chunk.len() as u64;
                self.set_state(TtsState::Installing { done, total, what: what.to_string() });
            }
            anyhow::Ok(())
        })
        .with_context(|| format!("downloading {what}"))?;
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }

    /// Deletes Piper and the downloaded voices.
    pub fn remove(&self) -> Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        self.cache.lock().clear();
        self.set_state(TtsState::Missing);
        Ok(())
    }

    /// Speaks `text`: mono samples and their sample rate.
    pub fn synthesize(&self, cfg: &TtsConfig, text: &str) -> Result<(Arc<Vec<f32>>, u32)> {
        let key = format!("{}|{}|{}|{text}", cfg.voice, cfg.voice_path, cfg.length_scale);
        if let Some(hit) = self.cache.lock().get(&key) {
            return Ok(hit.clone());
        }
        let exe = self.executable(cfg).ok_or_else(|| anyhow!("Piper isn't installed (Settings → Auto Mode)"))?;
        let model = self.voice_model(cfg).ok_or_else(|| anyhow!("The voice isn't installed (Settings → Auto Mode)"))?;
        let rate = std::fs::read_to_string(model.with_extension("onnx.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["audio"]["sample_rate"].as_u64())
            .unwrap_or(22050) as u32;
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("--model").arg(&model).arg("--output_raw").arg("--quiet");
        if cfg.length_scale > 0.0 && (cfg.length_scale - 1.0).abs() > 1e-3 {
            cmd.arg("--length_scale").arg(format!("{:.2}", cfg.length_scale));
        }
        if let Some(dir) = exe.parent() {
            // The standalone build keeps its libraries and espeak-ng data next to it.
            cmd.current_dir(dir);
        }
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        background_priority(&mut cmd);
        let mut child = cmd.spawn().with_context(|| format!("starting {}", exe.display()))?;
        child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?.write_all(format!("{}\n", text.replace('\n', " ")).as_bytes())?;
        let mut raw = vec![];
        child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?.read_to_end(&mut raw)?;
        let out = child.wait_with_output()?;
        if !out.status.success() || raw.len() < 2 {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Piper failed: {}", err.lines().last().unwrap_or("no output")));
        }
        let samples: Vec<f32> = raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        let clip = (Arc::new(samples), rate);
        let mut cache = self.cache.lock();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, clip.clone());
        Ok(clip)
    }
}

/// Piper's standalone build for this platform.
fn piper_asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("piper_linux_x86_64.tar.gz"),
        ("linux", "aarch64") => Some("piper_linux_aarch64.tar.gz"),
        ("macos", "x86_64") => Some("piper_macos_x64.tar.gz"),
        ("macos", "aarch64") => Some("piper_macos_aarch64.tar.gz"),
        ("windows", "x86_64") => Some("piper_windows_amd64.zip"),
        _ => None,
    }
}

fn extract(archive: &Path, into: &Path) -> Result<()> {
    let f = std::fs::File::open(archive)?;
    if archive.extension().is_some_and(|e| e == "zip") {
        zip::ZipArchive::new(f)?.extract(into)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(f)).unpack(into)?;
    }
    Ok(())
}

/// Synthesis is short but CPU-heavy: keep it below the audio threads, and
/// without a console window on Windows.
fn background_priority(cmd: &mut std::process::Command) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: nice() is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::nice(10);
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

//! Spoken announcements with Kokoro (https://huggingface.co/hexgrad/Kokoro-82M),
//! an open-weights neural text-to-speech model, run in-process on ONNX Runtime
//! (the same runtime as stem separation).
//!
//! Text → phonemes: English with misaki (Kokoro's own phonemizer, `misaki-rs`),
//! unknown English words and the other languages with espeak-ng (the pure-Rust
//! port, with its data bundled). Phonemes → token ids (Kokoro's vocabulary) →
//! the model, with the voice's style vector for that length → 24 kHz audio.
//!
//! The model (~160 MB, or ~325 MB at full precision), its vocabulary and the
//! chosen voice (~0.5 MB) are downloaded on first use into the machine-local
//! data folder. Installing needs an internet connection; speaking does not.

use crate::model::Pronunciation;
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Kokoro v1.0 in ONNX form (model variants, tokenizer, voices).
const REPO: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX";
const SAMPLE_RATE: u32 = 24_000;
/// Kokoro's longest input (tokens, excluding the two padding tokens).
const MAX_TOKENS: usize = 510;
/// The loaded model is released after this long without speaking.
const IDLE_UNLOAD: Duration = Duration::from_secs(15 * 60);
/// ONNX Runtime threads for synthesis (at low priority).
const THREADS: usize = 4;

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
    /// The voice in use.
    pub voice: String,
    /// Voices downloaded on this computer.
    pub installed_voices: Vec<String>,
    pub dir: String,
}

/// A Kokoro voice.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub key: String,
    pub name: String,
    pub language: String,
    pub gender: String,
}

/// Model variant and voice (settings).
#[derive(Debug, Clone, Default)]
pub struct TtsConfig {
    pub voice: String,
    /// `fp16` (default, ~160 MB) or `fp32` (~325 MB).
    pub model: String,
    /// Speech rate: 1 = normal, >1 faster.
    pub speed: f64,
}

impl TtsConfig {
    fn model_file(&self) -> &'static str {
        if self.model == "fp32" { "model.onnx" } else { "model_fp16.onnx" }
    }
}

/// How a voice's text becomes phonemes: Kokoro's voice prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Lang {
    EnUs,
    EnGb,
    /// Other languages via espeak-ng (its language code).
    Espeak(&'static str),
}

/// Kokoro's voice prefix → (language, espeak code, display name). Japanese and
/// Mandarin voices need phonemizers Orpheus doesn't have, so they're left out.
const LANGUAGES: &[(char, Lang, &str)] = &[
    ('a', Lang::EnUs, "English (US)"),
    ('b', Lang::EnGb, "English (UK)"),
    ('e', Lang::Espeak("es"), "Spanish"),
    ('f', Lang::Espeak("fr"), "French"),
    ('i', Lang::Espeak("it"), "Italian"),
    ('p', Lang::Espeak("pt-br"), "Portuguese (Brazil)"),
    ('h', Lang::Espeak("hi"), "Hindi"),
];

/// Whether `key` names a Kokoro voice Orpheus can use (`af_heart`, `bm_george`, …).
pub fn is_voice(key: &str) -> bool {
    let b = key.as_bytes();
    b.len() > 3 && matches!(b[1], b'f' | b'm') && b[2] == b'_' && b[3..].iter().all(|c| c.is_ascii_lowercase()) && voice_lang(key).is_some()
}

fn voice_lang(voice: &str) -> Option<(Lang, &'static str)> {
    let c = voice.chars().next()?;
    LANGUAGES.iter().find(|(p, ..)| *p == c).map(|(_, l, n)| (*l, *n))
}

struct Loaded {
    model_file: &'static str,
    session: ort::session::Session,
    vocab: HashMap<char, i64>,
    /// Style vectors per voice: rows of 256 floats, one per input length.
    voices: HashMap<String, Vec<f32>>,
    last_used: Instant,
}

type ChangeFn = Box<dyn Fn() + Send>;

pub struct Tts {
    dir: PathBuf,
    rt: tokio::runtime::Handle,
    http: reqwest::Client,
    state: Mutex<TtsState>,
    on_change: Mutex<Option<ChangeFn>>,
    loaded: Mutex<Option<Loaded>>,
    /// Phonemizers by language (independent of the model).
    phonemizers: Mutex<HashMap<Lang, Arc<Phonemizer>>>,
    /// Recent clips (text → samples), so a repeated announcement is instant.
    cache: Mutex<HashMap<String, Arc<Vec<f32>>>>,
}

impl Tts {
    pub fn new(data_dir: &Path, rt: tokio::runtime::Handle) -> Arc<Self> {
        // Earlier versions downloaded the Piper program and voices here.
        for old in ["piper", "voices", "voices.json"] {
            let p = data_dir.join("tts").join(old);
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else if p.exists() {
                let _ = std::fs::remove_file(&p);
            }
        }
        Arc::new(Self {
            dir: data_dir.join("tts").join("kokoro"),
            rt,
            http: reqwest::Client::new(),
            state: Mutex::new(TtsState::Missing),
            on_change: Mutex::new(None),
            loaded: Mutex::new(None),
            phonemizers: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn on_change(&self, f: ChangeFn) {
        *self.on_change.lock() = Some(f);
    }

    fn set_state(&self, s: TtsState) {
        *self.state.lock() = s;
        if let Some(f) = self.on_change.lock().as_ref() {
            f();
        }
    }

    fn voices_dir(&self) -> PathBuf {
        self.dir.join("voices")
    }

    fn installed(&self, cfg: &TtsConfig) -> bool {
        self.dir.join(cfg.model_file()).exists()
            && self.dir.join("tokenizer.json").exists()
            && self.voices_dir().join(format!("{}.bin", cfg.voice)).exists()
    }

    pub fn status(&self, cfg: &TtsConfig) -> TtsStatus {
        let mut state = self.state.lock().clone();
        if matches!(state, TtsState::Missing | TtsState::Ready) {
            state = if self.installed(cfg) { TtsState::Ready } else { TtsState::Missing };
        }
        let mut installed_voices: Vec<String> = std::fs::read_dir(self.voices_dir())
            .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".bin").map(String::from)).collect())
            .unwrap_or_default();
        installed_voices.sort();
        TtsStatus { state, voice: cfg.voice.clone(), installed_voices, dir: self.dir.display().to_string() }
    }

    /// Kokoro's voices (those Orpheus can phonemize).
    pub fn voices(&self) -> Result<Vec<VoiceInfo>> {
        let path = self.dir.join("voice-list.json");
        let names: Vec<String> = match std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            Some(n) => n,
            None => {
                let listing: serde_json::Value = self.rt.block_on(async {
                    let url = REPO.replace("huggingface.co/", "huggingface.co/api/models/") + "/tree/main/voices";
                    self.http.get(url).send().await?.error_for_status()?.json().await
                })?;
                let names: Vec<String> = listing
                    .as_array()
                    .map(|a| a.iter().filter_map(|f| f["path"].as_str()?.strip_prefix("voices/")?.strip_suffix(".bin").map(String::from)).collect())
                    .unwrap_or_default();
                std::fs::create_dir_all(&self.dir)?;
                std::fs::write(&path, serde_json::to_string(&names)?)?;
                names
            }
        };
        Ok(names
            .into_iter()
            .filter_map(|key| {
                let (_, language) = voice_lang(&key)?;
                let (prefix, name) = key.split_once('_')?;
                let gender = match prefix.chars().nth(1) {
                    Some('f') => "female",
                    Some('m') => "male",
                    _ => return None,
                };
                let mut n = name.to_string();
                n[..1].make_ascii_uppercase();
                Some(VoiceInfo { name: n, language: language.into(), gender: gender.into(), key })
            })
            .collect())
    }

    /// Downloads the model, its vocabulary and the configured voice.
    pub fn install(&self, cfg: &TtsConfig) -> Result<()> {
        let r = (|| -> Result<()> {
            std::fs::create_dir_all(self.voices_dir())?;
            voice_lang(&cfg.voice).ok_or_else(|| anyhow!("Unsupported Kokoro voice “{}”", cfg.voice))?;
            for (file, what) in [("tokenizer.json", "vocabulary"), (cfg.model_file(), "Kokoro model")] {
                let dest = self.dir.join(file);
                if !dest.exists() {
                    let sub = if file.ends_with(".onnx") { "onnx/" } else { "" };
                    self.download(&format!("{REPO}/resolve/main/{sub}{file}"), &dest, what)?;
                }
            }
            let voice = self.voices_dir().join(format!("{}.bin", cfg.voice));
            if !voice.exists() {
                self.download(&format!("{REPO}/resolve/main/voices/{}.bin", cfg.voice), &voice, &format!("voice {}", cfg.voice))?;
            }
            Ok(())
        })();
        match &r {
            Ok(()) => self.set_state(TtsState::Ready),
            Err(e) => self.set_state(TtsState::Error { message: format!("{e:#}") }),
        }
        r
    }

    fn download(&self, url: &str, dest: &Path, what: &str) -> Result<()> {
        let tmp = dest.with_extension("part");
        self.rt
            .block_on(async {
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
            .with_context(|| format!("downloading the {what}"))?;
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }

    /// Deletes the downloaded model and voices.
    pub fn remove(&self) -> Result<()> {
        *self.loaded.lock() = None;
        self.cache.lock().clear();
        if let Some(tts) = self.dir.parent().filter(|p| p.exists()) {
            std::fs::remove_dir_all(tts)?;
        }
        self.set_state(TtsState::Missing);
        Ok(())
    }

    /// Loads the model ahead of time (in the background), so the first
    /// announcement isn't late.
    pub fn warm_up(self: &Arc<Self>, cfg: TtsConfig) {
        if !self.installed(&cfg) {
            return;
        }
        let tts = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = tts.synthesize(&cfg, "Ready.", &[]) {
                log::warn!("tts warm-up: {e:#}");
            }
        });
    }

    /// Releases the model after a while without speaking (called periodically).
    pub fn unload_if_idle(&self) {
        let mut l = self.loaded.lock();
        if l.as_ref().is_some_and(|l| l.last_used.elapsed() > IDLE_UNLOAD) {
            *l = None;
            log::info!("tts: model unloaded (idle)");
        }
    }

    /// Speaks `text`: mono samples and their sample rate. Runs at low priority
    /// (call from a background thread).
    pub fn synthesize(&self, cfg: &TtsConfig, text: &str, prons: &[Pronunciation]) -> Result<(Arc<Vec<f32>>, u32)> {
        let speed = if cfg.speed > 0.0 { cfg.speed } else { 1.0 };
        let key = format!("{}|{}|{speed}|{text}|{}", cfg.voice, cfg.model, prons_key(prons));
        if let Some(hit) = self.cache.lock().get(&key) {
            return Ok((hit.clone(), SAMPLE_RATE));
        }
        if !self.installed(cfg) {
            return Err(anyhow!("The announcer voice isn't installed (Settings → Auto Mode)"));
        }
        let (lang, _) = voice_lang(&cfg.voice).ok_or_else(|| anyhow!("Unsupported Kokoro voice “{}”", cfg.voice))?;
        // Synthesis is CPU-heavy: below the audio threads. ONNX Runtime's
        // worker threads are created from this thread and inherit it.
        crate::audio::priority::lower_current_thread();
        let phonemes = self.phonemizer(lang)?.phonemize(&with_pronunciations(text, prons))?;
        let mut guard = self.loaded.lock();
        let l = self.ready(&mut guard, cfg)?;
        if !l.voices.contains_key(&cfg.voice) {
            let raw = std::fs::read(self.voices_dir().join(format!("{}.bin", cfg.voice)))?;
            let styles = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            l.voices.insert(cfg.voice.clone(), styles);
        }
        let mut audio = vec![];
        for chunk in split_tokens(&phonemes, &l.vocab) {
            let n = chunk.len();
            let styles = &l.voices[&cfg.voice];
            let row = n.min(styles.len() / 256 - 1);
            let style = styles[row * 256..row * 256 + 256].to_vec();
            let mut ids = Vec::with_capacity(n + 2);
            ids.push(0);
            ids.extend(chunk);
            ids.push(0);
            let len = ids.len();
            let out = l.session.run(ort::inputs![
                "input_ids" => ort::value::Tensor::from_array(([1usize, len], ids))?,
                "style" => ort::value::Tensor::from_array(([1usize, 256], style))?,
                "speed" => ort::value::Tensor::from_array(([1usize], vec![speed as f32]))?,
            ])?;
            let (_, wav) = out[0].try_extract_tensor::<f32>()?;
            audio.extend_from_slice(wav);
        }
        drop(guard);
        let clip = Arc::new(audio);
        let mut cache = self.cache.lock();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, clip.clone());
        Ok((clip, SAMPLE_RATE))
    }

    /// The phonemes Kokoro would read for `text` (to check pronunciations).
    pub fn phonemes(&self, cfg: &TtsConfig, text: &str, prons: &[Pronunciation]) -> Result<String> {
        let (lang, _) = voice_lang(&cfg.voice).ok_or_else(|| anyhow!("Unsupported Kokoro voice “{}”", cfg.voice))?;
        self.phonemizer(lang)?.phonemize(&with_pronunciations(text, prons))
    }

    fn phonemizer(&self, lang: Lang) -> Result<Arc<Phonemizer>> {
        let mut map = self.phonemizers.lock();
        if let Some(p) = map.get(&lang) {
            return Ok(p.clone());
        }
        let p = Arc::new(Phonemizer::new(lang, &self.dir.join("espeak-ng-data"))?);
        map.insert(lang, p.clone());
        Ok(p)
    }

    /// The loaded model (loading it if needed).
    fn ready<'a>(&self, guard: &'a mut Option<Loaded>, cfg: &TtsConfig) -> Result<&'a mut Loaded> {
        if !self.installed(cfg) {
            return Err(anyhow!("The announcer voice isn't installed (Settings → Auto Mode)"));
        }
        if guard.as_ref().is_none_or(|l| l.model_file != cfg.model_file()) {
            *guard = None;
            *guard = Some(self.load(cfg)?);
        }
        let l = guard.as_mut().expect("loaded");
        l.last_used = Instant::now();
        Ok(l)
    }

    fn load(&self, cfg: &TtsConfig) -> Result<Loaded> {
        let started = Instant::now();
        let tok: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(self.dir.join("tokenizer.json"))?)?;
        let vocab = tok["model"]["vocab"]
            .as_object()
            .ok_or_else(|| anyhow!("Kokoro's vocabulary is unreadable; remove and download it again"))?
            .iter()
            .filter_map(|(k, v)| Some((k.chars().next()?, v.as_i64()?)))
            .collect();
        let session = ort::session::Session::builder()?
            .with_intra_threads(THREADS)?
            .commit_from_file(self.dir.join(cfg.model_file()))
            .context("loading the Kokoro model")?;
        log::info!("tts: Kokoro loaded in {:.1} s", started.elapsed().as_secs_f64());
        Ok(Loaded {
            model_file: cfg.model_file(),
            session,
            vocab,
            voices: HashMap::new(),
            last_used: Instant::now(),
        })
    }
}

/// A piece of text after applying pronunciations.
#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    /// Phonemes for this word (from a `/…/` pronunciation).
    Phonemes { word: String, phonemes: String },
}

/// Text with pronunciations applied: respellings replace the word in the
/// text; phoneme pronunciations become separate pieces. Whole words, any
/// case; longer entries win over shorter ones.
fn with_pronunciations(text: &str, prons: &[Pronunciation]) -> Vec<Piece> {
    let mut prons: Vec<&Pronunciation> = prons.iter().filter(|p| !p.word.trim().is_empty() && !p.say.trim().is_empty()).collect();
    prons.sort_by_key(|p| std::cmp::Reverse(p.word.trim().chars().count()));
    let mut pieces = vec![Piece::Text(text.to_string())];
    for p in prons {
        let Ok(re) = regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(p.word.trim()))) else { continue };
        let say = p.say.trim();
        let phonemes = say.strip_prefix('/').and_then(|s| s.strip_suffix('/'));
        let mut next = vec![];
        for piece in pieces {
            let Piece::Text(t) = piece else {
                next.push(piece);
                continue;
            };
            let mut last = 0;
            for m in re.find_iter(&t) {
                next.push(Piece::Text(t[last..m.start()].to_string()));
                match phonemes {
                    Some(ph) => next.push(Piece::Phonemes { word: m.as_str().to_string(), phonemes: ph.trim().to_string() }),
                    None => next.push(Piece::Text(say.to_string())),
                }
                last = m.end();
            }
            next.push(Piece::Text(t[last..].to_string()));
        }
        // Merge neighbouring text pieces (a respelling stays in its sentence).
        pieces = vec![];
        for piece in next {
            match (pieces.last_mut(), piece) {
                (Some(Piece::Text(a)), Piece::Text(b)) => a.push_str(&b),
                (_, Piece::Text(b)) if b.is_empty() => {}
                (_, piece) => pieces.push(piece),
            }
        }
    }
    pieces
}

fn prons_key(prons: &[Pronunciation]) -> String {
    prons.iter().map(|p| format!("{}={}", p.word, p.say)).collect::<Vec<_>>().join(";")
}

/// Phoneme token ids in chunks Kokoro accepts, split at pauses where possible.
fn split_tokens(phonemes: &str, vocab: &HashMap<char, i64>) -> Vec<Vec<i64>> {
    let ids: Vec<(char, i64)> = phonemes.chars().filter_map(|c| vocab.get(&c).map(|&id| (c, id))).collect();
    let mut chunks = vec![];
    let mut start = 0;
    while start < ids.len() {
        let mut end = (start + MAX_TOKENS).min(ids.len());
        if end < ids.len() {
            // Break after the last sentence or clause mark in range.
            if let Some(p) = ids[start..end].iter().rposition(|(c, _)| ".!?;:,".contains(*c)) {
                end = start + p + 1;
            }
        }
        chunks.push(ids[start..end].iter().map(|(_, id)| *id).collect());
        start = end;
    }
    chunks
}

/// Text → Kokoro phonemes for one language.
enum Phonemizer {
    English(misaki_rs::G2P, bool),
    Espeak(Mutex<espeak_ng::Translator>),
}

impl Phonemizer {
    fn new(lang: Lang, espeak_data: &Path) -> Result<Self> {
        let espeak = |code: &str| -> Result<espeak_ng::Translator> {
            let base = code.split('-').next().unwrap_or(code);
            espeak_ng::install_bundled_language(espeak_data, base).with_context(|| format!("espeak-ng data for {base}"))?;
            Ok(espeak_ng::Translator::new(code, Some(espeak_data))?)
        };
        Ok(match lang {
            Lang::EnUs | Lang::EnGb => {
                let gb = lang == Lang::EnGb;
                let fallback = EspeakFallback(Mutex::new(espeak(if gb { "en" } else { "en-us" })?), gb);
                let language = if gb { misaki_rs::Language::EnglishGB } else { misaki_rs::Language::EnglishUS };
                Phonemizer::English(misaki_rs::G2P::with_fallback(language, Some(Box::new(fallback))), gb)
            }
            Lang::Espeak(code) => Phonemizer::Espeak(Mutex::new(espeak(code)?)),
        })
    }

    fn phonemize(&self, pieces: &[Piece]) -> Result<String> {
        match self {
            Phonemizer::English(g2p, gb) => {
                // misaki's own markup keeps the words in their sentence: `[word](/phonemes/)`.
                let text: String = pieces
                    .iter()
                    .map(|p| match p {
                        Piece::Text(t) => t.clone(),
                        Piece::Phonemes { word, phonemes } => format!("[{}](/{}/)", word.replace(['[', ']'], ""), phonemes),
                    })
                    .collect();
                let (p, _) = g2p.g2p(&text).map_err(|e| anyhow!("phonemizing: {e}"))?;
                Ok(normalize_english(&p, *gb))
            }
            Phonemizer::Espeak(tr) => {
                let tr = tr.lock();
                let mut out = vec![];
                for p in pieces {
                    match p {
                        Piece::Text(t) if !t.trim().is_empty() => out.push(tr.text_to_ipa(t)?.replace('\u{200d}', "")),
                        Piece::Text(_) => {}
                        Piece::Phonemes { phonemes, .. } => out.push(phonemes.clone()),
                    }
                }
                Ok(out.join(" "))
            }
        }
    }
}

/// misaki-rs writes diphthongs as letter pairs joined by U+200D; Kokoro's
/// vocabulary has misaki's single-letter forms. American English doesn't mark
/// vowel length.
fn normalize_english(p: &str, gb: bool) -> String {
    let mut s = p.to_string();
    for (pair, one) in [("a\u{200d}ɪ", "I"), ("e\u{200d}ɪ", "A"), ("o\u{200d}ʊ", "O"), ("a\u{200d}ʊ", "W"), ("ɔ\u{200d}ɪ", "Y"), ("ə\u{200d}ʊ", "Q")] {
        s = s.replace(pair, one);
    }
    s = s.replace('\u{200d}', "");
    if !gb {
        s = s.replace('ː', "");
    }
    s
}

/// Unknown English words: espeak-ng's IPA in misaki's conventions (as
/// misaki-rs' own espeak fallback does).
struct EspeakFallback(Mutex<espeak_ng::Translator>, bool);

impl misaki_rs::Fallback for EspeakFallback {
    fn phonemize(&self, word: &str) -> std::result::Result<String, misaki_rs::fallback::FallbackError> {
        let ipa = self.0.lock().text_to_ipa(word).map_err(|e| misaki_rs::fallback::FallbackError::Espeak { word: word.into(), error: e.to_string() })?;
        let mut r = ipa;
        if !self.1 {
            for (long, short) in [("iː", "i"), ("uː", "u"), ("ɜː", "ɜ"), ("ɔː", "ɔ"), ("ɑː", "ɑ")] {
                r = r.replace(long, short);
            }
        }
        Ok(r.replace('_', "").replace("ˈˈ", "ˈ").replace("ˌˌ", "ˌ"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_phonemes_fit_kokoro() {
        let s = normalize_english("fˈa\u{200d}ɪnd ɐ pˈɑː\u{200d}ɹtnɚ", false);
        assert_eq!(s, "fˈInd ɐ pˈɑɹtnɚ");
        assert_eq!(normalize_english("ɡˈə\u{200d}ʊ", true), "ɡˈQ");
    }

    #[test]
    fn long_input_is_split_at_pauses() {
        let vocab: HashMap<char, i64> = [('a', 1), ('.', 4)].into_iter().collect();
        let text = format!("{}.{}", "a".repeat(400), "a".repeat(300));
        let chunks = split_tokens(&text, &vocab);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 401);
        assert!(chunks.iter().all(|c| c.len() <= MAX_TOKENS));
    }

    #[test]
    fn pronunciations_apply_to_whole_words() {
        let prons = vec![
            Pronunciation { word: "Schottische".into(), say: "shot-eesh".into() },
            Pronunciation { word: "hambo".into(), say: "/hˈɑmbO/".into() },
            Pronunciation { word: "Cha Cha".into(), say: "chah chah".into() },
        ];
        let pieces = with_pronunciations("A schottische, a Hambo and a cha cha (not Hambone).", &prons);
        assert_eq!(
            pieces,
            vec![
                Piece::Text("A shot-eesh, a ".into()),
                Piece::Phonemes { word: "Hambo".into(), phonemes: "hˈɑmbO".into() },
                Piece::Text(" and a chah chah (not Hambone).".into()),
            ]
        );
        assert_eq!(with_pronunciations("Waltz", &[]), vec![Piece::Text("Waltz".into())]);
    }

    #[test]
    fn phoneme_overrides_reach_the_phonemes() {
        let dir = std::env::temp_dir().join(format!("orpheus-tts-test-{}", std::process::id()));
        let p = Phonemizer::new(Lang::EnUs, &dir).unwrap();
        let prons = vec![Pronunciation { word: "hambo".into(), say: "/hˈɑmbO/".into() }];
        let out = p.phonemize(&with_pronunciations("The next dance is a Hambo.", &prons)).unwrap();
        assert!(out.contains("hˈɑmbO"), "{out}");
        assert!(out.contains("dˈæns"), "{out}");
        let plain = p.phonemize(&with_pronunciations("Rotary Waltz", &[])).unwrap();
        assert!(!plain.contains('\u{200d}') && !plain.is_empty(), "{plain}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn voices_map_to_languages() {
        assert_eq!(voice_lang("af_heart").map(|v| v.0), Some(Lang::EnUs));
        assert_eq!(voice_lang("bm_george").map(|v| v.0), Some(Lang::EnGb));
        assert!(voice_lang("jf_alpha").is_none());
        assert!(is_voice("af_heart") && is_voice("bm_george"));
        assert!(!is_voice("en_US-lessac-medium") && !is_voice("jf_alpha"));
    }
}

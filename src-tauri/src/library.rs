//! Local library: folder scanning, content-addressed import, background analysis.
//!
//! Tracks are identified by a hash of their audio data (see `decode::content_hash`),
//! not by path. Moving or renaming files, or reorganising the music folder, keeps
//! all tags, styles and setlist references intact: the next scan re-attaches the
//! new path to the existing track. Files that vanish leave the track in place,
//! marked missing, until it is found again.

use crate::model::{Id, SourceRef};
use crate::store::{FileEntry, NewTrack, Store};
use crate::decode;
use anyhow::Result;
use lofty::prelude::*;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub added: usize,
    pub relinked: usize,
    pub removed: usize,
    pub running: bool,
}

struct Candidate {
    path: PathBuf,
    size: i64,
    mtime: i64,
    root: Option<String>,
}

fn file_stat(path: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    let mtime = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((m.len() as i64, mtime))
}

pub fn read_tags(path: &Path) -> NewTrack {
    let mut t = NewTrack {
        title: path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string(),
        ..Default::default()
    };
    if let Ok(tagged) = lofty::read_from_path(path) {
        t.duration_ms = tagged.properties().duration().as_millis() as i64;
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(v) = tag.title() {
                if !v.trim().is_empty() {
                    t.title = v.trim().to_string();
                }
            }
            t.artist = tag.artist().map(|s| s.trim().to_string()).unwrap_or_default();
            t.album = tag.album().map(|s| s.trim().to_string()).unwrap_or_default();
            t.genre = tag.genre().map(|s| s.trim().to_string()).unwrap_or_default();
            let bpm = tag
                .get_string(ItemKey::Bpm)
                .or_else(|| tag.get_string(ItemKey::IntegerBpm))
                .and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok())
                .filter(|b| *b > 20.0 && *b < 400.0);
            if let Some(b) = bpm {
                t.bpm = Some(b);
                t.bpm_source = Some("tag".into());
            }
        }
    }
    t
}

/// The ISRC in a file's tags (identifies the recording at Spotify and elsewhere).
pub fn read_isrc(path: &Path) -> Option<String> {
    let tagged = lofty::read_from_path(path).ok()?;
    tagged.tags().iter().find_map(|t| t.get_string(ItemKey::Isrc)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Cover art for a local file: the embedded front cover (or any embedded
/// picture), else an image file next to it. Returns (MIME type, bytes).
pub fn artwork(path: &Path) -> Option<(String, Vec<u8>)> {
    if let Ok(tagged) = lofty::read_from_path(path) {
        let pics: Vec<&lofty::picture::Picture> = tagged.tags().iter().flat_map(|t| t.pictures()).collect();
        let pic = pics
            .iter()
            .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
            .or_else(|| pics.first());
        if let Some(p) = pic {
            let mime = p.mime_type().map(|m| m.as_str().to_string()).unwrap_or_else(|| "image/jpeg".into());
            return Some((mime, p.data().to_vec()));
        }
    }
    // An image next to the file: one with a conventional name, or the only image there.
    let images: Vec<PathBuf> = std::fs::read_dir(path.parent()?)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| image_mime(p).is_some())
        .collect();
    let conventional = |p: &&PathBuf| {
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        ["cover", "folder", "front", "album"].iter().any(|n| stem.contains(n))
    };
    let p = images.iter().find(conventional).or(if images.len() == 1 { images.first() } else { None })?;
    Some((image_mime(p)?.to_string(), std::fs::read(p).ok()?))
}

fn image_mime(p: &Path) -> Option<&'static str> {
    match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Imports (or re-links) one file. Returns the songs playing its recording
/// and whether it is new (new audio gets a song; known audio, and files
/// removed from the library, get none).
fn import_candidate(store: &Store, c: &Candidate, hash: &str, tags: Option<NewTrack>) -> Result<(Vec<Id>, bool)> {
    let path_s = c.path.to_string_lossy().to_string();
    store.upsert_file(&path_s, FileEntry { hash: hash.to_string(), size: c.size, mtime: c.mtime, root: c.root.clone() });
    let src = SourceRef { kind: "file".into(), key: hash.to_string() };
    // Removed from the library: only Library → Files adds it again.
    if store.is_excluded(hash) {
        return Ok((vec![], false));
    }
    if let Some(rid) = store.recording_for_source(&src) {
        return Ok((store.songs_for_recording(&rid), false));
    }
    let nt = tags.unwrap_or_else(|| read_tags(&c.path));
    Ok((vec![store.insert_track(&nt, src)?], true))
}

/// Low-priority threads for scanning, so a big import never disturbs playback.
fn background_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("scan-{i}"))
            .start_handler(|_| crate::audio::priority::lower_current_thread())
            .build()
            .expect("scan thread pool")
    })
}

fn process_candidates(db: &Store, cands: Vec<Candidate>, progress: &dyn Fn(&ScanProgress), p: &mut ScanProgress) -> Vec<Id> {
    p.phase = "Reading files".into();
    p.total = cands.len();
    p.done = 0;
    progress(p);
    let mut touched = vec![];
    // Hash + read tags in parallel, write to the database serially.
    for chunk in cands.chunks(64) {
        let results: Vec<_> = background_pool().install(|| {
            chunk
                .par_iter()
                .map(|c| {
                    let hash = decode::content_hash(&c.path);
                    let tags = hash.as_ref().ok().map(|_| read_tags(&c.path));
                    (hash, tags)
                })
                .collect()
        });
        for (c, (hash, tags)) in chunk.iter().zip(results) {
            match hash {
                Ok(h) => {
                    let had_path = db.file_info(&c.path.to_string_lossy()).is_some();
                    match import_candidate(db, c, &h, tags) {
                        Ok((ids, new)) => {
                            if new {
                                p.added += 1;
                            } else if !had_path {
                                p.relinked += 1;
                            }
                            touched.extend(ids);
                        }
                        Err(e) => log::warn!("import {}: {e:#}", c.path.display()),
                    }
                }
                Err(e) => log::warn!("hash {}: {e:#}", c.path.display()),
            }
            p.done += 1;
        }
        progress(p);
    }
    if let Err(e) = db.save_index() {
        log::warn!("saving file index: {e:#}");
    }
    touched
}

/// Scans library folders. Unchanged files (same size and mtime) are skipped.
pub fn scan(db: &Store, folders: &[String], progress: &dyn Fn(&ScanProgress)) -> Result<ScanProgress> {
    let mut p = ScanProgress { running: true, phase: "Finding files".into(), ..Default::default() };
    progress(&p);
    let mut cands = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for root in folders {
        for entry in walkdir::WalkDir::new(root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !entry.file_type().is_file() || !decode::is_audio_file(path) {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("._")).unwrap_or(false) {
                continue; // macOS resource forks
            }
            let Some((size, mtime)) = file_stat(path) else { continue };
            let path_s = path.to_string_lossy().to_string();
            seen.insert(path_s.clone());
            match db.file_info(&path_s) {
                Some(f) if f.size == size && f.mtime == mtime => {}
                _ => cands.push(Candidate { path: path.to_path_buf(), size, mtime, root: Some(root.clone()) }),
            }
        }
    }
    process_candidates(db, cands, progress, &mut p);

    // Forget paths that no longer exist. Tracks themselves are kept.
    p.phase = "Cleaning up".into();
    progress(&p);
    for path in db.all_file_paths() {
        if !seen.contains(&path) && !Path::new(&path).exists() {
            db.remove_file(&path);
            p.removed += 1;
        }
    }
    db.save_index()?;
    p.phase = "Done".into();
    p.running = false;
    progress(&p);
    Ok(p)
}

/// Imports individual files or folders (e.g. drag and drop).
pub fn import_paths(db: &Store, paths: &[String], progress: &dyn Fn(&ScanProgress)) -> Result<Vec<Id>> {
    let mut p = ScanProgress { running: true, phase: "Importing".into(), ..Default::default() };
    let mut cands = vec![];
    for root in paths {
        for entry in walkdir::WalkDir::new(root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if entry.file_type().is_file() && decode::is_audio_file(path) {
                if let Some((size, mtime)) = file_stat(path) {
                    cands.push(Candidate { path: path.to_path_buf(), size, mtime, root: None });
                }
            }
        }
    }
    let ids = process_candidates(db, cands, progress, &mut p);
    p.running = false;
    p.phase = "Done".into();
    progress(&p);
    Ok(ids)
}

/// Finds a readable file for a recording, if any of its known paths still exist.
pub fn resolve_path(db: &Store, rec_id: &str) -> Option<PathBuf> {
    db.recording_file_paths(rec_id).into_iter().map(PathBuf::from).find(|p| p.exists())
}

pub fn analyze_recording(db: &Store, rec_id: &str) -> Result<()> {
    let path = resolve_path(db, rec_id).ok_or_else(|| anyhow::anyhow!("file missing"))?;
    let (mono, sr) = decode::decode_mono(&path)?;
    let a = crate::analysis::analyze(&mono, sr, db.known_bpm(rec_id));
    db.set_analysis(rec_id, &a)?;
    Ok(())
}

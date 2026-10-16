//! Set files (`*.orpheus-set.toml`): a set to share or keep, as text. Each
//! song entry carries its audio's identity (file hashes, Spotify tracks,
//! fingerprint, length), so another library links it to the same audio: the
//! same file, another encoding of it (by fingerprint), or Spotify. Audio it
//! doesn't have yet is added as missing, and links up when it turns up (see
//! `Store::reconnect_matches`).

use crate::model::*;
use crate::store::Store;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// The format's version (`orpheus_set` in the file).
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetFile {
    pub orpheus_set: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub club: Option<SetClub>,
    #[serde(rename = "entry")]
    pub entries: Vec<SetFileEntry>,
}

/// The club guidelines the set was planned with (linked if you have them).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetClub {
    pub id: String,
    pub version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// That version of its guidelines, as a self-contained club file (added
    /// on import if the library doesn't have it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetFileEntry {
    /// `song` (a song, or a style slot without one) or `pause`.
    #[serde(skip_serializing_if = "is_song")]
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song: Option<SetSong>,
    /// The version's name and display name (informational / for a new version).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Your style keys shown (the entry's, else the version's).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub club_styles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hold: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_before: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_after: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    /// How it played there (informational: an imported set starts afresh).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// How it's played: the version's settings with the entry's on top.
    #[serde(skip_serializing_if = "is_default_settings")]
    pub settings: Overrides,
}

fn is_song(k: &EntryKind) -> bool {
    *k == EntryKind::Song
}

fn is_default_settings(o: &Overrides) -> bool {
    o == &Overrides::default()
}

/// What identifies a song's audio: any one of these is enough.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetSong {
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub artist: String,
    /// Seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Files with exactly this audio (Orpheus's content hash).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hash: Vec<String>,
    /// Spotify tracks of it (`spotify:track:…`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spotify: Vec<String>,
    /// Chromaprint fingerprint: recognises it in any encoding (not an edit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// A set as a set file.
pub fn export(store: &Store, setlist_id: &str, club_name: Option<String>, club_rules: Option<String>) -> Result<String> {
    let set = store.setlist(setlist_id)?;
    let mut entries = vec![];
    for e in &set.entries {
        let mut out = SetFileEntry {
            kind: e.kind,
            club_styles: e.club_styles.clone(),
            label: e.label.clone(),
            note: e.note.clone(),
            tags: e.tags.clone(),
            hold: e.hold,
            pause_before: e.pause_before,
            pause_after: e.pause_after,
            announcement: e.announcement.clone(),
            status: e.status.clone(),
            styles: e.styles.clone().unwrap_or_default(),
            settings: e.overrides.clone(),
            ..Default::default()
        };
        if let Some(vid) = &e.version_id {
            if let Ok((v, tid)) = store.version(vid) {
                let t = store.track(&tid)?;
                let r = store.recording(&v.recording).ok().map(|r| r.rec);
                if e.styles.is_none() {
                    out.styles = v.styles.clone();
                }
                out.version = Some(v.name.clone());
                out.display_name = v.display_name.clone();
                out.settings = crate::app::App::resolve_settings(&v, Some(e));
                out.song = Some(SetSong {
                    title: t.rec.title.clone(),
                    artist: t.rec.artist.clone(),
                    length: r.as_ref().filter(|r| r.duration_ms > 0).map(|r| (r.duration_ms as f64 / 1000.0).round()),
                    hash: r.iter().flat_map(|r| r.sources.iter().filter(|s| s.kind == "file").map(|s| s.key.clone())).collect(),
                    spotify: r.iter().flat_map(|r| r.sources.iter().filter(|s| s.kind == "spotify").map(|s| s.key.clone())).collect(),
                    fingerprint: r.and_then(|r| r.fingerprint),
                });
            }
        }
        entries.push(out);
    }
    let file = SetFile {
        orpheus_set: FORMAT,
        name: set.name,
        date: set.event_date,
        mode: set.mode,
        notes: set.notes,
        club: set.club.map(|c| SetClub { id: c.id, version: c.version, name: club_name.unwrap_or_default(), rules: club_rules }),
        entries,
    };
    let body = toml::to_string_pretty(&file)?;
    Ok(format!("# An Orpheus set. Each song names its audio (file hashes, Spotify tracks, fingerprint),\n# so another library can link it to the same audio, in any encoding.\n\n{body}"))
}

/// How an import went.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub setlist_id: Id,
    /// Songs linked to audio already in the library.
    pub linked: usize,
    /// Songs added, playable (from Spotify).
    pub spotify: usize,
    /// Songs whose audio isn't here (they link up when it turns up).
    pub missing: usize,
}

/// Adds a set from a set file. `club` links the club the file names (if you have it).
pub fn import(store: &Store, text: &str, club: Option<ClubRef>) -> Result<ImportReport> {
    let file: SetFile = toml::from_str(text).map_err(|e| anyhow!("not an Orpheus set file: {e}"))?;
    if file.orpheus_set > FORMAT {
        return Err(anyhow!("this set file is from a newer Orpheus (format {}); update Orpheus to open it", file.orpheus_set));
    }
    let name = if file.name.trim().is_empty() { "Imported set".to_string() } else { file.name.clone() };
    let mode = if file.mode == "preset" { "preset" } else { "live" };
    let sid = store.setlist_create(&name, mode, club, file.date.clone())?;
    let mut report = ImportReport { setlist_id: sid.clone(), ..Default::default() };
    for e in &file.entries {
        let version_id = match &e.song {
            Some(song) if e.kind == EntryKind::Song => {
                let (vid, how) = link(store, song, e)?;
                match how {
                    Linked::Library => report.linked += 1,
                    Linked::Spotify => report.spotify += 1,
                    Linked::Missing => report.missing += 1,
                }
                Some(vid)
            }
            _ => None,
        };
        let ids = store.entries_insert(
            &sid,
            None,
            &[NewEntry {
                version_id,
                kind: e.kind,
                pause_before: e.pause_before,
                announcement: e.announcement.clone(),
                styles: (!e.styles.is_empty()).then(|| e.styles.clone()),
                club_styles: e.club_styles.clone(),
                label: e.label.clone(),
                overrides: e.settings.clone(),
                note: e.note.clone(),
                tags: e.tags.clone(),
            }],
        )?;
        if e.hold || e.pause_after.is_some() {
            store.entry_update(&ids[0], &EntryPatch { hold: Some(e.hold), pause_after: Some(e.pause_after), ..Default::default() })?;
        }
    }
    Ok(report)
}

enum Linked {
    Library,
    Spotify,
    Missing,
}

/// The version an imported song entry plays: one of the library's songs if
/// it has the audio, else a new set-only song for it.
fn link(store: &Store, song: &SetSong, e: &SetFileEntry) -> Result<(Id, Linked)> {
    if let Some(rid) = store.find_recording(&song.hash, &song.spotify, song.fingerprint.as_deref()) {
        if let Some(sid) = store.song_for_recording(&rid) {
            let t = store.track(&sid)?;
            let playing: Vec<&Version> = t.rec.versions.iter().filter(|v| v.recording == rid).collect();
            let v = playing
                .iter()
                .find(|v| Some(&v.name) == e.version.as_ref())
                .or_else(|| playing.iter().find(|v| t.rec.primary.as_deref() == Some(v.id.as_str())))
                .or_else(|| playing.first())
                .map(|v| v.id.clone());
            if let Some(v) = v {
                return Ok((v, Linked::Library));
            }
        }
        let sid = store.song_from_recording(&rid, false)?;
        return Ok((store.track(&sid)?.rec.versions[0].id.clone(), Linked::Library));
    }
    // Not here: the audio as the file names it, missing until it turns up
    // (or playable from Spotify).
    let rid = store.insert_missing_recording(song)?;
    let sid = store.song_from_recording(&rid, false)?;
    let mut t = store.track(&sid)?;
    if !song.title.is_empty() {
        store.update_track(&sid, &TrackPatch { title: Some(song.title.clone()), artist: Some(song.artist.clone()), ..Default::default() })?;
        t = store.track(&sid)?;
    }
    let vid = t.rec.versions[0].id.clone();
    store.version_update(
        &vid,
        &VersionPatch {
            name: e.version.clone(),
            display_name: Some(e.display_name.clone()),
            styles: (!e.styles.is_empty()).then(|| e.styles.clone()),
            ..Default::default()
        },
    )?;
    let how = if song.spotify.is_empty() { Linked::Missing } else { Linked::Spotify };
    Ok((vid, how))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NewTrack, Store};

    fn store() -> (Store, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("orpheus-test-{}", new_id()));
        (Store::open(&base.join("lib"), &base.join("local")).unwrap(), base)
    }

    fn song(store: &Store, title: &str, kind: &str, key: &str) -> (Id, Id) {
        let id = store.insert_track(&NewTrack { title: title.into(), ..Default::default() }, SourceRef { kind: kind.into(), key: key.into() }).unwrap();
        let v = store.track(&id).unwrap().rec.versions[0].clone();
        (v.id, v.recording)
    }

    /// Two seconds of a chord sequence, fingerprinted (a little noise: a re-encode).
    fn fp(seed: u32, noise: f32) -> String {
        let sr = 11025u32;
        let (mut st, mut n) = (seed, 3u32);
        let mut f = [0.0f32; 3];
        let mono: Vec<f32> = (0..sr as usize * 40)
            .map(|i| {
                if i % (sr as usize / 2) == 0 {
                    for x in f.iter_mut() {
                        st = st.wrapping_mul(1664525).wrapping_add(1013904223);
                        *x = 110.0 * 2f32.powf((st >> 24) as f32 / 48.0);
                    }
                }
                n = n.wrapping_mul(1664525).wrapping_add(1013904223);
                let t = i as f32 / sr as f32;
                f.iter().map(|x| (t * x * std::f32::consts::TAU).sin()).sum::<f32>() * 0.25 + ((n >> 16) as f32 / 65536.0 - 0.5) * noise
            })
            .collect();
        crate::fingerprint::encode(&crate::fingerprint::compute(&mono, sr).unwrap())
    }

    #[test]
    fn sets_travel_between_libraries() {
        let (a, base_a) = store();
        let (same, _) = song(&a, "Same file", "file", "h-same");
        let (enc, renc) = song(&a, "Other encoding", "file", "h-flac");
        a.set_analysis(&renc, &crate::store::Analysis { fingerprint: Some(fp(5, 0.0)), duration_ms: 40_000, ..Default::default() }).unwrap();
        let (sp, _) = song(&a, "Spotify only", "spotify", "spotify:track:abc");
        let (gone, _) = song(&a, "Not there", "file", "h-gone");
        let sid = a.setlist_create("Fall Social", "preset", None, Some("2026-10-16".into())).unwrap();
        let mk = |v: &Id| NewEntry { version_id: Some(v.clone()), ..Default::default() };
        let ids = a.entries_insert(&sid, None, &[mk(&same), mk(&enc), mk(&sp), mk(&gone), NewEntry { styles: Some(vec!["waltz".into()]), ..Default::default() }, NewEntry { kind: EntryKind::Pause, pause_before: Some(30.0), ..Default::default() }]).unwrap();
        a.entry_update(&ids[0], &EntryPatch { overrides: Some(Overrides { target_bpm: Some(150.0), ..Default::default() }), hold: Some(true), ..Default::default() }).unwrap();
        let text = export(&a, &sid, None, None).unwrap();
        assert!(text.contains("h-same") && text.contains("spotify:track:abc") && text.contains("targetBpm = 150"), "{text}");

        // Another library: the same file, and an MP3 of the FLAC (another hash, same audio).
        let (b, base_b) = store();
        let (b_same, _) = song(&b, "My copy", "file", "h-same");
        let (b_mp3, rmp3) = song(&b, "My MP3", "file", "h-mp3");
        b.set_analysis(&rmp3, &crate::store::Analysis { fingerprint: Some(fp(5, 0.02)), duration_ms: 40_000, ..Default::default() }).unwrap();
        let r = import(&b, &text, None).unwrap();
        assert_eq!((r.linked, r.spotify, r.missing), (2, 1, 1));
        let set = b.setlist(&r.setlist_id).unwrap();
        assert_eq!(set.name, "Fall Social");
        assert_eq!(set.entries.len(), 6);
        assert_eq!(set.entries[0].version_id.as_ref(), Some(&b_same), "the same file");
        assert_eq!(set.entries[0].overrides.target_bpm, Some(150.0));
        assert!(set.entries[0].hold);
        assert_eq!(set.entries[1].version_id.as_ref(), Some(&b_mp3), "another encoding, by fingerprint");
        // The others are set-only songs: one plays from Spotify, one is missing until its file turns up.
        let (spv, _) = b.version(set.entries[2].version_id.as_ref().unwrap()).unwrap();
        assert_eq!(b.recording(&spv.recording).unwrap().rec.sources[0].key, "spotify:track:abc");
        let (gv, gt) = b.version(set.entries[3].version_id.as_ref().unwrap()).unwrap();
        let t = b.track(&gt).unwrap();
        assert!(!t.rec.in_library && t.missing && t.rec.title == "Not there");
        assert_eq!(gv.recording, "h-gone", "named by its content, so its file links it when scanned");
        assert!(set.entries[4].version_id.is_none() && set.entries[4].styles.as_deref() == Some(&["waltz".to_string()][..]));
        assert_eq!(set.entries[5].kind, EntryKind::Pause);
        let _ = std::fs::remove_dir_all(base_a);
        let _ = std::fs::remove_dir_all(base_b);
    }
}

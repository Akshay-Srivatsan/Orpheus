//! Set and crate files (`*.orpheus-set.toml`, `*.orpheus-crate.toml`): a set
//! or crate to share or keep, as text. Each song carries its audio's identity
//! (file hashes, Spotify tracks, fingerprint, length), so another library
//! links it to the same audio: the same file, another encoding of it (by
//! fingerprint), or Spotify. Audio it doesn't have yet is added as missing,
//! and links up when it turns up (see `Store::reconnect_matches`).

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
    /// What is said before it (the entry's choice, else its version's or song's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    /// Whether a chime plays before its announcement, and which (the
    /// entry's choice, else its set's, version's or song's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chime: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chime_song: Option<SetChime>,
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

/// A chime: a version of a song (linked like the set's songs), played as set.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetChime {
    #[serde(flatten)]
    pub song: SetSong,
    pub version: String,
    #[serde(skip_serializing_if = "is_default_settings")]
    pub settings: Overrides,
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
        let vt = e.version_id.as_deref().and_then(|vid| store.version(vid).ok()).and_then(|(v, tid)| Some((v, store.track(&tid).ok()?.rec)));
        let layers = crate::chime::Layers { entry: Some(e), set: Some(&set), version: vt.as_ref().map(|x| &x.0), song: vt.as_ref().map(|x| &x.1) };
        let mut out = SetFileEntry {
            kind: e.kind,
            club_styles: e.club_styles.clone(),
            label: e.label.clone(),
            note: e.note.clone(),
            tags: e.tags.clone(),
            hold: e.hold,
            pause_before: e.pause_before,
            pause_after: e.pause_after,
            announcement: layers.announcement().map(String::from),
            chime: layers.chosen_chime().enabled,
            chime_song: layers.chosen_chime().sound.and_then(|vid| song_of(store, &vid)).map(|(song, v)| SetChime { song, version: v.name, settings: v.settings }),
            status: e.status.clone(),
            styles: e.styles.clone().unwrap_or_default(),
            settings: e.overrides.clone(),
            ..Default::default()
        };
        if let Some((song, v)) = e.version_id.as_deref().and_then(|vid| song_of(store, vid)) {
            if e.styles.is_none() {
                out.styles = v.styles.clone();
            }
            out.version = Some(v.name.clone());
            out.display_name = v.display_name.clone();
            out.settings = crate::app::App::resolve_settings(&v, Some(e));
            out.song = Some(song);
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
                let (vid, how) = link(store, song, e.version.as_deref(), e.display_name.clone(), &e.styles, false)?;
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
                chime: ChimeChoice { enabled: e.chime, sound: e.chime_song.as_ref().map(|c| link_chime(store, c)).transpose()? },
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

/// A version's song and its audio's identity.
fn song_of(store: &Store, version_id: &str) -> Option<(SetSong, Version)> {
    let (v, tid) = store.version(version_id).ok()?;
    let t = store.track(&tid).ok()?;
    let r = store.recording(&v.recording).ok().map(|r| r.rec);
    let song = SetSong {
        title: t.rec.title.clone(),
        artist: t.rec.artist.clone(),
        length: r.as_ref().filter(|r| r.duration_ms > 0).map(|r| (r.duration_ms as f64 / 1000.0).round()),
        hash: r.iter().flat_map(|r| r.sources.iter().filter(|s| s.kind == "file").map(|s| s.key.clone())).collect(),
        spotify: r.iter().flat_map(|r| r.sources.iter().filter(|s| s.kind == "spotify").map(|s| s.key.clone())).collect(),
        fingerprint: r.and_then(|r| r.fingerprint),
    };
    Some((song, v))
}

/// The version an imported song plays: one of the library's songs if it
/// has the audio, else a new song for it (set-only unless `in_library`).
fn link(store: &Store, song: &SetSong, version: Option<&str>, display_name: Option<String>, styles: &[String], in_library: bool) -> Result<(Id, Linked)> {
    if let Some(rid) = store.find_recording(&song.hash, &song.spotify, song.fingerprint.as_deref()) {
        if let Some(sid) = store.song_for_recording(&rid) {
            // A set-only song joining a crate joins the library.
            if in_library && !store.track(&sid)?.rec.in_library {
                store.set_in_library(&sid, true)?;
            }
            let t = store.track(&sid)?;
            let playing: Vec<&Version> = t.rec.versions.iter().filter(|v| v.recording == rid).collect();
            let v = playing
                .iter()
                .find(|v| Some(v.name.as_str()) == version)
                .or_else(|| playing.iter().find(|v| t.rec.primary.as_deref() == Some(v.id.as_str())))
                .or_else(|| playing.first())
                .map(|v| v.id.clone());
            if let Some(v) = v {
                return Ok((v, Linked::Library));
            }
        }
        let sid = store.song_from_recording(&rid, in_library)?;
        return Ok((store.track(&sid)?.rec.versions[0].id.clone(), Linked::Library));
    }
    // Not here: the audio as the file names it, missing until it turns up
    // (or playable from Spotify).
    let rid = store.insert_missing_recording(song)?;
    let sid = store.song_from_recording(&rid, in_library)?;
    let mut t = store.track(&sid)?;
    if !song.title.is_empty() {
        store.update_track(&sid, &TrackPatch { title: Some(song.title.clone()), artist: Some(song.artist.clone()), ..Default::default() })?;
        t = store.track(&sid)?;
    }
    let vid = t.rec.versions[0].id.clone();
    store.version_update(
        &vid,
        &VersionPatch {
            name: version.map(String::from),
            display_name: Some(display_name),
            styles: (!styles.is_empty()).then(|| styles.to_vec()),
            ..Default::default()
        },
    )?;
    let how = if song.spotify.is_empty() { Linked::Missing } else { Linked::Spotify };
    Ok((vid, how))
}

/// The version a set file's chime plays: the library's version of that name
/// for its audio, else a new version (hidden from the library) with its settings.
fn link_chime(store: &Store, c: &SetChime) -> Result<Id> {
    let (vid, _) = link(store, &c.song, Some(&c.version), None, &[], false)?;
    let (v, tid) = store.version(&vid)?;
    if v.name == c.version {
        // Just made for it (its audio wasn't here): give it the chime's settings.
        if v.settings == Overrides::default() && c.settings != Overrides::default() {
            let o = &c.settings;
            store.version_update(
                &vid,
                &VersionPatch {
                    target_bpm: Some(o.target_bpm),
                    cue_in_ms: Some(o.cue_in_ms),
                    cue_out_ms: Some(o.cue_out_ms),
                    fade_out_ms: Some(o.fade_out_ms),
                    gain_db: Some(o.gain_db),
                    ..Default::default()
                },
            )?;
        }
        return Ok(vid);
    }
    let made = store.version_create(
        &tid,
        Version {
            id: String::new(),
            name: c.version.clone(),
            display_name: None,
            styles: vec![],
            legacy_style: None,
            settings: c.settings.clone(),
            comment: String::new(),
            announcement: None,
            chime: ChimeChoice::default(),
            tags: vec![],
            hidden: false,
            unlisted: true,
            ..v
        },
    )?;
    Ok(made.id)
}

/// A crate as a crate file: its songs (with their audio's identity), or a
/// smart crate's rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CrateFile {
    pub orpheus_crate: u32,
    pub name: String,
    /// A smart crate's rules (JSON, as Orpheus keeps them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smart: Option<String>,
    #[serde(rename = "song", skip_serializing_if = "Vec::is_empty")]
    pub songs: Vec<CrateSong>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CrateSong {
    #[serde(flatten)]
    pub song: SetSong,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
}

pub fn export_crate(store: &Store, crate_id: &str) -> Result<String> {
    let c = store.crates().into_iter().find(|c| c.id == crate_id).ok_or_else(|| anyhow!("crate not found"))?;
    let songs = c
        .version_ids
        .iter()
        .filter_map(|vid| song_of(store, vid))
        .map(|(song, v)| CrateSong { song, version: Some(v.name), display_name: v.display_name, styles: v.styles })
        .collect();
    let file = CrateFile { orpheus_crate: FORMAT, name: c.name, smart: c.smart.map(|s| s.to_string()), songs };
    let body = toml::to_string_pretty(&file)?;
    Ok(format!("# An Orpheus crate. Each song names its audio (file hashes, Spotify tracks, fingerprint),\n# so another library can link it to the same audio, in any encoding.\n\n{body}"))
}

/// Adds a crate from a crate file; its songs link to the library's audio
/// (songs it doesn't have join the library, missing until their audio turns up).
pub fn import_crate(store: &Store, text: &str) -> Result<ImportReport> {
    let file: CrateFile = toml::from_str(text).map_err(|e| anyhow!("not an Orpheus crate file: {e}"))?;
    if file.orpheus_crate > FORMAT {
        return Err(anyhow!("this crate file is from a newer Orpheus (format {}); update Orpheus to open it", file.orpheus_crate));
    }
    let smart = match &file.smart {
        Some(s) => Some(serde_json::from_str(s).map_err(|e| anyhow!("its smart rules can't be read: {e}"))?),
        None => None,
    };
    let name = if file.name.trim().is_empty() { "Imported crate".to_string() } else { file.name.clone() };
    let id = store.crate_create(&name, smart)?;
    let mut report = ImportReport { setlist_id: id.clone(), ..Default::default() };
    let mut vids = vec![];
    for s in &file.songs {
        let (vid, how) = link(store, &s.song, s.version.as_deref(), s.display_name.clone(), &s.styles, true)?;
        match how {
            Linked::Library => report.linked += 1,
            Linked::Spotify => report.spotify += 1,
            Linked::Missing => report.missing += 1,
        }
        vids.push(vid);
    }
    store.crate_add(&id, &vids)?;
    Ok(report)
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

        // A crate goes the same way; its songs join B's library (the missing one,
        // already in B from the set, too).
        let cr = a.crate_create("Waltzes", None).unwrap();
        a.crate_add(&cr, &[enc.clone(), gone.clone()]).unwrap();
        let text = export_crate(&a, &cr).unwrap();
        let r = import_crate(&b, &text).unwrap();
        assert_eq!((r.linked, r.missing), (2, 0));
        let got = b.crates().into_iter().find(|c| c.id == r.setlist_id).unwrap();
        assert_eq!(got.name, "Waltzes");
        assert_eq!(got.version_ids[0], b_mp3);
        let (_, t2) = b.version(&got.version_ids[1]).unwrap();
        assert!(b.track(&t2).unwrap().rec.in_library);
        // A smart crate keeps its rules.
        let rules = serde_json::json!({ "match": "all", "conditions": [{ "field": "style", "op": "is", "value": "waltz" }] });
        let sm = a.crate_create("Smart", Some(rules.clone())).unwrap();
        let r = import_crate(&b, &export_crate(&a, &sm).unwrap()).unwrap();
        assert_eq!(b.crates().into_iter().find(|c| c.id == r.setlist_id).unwrap().smart, Some(rules));
        let _ = std::fs::remove_dir_all(base_a);
        let _ = std::fs::remove_dir_all(base_b);
    }

    #[test]
    fn a_chime_travels_as_a_song() {
        let (a, base_a) = store();
        let (orig, _) = song(&a, "Bell", "file", "h-bell");
        let bell_song = a.version(&orig).unwrap().1;
        let ding = a
            .version_create(&bell_song, Version { name: "Ding".into(), settings: Overrides { cue_in_ms: Some(500), cue_out_ms: Some(2500), ..Default::default() }, ..a.version(&orig).unwrap().0 })
            .unwrap();
        let (tune, _) = song(&a, "Tune", "file", "h-tune");
        let sid = a.setlist_create("Chimes", "preset", None, None).unwrap();
        a.setlist_update(&sid, &SetlistPatch { chime: Some(ChimeChoice { enabled: Some(true), sound: Some(ding.id.clone()) }), ..Default::default() }).unwrap();
        a.entries_insert(&sid, None, &[NewEntry { version_id: Some(tune.clone()), ..Default::default() }]).unwrap();
        let text = export(&a, &sid, None, None).unwrap();
        assert!(text.contains("Ding") && text.contains("h-bell") && !text.contains(&ding.id), "{text}");

        // Here: the same version.
        let r = import(&a, &text, None).unwrap();
        let e = &a.setlist(&r.setlist_id).unwrap().entries[0];
        assert_eq!(e.chime, ChimeChoice { enabled: Some(true), sound: Some(ding.id.clone()) });

        // Another library with the file but not the excerpt: it's made, hidden from the library.
        let (b, base_b) = store();
        song(&b, "Bell", "file", "h-bell");
        let r = import(&b, &text, None).unwrap();
        let chime = b.setlist(&r.setlist_id).unwrap().entries[0].chime.sound.clone().unwrap();
        let (v, _) = b.version(&chime).unwrap();
        assert_eq!((v.name.as_str(), v.settings.cue_in_ms, v.settings.cue_out_ms, v.unlisted), ("Ding", Some(500), Some(2500), true));
        std::fs::remove_dir_all(base_a).ok();
        std::fs::remove_dir_all(base_b).ok();
    }
}

# Orpheus

DJ software for **social dancing**: waltz, swing, Latin, fusion and more,
where each song is a separate dance with a pause in between. It follows
Richard Powers' [DJ tips](https://socialdance.stanford.edu/Syllabi/DJtips.htm).
Nothing about styles or clubs is built in: you define your dance styles and
tags, and import each club's guidelines (`examples/clubs/` has examples for
Stanford's [Dancebreak](https://dancebreak.stanford.edu/info/djing/), Jammix
(Powers' tips) and Friday Night Waltz (Lucas Garron's
[tempos](https://garron.net/dance/tempos/))).

Runs on macOS, Linux and Windows (Tauri 2, Rust and Svelte).

## How it differs from club DJ software

- **No crossfading by default.** Each song ends, or fades out, and a pause
  follows. You can still crossfade by hand when you want to.
- **The next song is manual by default.** Press **Play Next** when the floor
  is ready. If a song is still playing, Play Next asks for a second press,
  then fades the song out before continuing. Auto-advance after a set pause
  is optional.
- **Two identical decks (A and B) and a crossfader.** The crossfader decides
  which deck the room hears; the other deck is the *free deck*, for cueing in
  headphones. The crossfader starts in the middle, where the room hears
  both decks: that's the position for **Play Next**, **Go Live** and auto
  mode, which fade out the previous song first so songs don't overlap.
  It snaps to the center within ±5% of it (see Settings → General). Orpheus
  never moves it: off center is your choice of deck, and starting the other
  deck is refused with an error while the controller's lights flash. The A, ▴ and B buttons around the
  on-screen crossfader (also mappable to controller buttons) put it at
  either end or the middle. Moving it toward a deck switches the headphones
  to the other deck (optional).
- **The next song in the set is loaded onto the free deck automatically**,
  paused at its start, so you can check it in headphones. Play Next always
  starts it from its cue point.
- **A song counts as played only once the room can hear it.** Playing
  something in headphones doesn't add it to the set; making it audible does,
  and songs that weren't planned are inserted into the set at that point.
- **Dance styles and tempos come first.** You tag songs with your own dance
  styles. Each club names and splits styles its own way, with a sweet-spot
  and an acceptable tempo range per style. Orpheus maps your styles onto
  the club's, so the setlist and projector use the club's names.
- **Projector display for dancers.**
  - Live sets show the current dance, the song, its cover art and recent
    songs. Cover art is the file's embedded picture, an image next to the
    file (e.g. `cover.jpg`), or Spotify's.
  - Preset sets show the whole setlist at once in columns (3 × 18 songs fit;
    longer sets scroll sideways to keep the current song in view), with a
    banner for the current dance.
  - Live sets can also list what's coming up (queued songs and style slots).
  - During the pause it shows the next dance, so people can find a partner.
  - It can also show announcements, such as "No street shoes on the floor".

## Concepts

| | |
|---|---|
| **Song** | What you think of as one song: title, artist, notes and its versions. **Remove from Library** (a song or a version) takes it out of the library and its crates; sets that play it keep it (it's deleted once no set does: remove it there too, or delete the set), and Info offers **Add to Library** / **Show in Library** to bring it back. The library lists songs; one with several versions is folded into one row, its **primary version** (▸ shows how many; click to unfold, or use the Version column's ▸ to unfold them all). The primary (★; right-click → Make Primary Version, or the ☆ in Info) is also the version chosen when a song is added without one, e.g. from Spotify. Setlists, history and crates list each version. |
| **Version** | Something to play: a recording, and how to play it (dance styles, tempo, start, end and fade points, gain, stem defaults, comments and your tags). One song can be a Rotary Waltz at 150 BPM and a Fast Waltz at 168 BPM, or have several people's edits of a line dance (or edits of different songs for one choreography), each its own recording. A version can have a **display name** shown to dancers instead of the style names, e.g. the name of a particular line dance. In a set, an entry's **Version** menu (Info) switches between them. |
| **Recording** | A piece of audio: its sources, BPM, meter, loudness, waveform, hot cues and stems; Info shows it as each version's **Audio**. Several songs can play one recording (a song that works for two line dances): **Make a Song from It** in Info. Its id comes from its content (the file's hash, or the Spotify track), so audio removed and added again is the same recording. **Library → Files** lists the files in your library folders: removing one keeps it on disk but out of the library (scanning skips it, listed in `excluded.json`); a recording with another file or a Spotify link just loses that file. Audio that songs or sets still play stays as a **ghost**: they keep pointing to it and show a warning, and it plays again once the same audio is back: the same file or Spotify track, or a file that sounds the same by fingerprint (another encoding, not an edit), which Files offers to **Use** in its place. |
| **Source** | Where a recording's audio comes from: a local file or a Spotify track (the same audio). A recording can have both. |
| **Combining** | Select songs → **Combine…** makes them one song. It compares their audio's fingerprints and suggests whether it's **the same audio** (a FLAC and an MP3, a file and its Spotify track, a remaster: one audio, analysed once; versions that become identical merge) or **different audio** (several edits for one line dance: each version keeps its own). Where they differ (title, BPM…), you choose what to keep. **Separate into Its Own Song** (a version; it keeps its audio, so one audio can serve two songs) and **Split off…** (a file or Spotify link, in a version's Audio) undo it or build other combinations. |
| **Meter** | Triple (3/4, or 6/8 counted in eighths) or duple (2/4, 4/4), detected from the audio, or set by hand. It's detected on the beat grid of the song's BPM, so after fixing a BPM (e.g. with 1½) the meter is re-detected. Triple songs whose tempo was detected at 2/3 of the beat get the corrected BPM automatically. Songs where the evidence is unclear stay unknown. |
| **Your tags** | Labels that aren't dance styles, e.g. "Classic" or "Teaching" (Settings → Styles & Tags). Smart crates can use them. |
| **Your dance styles** | Labels on versions, not places in the library. You define them in Settings → Styles & Tags; there are no defaults. A version can have several, e.g. a song that works as both Lindy Hop and East Coast Swing. Every view (all songs, a crate, a setlist, history) has a style filter bar: click a style to narrow the view, ⌘/Shift-click to combine styles. With one style chosen, unlabelled songs whose tempo fits it are suggested too. Drop songs on a style to add it; use the right-click menu to remove one. Giving a Spotify song a style adds it to your library. |
| **Club styles** | In a club's set, songs show the club's styles they **match**: by meter, tempo (as recorded, at half or double time, or with a tempo change within what the club allows) and length. Styles without a tempo (Mixer, Line Dance…) aren't matched automatically. When a song fits several club styles, your styles on it pick which show by default: a club style with the same name as one of yours (ignoring case, spaces and hyphens), or your rules (Settings → Styles & Tags: "what I call X, the club may call Y"). E.g. a 120 BPM song you call Polka shows as their Polka rather than Polka / Hustle / Cha Cha. It never adds a style the song doesn't fit and doesn't matter when only one fits. Right-click a song in the set → **Show as** to choose any club style for it, even one it doesn't fit (with a warning). A club can also **suggest songs** for a style (below); a song it suggests shows as that style. Without a club, your own styles are shown. |
| **Suggested songs** | A club can list songs for any style, e.g. the accepted edits for a choreography or line dance (Bohemian National Polka, Barbie Line Dance…). Any one of a **fingerprint**, a **Spotify track** or a **file hash** identifies a song; title, artist, length, notes and **download links** (several; they need the hash, which the download must match) are extra. Fingerprints (Chromaprint, as `fpcalc` makes them) recognise a version in any audio format and tell edits apart: an edit with a section cut, a new intro or a tempo change is a different song. They can be written in the club file or kept in files next to it (or on the web), fetched when the club is imported. Clubs can share a **song library** (`song_library = "songs.toml"`, a file next to the club file or a web address): its `[song.<id>]` entries are suggested by id (`suggested = ["…"]`), and it's fetched with the club and kept with each version. Styles danced to particular songs (no meter, tempo or length) fit only the songs suggested; other styles also fit matching songs from your library. Settings → Clubs shows which suggested songs you have, and adds the others from Spotify or a download link. A song's Info has **For a club file: Song entry / Fingerprint / Hash** to copy them. |
| **Crate** | Your own collection of songs: "Waltzes", "Dancebreak classics", "Pre-approved rotary waltzes", "Wedding requests". Crates can mix styles freely. Drag songs onto a crate to add them. |
| **Smart crate** | A crate defined by rules instead of a hand-picked list, e.g. "in *Waltzes* and tempo fits Rotary Waltz", "Lindy Hop, not played in the last 30 days", "longer than 4:30". It updates as the library changes. |
| **Setlist** | An ordered list of versions, or of **style slots** ("a West Coast Swing goes here") that you fill live: press **Choose Song…** on the slot, then double-click a song (or drag one onto the slot). Each setlist uses one club. Each entry can override its version's settings for that set only. It can also show just some of a song's styles, or a custom label such as "Mixer" instead of style names. Songs already played in the open set are greyed out in the library and the set. **Export Set…** (the set's menu) writes a set file (`.orpheus-set.toml`); **Import Set** (Setlists header) opens one. Each song names its audio (file hashes, Spotify tracks, fingerprint), so another library links it to the same file, another encoding of it, or Spotify; audio it doesn't have yet links up when it turns up. The club guidelines version the set uses travels with it (added on import if you don't have it). |
| **Club guidelines** | A club's dance styles (meter, tempo and preferred tempo, length) and what its sets should contain: songs and minutes, how many (or what share) of each style or group of styles, where in the set a style belongs, the first and last dance, rules for part of the set (e.g. "the first 10 songs under 3:30"), how many of a style in a row; plus pause and fade lengths, tags (e.g. "Dance with a Stranger", "Jam") and announcements. A human-editable TOML file (see `examples/clubs/`) a club can publish on its website: import it from a file or its web address (Settings → Clubs), edit it there as a form or as the file, and **Check for Updates** later. Every version is kept; a set stays with the version it was planned with until you move it (**Use newer guidelines** in the set). The set panel's **Guidelines** button checks the set against them. |
| **History** | Derived from setlists: every song that was played (audible in the room). Play counts and "last played" come from it. "Remove from History" marks the entry as not played. |

Settings resolve in this order: setlist entry override, then the version,
then the track's defaults.

**Live, preset and hybrid sets** all use the same setlist:

- **Live:** press **+** on songs as you go. A live set is created for you.
- **Preset:** plan the whole list ahead of time.
- **Hybrid:** plan some songs and leave style slots to fill during the
  event.

During a set you can skip, reorder or insert songs at any time.

## Your library is plain files

The library folder holds songs (with their versions), recordings, setlists and crates as
pretty-printed JSON, one file per object, and club guidelines as TOML files
(every version kept, in `clubs/<club>/`). Your dance styles and
their club rules (and announcement pronunciations) are in `styles.json`. Play history and play counts are
derived from the setlists, so there is no separate log to merge. Keep the
folder in **git** or a synced folder to share it between DJs. IDs are
random, so concurrent edits don't collide. After a `git pull`, use
**Settings → Library → Reload**.

Your music files are **never modified**:

- Files are identified by a hash of their audio data, not by path or tags.
- Renaming, moving or retagging files keeps everything linked.
- If a file goes missing, its track is kept and relinks on the next scan.

Machine-specific state stays in the app data folder:

- where files live on this computer;
- waveforms and prepared stems;
- audio and MIDI settings;
- the Spotify login.

## Controllers and audio

- **Deck controls:**
  - **CUE** works CDJ-style. While playing, it returns to the cue point and
    stops. While paused away from the cue point, it sets the cue point. Held
    at the cue point, it previews until released.
  - **Four hot cues per song**, saved with the track. Click a pad to set or
    jump; ⌥-click or right-click to clear.
  - **Scrubbing:** jog wheels, or dragging on the waveform. A paused deck
    plays short snippets so you can hear where you are.
  - **On-air protection** (Settings → General, on by default): jog, CUE and
    cue jumps are ignored on the deck the room is hearing.
- **MIDI:** controllers are detected and connected automatically, with LED
  feedback. A device exposed twice (e.g. through PipeWire's Midi-Bridge)
  only connects once. With the mapping set to **Automatic** (the default),
  the mapping that matches the connected controller is used. Otherwise the
  generic one is.
  - Built-in mapping for the **Numark DJ2GO2 / DJ2GO2 Touch** (left = deck A,
    right = deck B):

    | Control | Function |
    |---|---|
    | PLAY / CUE / PFL | Per deck |
    | SYNC | Go Live |
    | Pitch faders | Tempo |
    | Level knobs | Gain |
    | Crossfader | Crossfader |
    | Master / headphone knobs | Volumes |
    | LOAD A / B | Load the selected song |
    | Browse knob | Scrolls; press loads the song onto the free deck, paused (or fills the slot you're choosing for) |
    | Jog wheels | Scrub (touch = fine, edge = fast) |
    | Pads, Cues mode | Hot cues (hold PAD MODE to clear) |
    | Pads, Sampler mode | Stems on/off: vocals / drums / bass / other |
    | SYNC on the on-air deck | Fade out (press again to cancel) |

    Tempo faders use soft takeover; the crossfader always follows the hardware.
  - Any other controller works through MIDI Learn (Settings → Controllers).
- **Audio:** choose any output device and channel pair separately for the
  room and for headphones. For example, with a DJ2GO2 use channels 1–2 for
  the room and 3–4 for headphones.
  - The audio never waits for the interface. The output callbacks and the
    deck threads that feed them run at real-time priority, while analysis,
    library scans and stem separation run at the lowest priority. On Linux,
    real-time priority comes from RealtimeKit (as for PipeWire), or from
    SCHED_FIFO if your user may use it (e.g. the `audio` group). Without
    either, Orpheus logs a warning and runs at normal priority.
- **Loading:** double-clicking a song (or pressing the browse knob) loads it
  onto the off-air deck, paused; the headphones button plays it there. A
  preview never plays if the crossfader lets the room hear that deck (e.g.
  with the crossfader in the middle): the song is loaded paused instead.
- **Tempo:** pitch is preserved while changing tempo. Four time-stretch
  engines (Settings → General): Signalsmith Stretch (default), Rubber Band
  R3 (highest quality, more CPU), SoundTouch (crisp rhythm) and Bungee. If
  slowed-down songs sound smeared, try Rubber Band or SoundTouch.
  BPM is detected automatically. The Info tab adds tap tempo and ½ / ⅔ /
  1½ / 2× fixes for octave and triple-meter mistakes. After editing a BPM,
  **↺ Analysed** or **↺ File tag** puts back the detected or tagged value.
- **Stems:** songs can be split into vocals, drums, bass and other
  (HT-Demucs via [stem-splitter-core](https://github.com/gentij/stem-splitter-core)),
  e.g. to mute the vocals of a teaching track.
  - Separation takes a minute or two per song. It runs in the background at
    low priority, needs a one-time 210 MB model download and an internet
    connection while preparing.
  - Prepared stems are cached on this computer (about 85 MB per 3-minute
    song) and work offline.
  - With every stem on you hear the untouched original. Each version (or
    setlist entry) can set which stems start muted.
  - Local files only.
- **Silence and levels:** silence at the start and end of songs is skipped,
  and there is optional automatic loudness levelling.

## Auto mode and announcements

Off by default; turn it on in Settings → Auto Mode (or with the **Auto**
button under Play Next for the current session).

- **Auto mode** starts the next song by itself after the pause.
- **Announcements:** auto mode can also announce the next dance: the song
  ends, a pause, a spoken announcement, another pause, then the next song.
  - You write the sentences, e.g. "The next dance is a {dance}." or
    "Please find a partner for a {dance}.". One is picked at random each
    time, never the same twice in a row.
  - `{dance}` is the next dance as the projector names it (the club's style
    names, or the version's display name). Songs without a style just get
    the pauses.
  - Announcements play in the room and in the headphones. The speaker
    button under Play Next announces the next dance whenever you like.
  - Speech comes from [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M),
    an open-weights text-to-speech model that runs inside Orpheus (on the
    same ONNX Runtime as stem separation; no extra program). Settings
    downloads the model (160 MB, or 325 MB at full precision) and a voice
    (0.5 MB each) into the app data folder. Voices: English (US and UK),
    Spanish, French, Italian, Brazilian Portuguese and Hindi. Downloading
    needs the internet; speaking doesn't. The model is loaded ahead of time
    and released after 15 minutes without announcements.
  - **Pronunciation:** if the voice says a word wrong (a dance name, say),
    add it under Settings → Auto Mode → Pronunciation with how to say it:
    a respelling ("Schottische" → "shot-eesh") or phonemes between slashes
    (`/ʃɑtˈiʃ/`, Kokoro's symbols; "phonemes" shows what the voice reads).
    Matching is by whole word, ignoring case. The list is saved in the
    library's `styles.json`, so it travels with your library.
- **Per entry** (Info tab, "Auto mode", for an entry in a set):
  - the pause before it;
  - what is announced: a random sentence, your own text (`{dance}`
    allowed), or nothing;
  - the pause after the announcement;
  - and whether auto mode **stops before it**. You can also right-click it
    and choose **Auto Mode Stops Before This Song**, e.g. before a mixer
    that needs explaining. Auto mode waits there until you press Play Next.
- **Pause entries** (the **Pause** button under a set) have no song. Use
  them for a break, an extra pause, an announcement ("We'll take a
  10-minute break") or a stop. Auto mode runs their pause and announcement
  and moves on. With Play Next, a pause entry says its announcement and is
  done.

## Spotify

Settings → Spotify → **Connect** signs you in through the browser. A
Premium account is required, and playback uses
[librespot](https://github.com/librespot-org/librespot). Spotify tracks
get the same tempo control, fades, preview and projector support as local
files. You can:

- search Spotify, either in the Spotify view or from the library search
  box, which shows Spotify results under your local matches;
- paste a track, album or playlist link;
- browse your playlists;
- turn a playlist into a setlist;
- link a Spotify track to an existing local track.

Previewing a Spotify song, or adding it to a set, does **not** add it to
your library. It's kept just for the setlists that use it, and removed
once none do. Use **+** (or "Add to Library" in the Info tab) to keep it.

Spotify needs an internet connection while playing.

Search and playlists use Spotify's Web API. By default they use the client
ID librespot shares with many other apps, and Spotify rate-limits it
("Spotify is limiting requests"). To avoid that, create your own app at
[developer.spotify.com](https://developer.spotify.com/dashboard) with the
redirect URI `http://127.0.0.1:8898/login` and the Web API enabled. Enter
its client ID under Settings → Spotify → Search app, then press
**Sign In for Search**. Playback keeps using your normal sign-in. Spotify
waveforms and BPMs are analysed the first time a song is loaded onto a
deck.

## Development

You need Rust (stable), Node 20+, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.
You also need **libclang**, which `signalsmith-stretch` uses for bindgen:

| OS | libclang |
|---|---|
| macOS | Included with the Xcode command-line tools. |
| Linux | Your distribution's `clang` package. |
| Windows | Install LLVM. |

```sh
npm install
npm run tauri dev        # run the app
npm run tauri build      # build an installer / .app
npm test                 # frontend tests (style mapping, smart crates)
cd src-tauri && cargo test --release   # backend tests
```

Useful environment variables:

- `ORPHEUS_DATA_DIR`: use a separate data folder, e.g. for testing.
- `ORPHEUS_SPOTIFY_CLIENT_ID`: your own Spotify app for search and playlists, if none is set in Settings.

The developer examples in `src-tauri/examples/` work without the UI:

- `scan` scans a folder into a scratch library and prints BPM and identity
  results.
- `render` renders a file through the tempo/stretch pipeline and reports
  the output BPM and duration.
- `devices` lists audio outputs.
- `meter` prints detected BPM and meter evidence for files.
- `stretch` renders an excerpt at a tempo with every time-stretch engine,
  for listening tests.
- `say` speaks a sentence with the announcer voice into a WAV file.

### Layout

```
src-tauri/src/
  app.rs        app state, settings, audio routing, loading, background jobs
  perform.rs    two-deck model: crossfader, Play Next / Go Live, auto-load, pauses, set history, projector state
  controls.rs   deck controls (CUE, hot cues, scrubbing, edit points) and controller actions
  store.rs      file-based library (songs, recordings, setlists, crates, styles; history derived from setlists)
  club.rs       club guidelines: TOML format, validation, versions, import from files and the web
  fingerprint.rs  Chromaprint fingerprints: compute, fpcalc's text form, same-version test
  library.rs    folder scanning, content-addressed import, analysis jobs
  decode.rs     symphonia decoding + audio-content hashing
  analysis.rs   BPM, 3-band waveform, loudness, silence detection
  audio/        engine: decks (stretch + SRC + fades) → lock-free rings → device mixer (cpal); stretch engines; thread priorities
  spotify.rs    librespot session, per-deck players, metadata, Web API search
  stems.rs      background stem separation and cache
  tts.rs        Kokoro text-to-speech for announcements (download, phonemes, synthesis)
  midi.rs       midir hot-plug, mappings (DJ2GO2 built in), learn, LEDs
  commands.rs   Tauri commands
examples/clubs/ example club guidelines (Dancebreak, Jammix, Friday Night Waltz), bundled with the app
src/
  components/   main window (decks, transport, library, set panel, inspector, settings)
  components/projector/  public display
  lib/clubs.ts  matching songs to club styles and checking sets (the only implementation; used by both windows)
  lib/styles.ts what a song shows: your styles, or the club's
  lib/smart.ts  smart crate rules
  lib/tempo.ts  tempo/duration helpers shared with the backend's logic
```

## License

Orpheus is licensed under the GNU General Public License, version 3 or
later (it uses the GPL Rubber Band Library).

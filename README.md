# Orpheus

DJ software for **social dancing**: waltz, swing, Latin, fusion and more,
where each song is a separate dance with a pause in between. It follows
Richard Powers' [DJ tips](https://socialdance.stanford.edu/Syllabi/DJtips.htm)
and ships with a profile for Stanford's
[Dancebreak](https://dancebreak.stanford.edu/info/djing/). Other clubs can
change every detail.

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
  headphones. **Play Next** and **Go Live** move the crossfader for you once
  the previous song has faded, so songs don't overlap unless you crossfade
  by hand.
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
  - Preset sets show the whole setlist with your position in it.
  - During the pause it shows the next dance, so people can find a partner.
  - It can also show announcements, such as "No street shoes on the floor".

## Concepts

| | |
|---|---|
| **Track** | A recording: title, artist, BPM, loudness and waveform. |
| **Source** | Where a track's audio comes from: a local file or a Spotify track. A track can have both. |
| **Version** | A playable configuration of a track: dance styles, tempo, start, end and fade points, gain, stem defaults, comments and tags. One song can be a Rotary Waltz at 150 BPM and a Fast Waltz at 168 BPM. You can also add a slowed "teaching" version. |
| **Your dance styles** | Labels on versions, not places in the library. You define them in Settings → Dance Styles. A version can have several, e.g. a song that works as both Lindy Hop and East Coast Swing. Every view (all songs, a crate, a setlist, history) has a style filter bar: click a style to narrow the view, ⌘/Shift-click to combine styles. With one style chosen, unlabelled songs whose tempo fits it are suggested too. Drop songs on a style to add it; use the right-click menu to remove one. |
| **Club styles** | Each club profile has its own styles and tempo ranges. For each club, ordered **mapping rules** say how your styles count there, optionally by planned tempo. For example, your "Waltz" from 105 to 125 BPM is a Cross-Step Waltz, from 135 to 160 BPM a Rotary Waltz, and otherwise not danced. The first matching rule wins. Ranges are matched in their own counting, so half- and double-time BPMs count. Without a rule, a club style with the same key or name is used. The **No club** profile has no styles of its own and shows yours unchanged. |
| **Crate** | Your own collection of songs: "Waltzes", "Dancebreak classics", "Pre-approved rotary waltzes", "Wedding requests". Crates can mix styles freely. Drag songs onto a crate to add them. |
| **Smart crate** | A crate defined by rules instead of a hand-picked list, e.g. "in *Waltzes* and tempo fits Rotary Waltz", "Lindy Hop, not played in the last 30 days", "longer than 4:30". It updates as the library changes. |
| **Setlist** | An ordered list of versions, or of **style slots** ("a West Coast Swing goes here") that you fill live. Each setlist uses one club. Each entry can override its version's settings for that set only. It can also show just some of a song's styles, or a custom label such as "Mixer" instead of style names. Songs already played in the open set are marked in the library and the set. |
| **Club profile** | The club's dance styles and tempo ranges, pause length, fade length, tags (e.g. "Dance with a Stranger", "Jam") and announcements. |
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

The library folder holds tracks, versions, setlists, crates and club
profiles as pretty-printed JSON, one file per object. Your dance styles and
their club mappings are in `styles.json`. Play history and play counts are
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
    | Browse knob | Scrolls; press auditions on the free deck |
    | Jog wheels | Scrub (touch = fine, edge = fast) |
    | Pads, Cues mode | Hot cues (hold PAD MODE to clear) |
    | Pads, Sampler mode | Stems on/off: vocals / drums / bass / other |
    | SYNC on the on-air deck | Fade out (press again to cancel) |

    Faders and the crossfader use soft takeover.
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
- **Tempo:** pitch is preserved while changing tempo (Signalsmith Stretch).
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

### Layout

```
src-tauri/src/
  app.rs        app state, settings, audio routing, loading, background jobs
  perform.rs    two-deck model: crossfader, Play Next / Go Live, auto-load, pauses, set history, projector state
  controls.rs   deck controls (CUE, hot cues, scrubbing, edit points) and controller actions
  store.rs      file-based library (tracks, setlists, crates, profiles, styles; history derived from setlists)
  library.rs    folder scanning, content-addressed import, analysis jobs
  decode.rs     symphonia decoding + audio-content hashing
  analysis.rs   BPM, 3-band waveform, loudness, silence detection
  audio/        engine: decks (stretch + SRC + fades) → lock-free rings → device mixer (cpal); thread priorities
  spotify.rs    librespot session, per-deck players, metadata, Web API search
  stems.rs      background stem separation and cache
  midi.rs       midir hot-plug, mappings (DJ2GO2 built in), learn, LEDs
  commands.rs   Tauri commands
  profiles/     built-in club profiles (No club, Dancebreak, Powers' DJ tips)
src/
  components/   main window (decks, transport, library, set panel, inspector, settings)
  components/projector/  public display
  lib/styles.ts your styles → club styles (the only implementation; used by both windows)
  lib/smart.ts  smart crate rules
  lib/tempo.ts  tempo/duration helpers shared with the backend's logic
```

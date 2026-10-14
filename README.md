# Orpheus

DJ software for **social dancing**: waltz, swing, Latin, fusion and more,
where each song is a separate dance with a pause in between. It follows
Richard Powers' [DJ tips](https://socialdance.stanford.edu/Syllabi/DJtips.htm)
and ships with a profile for Stanford's
[Dancebreak](https://dancebreak.stanford.edu/info/djing/). Other clubs can
change every detail.

Runs on macOS, Linux and Windows (Tauri 2, Rust and Svelte).

## How it differs from club DJ software

- **No crossfading.** Songs never overlap. Each one ends, or fades out, and
  a pause follows.
- **The next song is manual by default.** Press **Play Next** when the floor
  is ready. If a song is still playing, Play Next asks for a second press,
  then fades the song out before continuing. Auto-advance after a set pause
  is optional.
- **Two decks:**
  - **On Air** plays to the room.
  - **Preview** plays only in your headphones, for auditioning songs.
  - There is no crossfader.
- **Dance styles and tempos come first.** The library is organised around the
  styles a club dances. Each style has a sweet-spot and an acceptable tempo
  range.
- **Projector display for dancers.**
  - Live sets show the current dance, the song and recent songs.
  - Preset sets show the whole setlist with your position in it.
  - During the pause it shows the next dance, so people can find a partner.
  - It can also show announcements, such as "No street shoes on the floor".

## Concepts

| | |
|---|---|
| **Track** | A recording: title, artist, BPM, loudness and waveform. |
| **Source** | Where a track's audio comes from: a local file or a Spotify track. A track can have both. |
| **Version** | A playable configuration of a track: dance style, tempo, start, end and fade points, gain, stem defaults, comments and tags. One song can be a Rotary Waltz at 150 BPM and a Fast Waltz at 168 BPM. You can also add a slowed "teaching" version. |
| **Setlist** | An ordered list of versions, or of **style slots** ("a West Coast Swing goes here") that you fill live. Each entry can override its version's settings for that set only. |
| **Club profile** | Dance styles, tempo ranges, pause length, fade length, tags (e.g. "Dance with a Stranger", "Jam"), announcements and **setlist rules**. Rules include song count, song length limits, per-style minimums that depend on the lesson, tempo exceptions, and required closing sequences. The **Checks** tab tests a setlist against them. |

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
profiles as pretty-printed JSON, one file per object. Play history is kept
in append-only `.jsonl` files. Keep the folder in **git** or a synced
folder to share it between DJs. IDs are random, so concurrent edits don't
collide, and history logs merge with `merge=union`. After a `git pull`, use
**Settings → Library → Reload**.

Your music files are **never modified**:

- Files are identified by a hash of their audio data, not by path or tags.
- Renaming, moving or retagging files keeps everything linked.
- If a file goes missing, its track is kept and relinks on the next scan.

Machine-specific state stays in the app data folder:

- where files live on this computer;
- waveforms;
- audio and MIDI settings;
- the Spotify login.

## Controllers and audio

- **MIDI:** controllers are detected and connected automatically, with LED
  feedback.
  - There is a built-in mapping for the **Numark DJ2GO2 / DJ2GO2 Touch**.
    The left deck is On Air, the right deck is Preview.
    - The left pitch fader sets the main tempo, with soft takeover.
    - SYNC / pad 1 = Play Next, CUE / pad 2 = fade out, pad 3 = skip,
      pad 4 = auto-advance.
    - The browse knob scrolls the library and LOAD queues songs.
    - The crossfader is deliberately unused.
  - Any other controller works through MIDI Learn (Settings → Controllers).
- **Audio:** choose any output device and channel pair separately for the
  room and for headphones. For example, with a DJ2GO2 use channels 1–2 for
  the room and 3–4 for headphones.
- **Tempo:** pitch is preserved while changing tempo (Signalsmith Stretch).
  BPM is detected automatically. The Info tab adds tap tempo and ½ / ⅔ /
  1½ / 2× fixes for octave and triple-meter mistakes. After editing a BPM,
  **↺ Analysed** or **↺ File tag** puts back the detected or tagged value.
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
npm test                 # frontend tests (setlist rules)
cd src-tauri && cargo test --release   # backend tests
```

Useful environment variables:

- `ORPHEUS_DATA_DIR`: use a separate data folder, e.g. for testing.
- `ORPHEUS_SPOTIFY_CLIENT_ID`: use your own Spotify app.

The developer examples in `src-tauri/examples/` work without the UI:

- `scan` scans a folder into a scratch library and prints BPM and identity
  results.
- `render` renders a file through the tempo/stretch pipeline and reports
  the output BPM and duration.
- `devices` lists audio outputs.

### Layout

```
src-tauri/src/
  app.rs        performance logic: Play Next, pauses, fades, auto-advance, projector state, MIDI actions
  store.rs      file-based library (tracks, setlists, crates, profiles, history)
  library.rs    folder scanning, content-addressed import, analysis jobs
  decode.rs     symphonia decoding + audio-content hashing
  analysis.rs   BPM, 3-band waveform, loudness, silence detection
  audio/        engine: decks (stretch + SRC + fades) → lock-free rings → device mixer (cpal)
  spotify.rs    librespot session, per-deck players, metadata, Web API search
  midi.rs       midir hot-plug, mappings (DJ2GO2 built in), learn, LEDs
  commands.rs   Tauri commands
  profiles/     built-in club profiles (Dancebreak, Powers' DJ tips)
src/
  components/   main window (decks, transport, library, set panel, inspector, settings)
  components/projector/  public display
  lib/rules.ts  setlist rule checks
  lib/tempo.ts  tempo/duration helpers shared with the backend's logic
```

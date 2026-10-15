<script lang="ts">
  // Combine (Same Song): songs become one song, each version keeping its
  // recording. Combine (Same Recording): recordings become one (the same
  // audio), and their songs one song. Where they differ (title, BPM, …), the
  // DJ picks which value to keep. Undone by separating again (Inspector).
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { fmtBpm } from "../lib/tempo";
  import type { Id, Recording, RecordingPatch, Track, TrackPatch } from "../lib/types";

  /** The songs to combine (the first kept), and for Same Recording the recordings (the first kept). */
  let { songs: songIds, recordings: recIds }: { songs: Id[]; recordings: Id[] } = $props();

  interface Field {
    key: string;
    label: string;
    /** Each item's value, shown and compared as text. */
    values: { id: Id; text: string; of: string }[];
  }

  const songs = $derived(songIds.map((id) => app.trackMap.get(id)).filter(Boolean) as Track[]);
  const recordings = $derived(recIds.map((id) => app.recordingMap.get(id)).filter(Boolean) as Recording[]);
  const sameRecording = $derived(recordings.length > 1);
  const meter = (r: Recording) => (r.beatsPerBar ? `${r.beatsPerBar} beats a bar${r.meterSource === "user" ? " (set by you)" : ""}` : "unknown");
  /** "FLAC file + Spotify": tells recordings with the same title apart. */
  const sourceText = (r: Recording) => {
    const ext = r.files[0]?.match(/\.([a-z0-9]+)$/i)?.[1]?.toUpperCase();
    return r.sources.map((s) => (s.kind === "spotify" ? "Spotify" : ext ? `${ext} file` : "file")).join(" + ");
  };

  const fields = $derived.by((): Field[] => {
    const out: Field[] = [];
    const add = (key: string, label: string, values: Field["values"]) => {
      if (new Set(values.map((v) => v.text)).size > 1) out.push({ key, label, values });
    };
    if (songs.length > 1) {
      const of = (t: Track) => t.title;
      add("title", "Title", songs.map((t) => ({ id: t.id, text: t.title, of: of(t) })));
      add("artist", "Artist", songs.map((t) => ({ id: t.id, text: t.artist, of: of(t) })));
      add("album", "Album", songs.map((t) => ({ id: t.id, text: t.album, of: of(t) })));
      add("genre", "Genre", songs.map((t) => ({ id: t.id, text: t.genre ?? "", of: of(t) })));
      add("rating", "Rating", songs.map((t) => ({ id: t.id, text: t.rating ? "★".repeat(t.rating) : "", of: of(t) })));
      add("notes", "Notes", songs.map((t) => ({ id: t.id, text: t.notes ?? "", of: of(t) })));
    }
    if (sameRecording) {
      const of = (r: Recording) => `${r.title} (${sourceText(r)})`;
      add("bpm", "BPM", recordings.map((r) => ({ id: r.id, text: `${fmtBpm(r.bpm)}${r.bpmSource ? ` (${r.bpmSource})` : ""}`, of: of(r) })));
      add("meter", "Meter", recordings.map((r) => ({ id: r.id, text: meter(r), of: of(r) })));
      add("gainDb", "Gain", recordings.map((r) => ({ id: r.id, text: `${r.gainDb ?? 0} dB`, of: of(r) })));
      add("cues", "Hot cues", recordings.map((r) => ({ id: r.id, text: (r.hotCues ?? []).map((c) => c.slot).join(", ") || "none", of: of(r) })));
    }
    return out;
  });

  /** Field → the id whose value is kept (default: the first item's). */
  let picks = $state<Record<string, Id>>({});
  const pick = (f: Field) => picks[f.key] ?? f.values[0].id;
  const has = (key: string) => fields.some((f) => f.key === key);

  let busy = $state(false);
  async function merge() {
    busy = true;
    try {
      if (sameRecording) {
        // Hot cues come from the kept recording: keep the chosen one's by keeping it.
        const kept = recordings.find((r) => r.id === picks["cues"]) ?? recordings[0];
        const from = (f: string) => recordings.find((r) => r.id === (picks[f] ?? recordings[0].id))!;
        const patch: RecordingPatch = {};
        if (has("bpm")) { patch.bpm = from("bpm").bpm; patch.bpmSource = from("bpm").bpmSource ?? "user"; }
        if (has("meter")) patch.beatsPerBar = from("meter").beatsPerBar ?? null;
        if (has("gainDb")) patch.gainDb = from("gainDb").gainDb ?? 0;
        const ok = await app.run(api.mergeRecordings(kept.id, recordings.filter((r) => r.id !== kept.id).map((r) => r.id)));
        if (!ok) return;
        if (Object.keys(patch).length) await app.run(api.updateRecording(kept.id, patch));
      }
      if (songs.length > 1) {
        const [keep, ...others] = songs;
        const from = (f: string) => songs.find((t) => t.id === (picks[f] ?? keep.id))!;
        const patch: TrackPatch = {};
        if (has("title")) patch.title = from("title").title;
        if (has("artist")) patch.artist = from("artist").artist;
        if (has("album")) patch.album = from("album").album;
        if (has("genre")) patch.genre = from("genre").genre ?? "";
        if (has("rating")) patch.rating = from("rating").rating;
        if (has("notes")) patch.notes = from("notes").notes ?? "";
        const ok = await app.run(api.combineSongs(keep.id, others.map((t) => t.id)));
        if (!ok) return;
        if (Object.keys(patch).length) await app.run(api.updateTrack(keep.id, patch));
      }
      app.mergeDialog = null;
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (app.mergeDialog = null)}>
  <form class="sheet" onsubmit={(e) => { e.preventDefault(); merge(); }}>
    {#if sameRecording}
      <h2>Combine as the Same Recording</h2>
      <p class="dim">
        Their sources ({recordings.map(sourceText).join("; ")}) become one recording: the same audio, analysed once, with one
        BPM and one set of hot cues.{#if songs.length > 1}{" "}Their {songs.length} songs become one song.{/if} Setlists and crates
        keep working. To undo, split a source off in Info → Recording; “Separate into Its Own Song” makes a version a song of
        its own again (it keeps playing this recording).
      </p>
    {:else}
      <h2>Combine {songs.length} Songs as the Same Song</h2>
      <p class="dim">
        Their versions become versions of one song (each keeps its own recording), and count as the same song in a set.
        Setlists and crates keep working. To undo, select a version and choose “Separate into Its Own Song”.
      </p>
    {/if}
    {#if fields.length}
      <p>They differ in these; choose what to keep:</p>
      <div class="fields">
        {#each fields as f (f.key)}
          <span class="l">{f.label}</span>
          <div class="col">
            {#each f.values as v (v.id)}
              <label>
                <input type="radio" name={f.key} checked={pick(f) === v.id} onchange={() => (picks[f.key] = v.id)} />
                <span>{v.text || "(empty)"}</span>
                {#if v.of !== v.text}<span class="faint small">— {v.of}</span>{/if}
              </label>
            {/each}
          </div>
        {/each}
      </div>
    {:else}
      <p class="faint">Nothing differs between them, so nothing is lost.</p>
    {/if}
    <div class="buttons">
      <button type="button" onclick={() => (app.mergeDialog = null)}>Cancel</button>
      <button type="submit" class="primary" disabled={busy}>Combine</button>
    </div>
  </form>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 80; background: rgba(0,0,0,0.45); display: flex; justify-content: center; align-items: flex-start; }
  .sheet { margin-top: 60px; width: 600px; max-height: calc(100vh - 120px); overflow: auto; padding: 20px 22px; background: var(--bg-elev); border: 1px solid var(--border-strong); border-radius: 12px; box-shadow: var(--shadow); }
  h2 { margin: 0 0 10px; font: 600 var(--fs-lg) var(--font-display); }
  p { margin: 0 0 12px; }
  .fields { display: grid; grid-template-columns: 80px 1fr; gap: 10px 12px; align-items: start; }
  .l { color: var(--text-dim); text-align: right; padding-top: 2px; }
  .col { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .col label { display: flex; gap: 6px; align-items: baseline; }
  .col label span:first-of-type { overflow-wrap: anywhere; }
  .small { font-size: var(--fs-sm); }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
</style>

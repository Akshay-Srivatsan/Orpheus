<script lang="ts">
  // Combine: songs become one song. If their audio is the same (FLAC and MP3,
  // a file and its Spotify track: suggested by comparing fingerprints), it
  // becomes one audio too; otherwise each version keeps its own. Where they
  // differ (title, BPM, …), the DJ picks which value to keep. Undone by
  // separating again (Inspector).
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { fmtBpm } from "../lib/tempo";
  import type { Id, Recording, RecordingPatch, Track, TrackPatch } from "../lib/types";

  /** The songs to combine (the first kept), and the audio they play (the first kept, if it's the same). */
  let { songs: songIds, recordings: recIds }: { songs: Id[]; recordings: Id[] } = $props();

  interface Field {
    key: string;
    label: string;
    /** Each item's value, shown and compared as text. */
    values: { id: Id; text: string; of: string }[];
  }

  const songs = $derived(songIds.map((id) => app.trackMap.get(id)).filter(Boolean) as Track[]);
  const recordings = $derived(recIds.map((id) => app.recordingMap.get(id)).filter(Boolean) as Recording[]);
  /** The audio is the same (combine it too): suggested by its fingerprints, the DJ's to change. */
  let sameAudio = $state(false);
  let verdict = $state<"checking" | "same" | "different" | "unknown" | null>(null);
  let match = $state(0);
  /** Over the stretch that lines up, how much the audio differs (bit error). */
  let diff = $state(0);
  $effect(() => {
    if (recIds.length < 2) return;
    verdict = "checking";
    api.compareRecordings(recIds).then((m) => {
      const known = m.every((x) => x.known);
      match = Math.min(...m.map((x) => x.coverage));
      diff = Math.max(...m.map((x) => x.error));
      verdict = !known ? "unknown" : m.every((x) => x.same) ? "same" : "different";
      sameAudio = verdict === "same";
    }).catch(() => (verdict = "unknown"));
  });
  const sameRecording = $derived(recordings.length > 1 && sameAudio);
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
      const what = sameRecording ? "Combined as the same audio" : `Combined ${songs.length} songs`;
      const done = await app.undoable(what, async () => {
        if (sameRecording) {
          // Hot cues come from the kept recording: keep the chosen one's by keeping it.
          const kept = recordings.find((r) => r.id === picks["cues"]) ?? recordings[0];
          const from = (f: string) => recordings.find((r) => r.id === (picks[f] ?? recordings[0].id))!;
          const patch: RecordingPatch = {};
          if (has("bpm")) { patch.bpm = from("bpm").bpm; patch.bpmSource = from("bpm").bpmSource ?? "user"; }
          if (has("meter")) patch.beatsPerBar = from("meter").beatsPerBar ?? null;
          if (has("gainDb")) patch.gainDb = from("gainDb").gainDb ?? 0;
          const ok = await app.run(api.mergeRecordings(kept.id, recordings.filter((r) => r.id !== kept.id).map((r) => r.id)));
          if (!ok) return undefined;
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
          if (!ok) return undefined;
          if (Object.keys(patch).length) await app.run(api.updateTrack(keep.id, patch));
        }
        return true;
      });
      if (done) app.mergeDialog = null;
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (app.mergeDialog = null)}>
  <form class="sheet" onsubmit={(e) => { e.preventDefault(); merge(); }}>
    <h2>Combine{songs.length > 1 ? ` ${songs.length} Songs` : ""}</h2>
    {#if recordings.length > 1}
      <p class="small">
        {#if verdict === "checking"}<span class="faint">Comparing their audio…</span>
        {:else if verdict === "same"}<span class="ok">Their audio is the same{match ? ` (${Math.round(match * 100)}% match)` : ""}</span>: {recordings.map(sourceText).join(", ")}.
        {:else if verdict === "different"}<span>Their audio is different{match >= 0.95 ? ` (it lines up all through, but ${Math.round(diff * 100)}% of the detail differs: a remaster, or a lossy copy?)` : match > 0.05 ? ` (only ${Math.round(match * 100)}% of it lines up: an edit?)` : ""}.</span>
        {:else}<span class="faint">Not all of it is analysed yet, so Orpheus can't compare it.</span>{/if}
      </p>
      <div class="choices">
        <label><input type="radio" checked={!sameAudio} onchange={() => (sameAudio = false)} disabled={songs.length < 2} />
          <span><b>Different audio</b> — one song, each version keeping its own audio (e.g. several edits for one line dance){#if songs.length < 2} <span class="faint">(they're already one song)</span>{/if}</span></label>
        <label><input type="radio" checked={sameAudio} onchange={() => (sameAudio = true)} />
          <span><b>The same audio</b> — one song and one audio (e.g. a FLAC and an MP3, or a file and its Spotify track): analysed once, one BPM, one set of hot cues; versions that become identical merge</span></label>
      </div>
    {/if}
    <p class="dim small">
      {songs.length > 1 ? "Their versions become versions of one song, and count as the same song in a set. " : ""}Setlists and crates keep working.
      To undo: “Separate into Its Own Song” (a version), or Split off… (a file or Spotify link, in its Audio).
    </p>
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
      <button type="submit" class="primary" disabled={busy || verdict === "checking" || (songs.length < 2 && !sameAudio)}>Combine</button>
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
  .ok { color: var(--green); }
  .choices { display: flex; flex-direction: column; gap: 6px; margin: 0 0 12px; }
  .choices label { display: flex; gap: 8px; align-items: flex-start; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
</style>

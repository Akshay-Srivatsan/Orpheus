<script lang="ts">
  // Combining songs into one, or marking recordings as the same audio. Where
  // they differ (title, BPM, …), the DJ picks which value to keep; the rest is
  // undone by separating again (Inspector).
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { fmtBpm } from "../lib/tempo";
  import type { Id, Recording, RecordingPatch, Track, TrackPatch } from "../lib/types";

  let { kind, ids }: { kind: "songs" | "recordings"; ids: Id[] } = $props();

  interface Field {
    key: string;
    label: string;
    /** Each item's value, shown and compared as text. */
    values: { id: Id; text: string; of: string }[];
  }

  const songs = $derived(kind === "songs" ? (ids.map((id) => app.trackMap.get(id)).filter(Boolean) as Track[]) : []);
  const recordings = $derived.by((): Recording[] => {
    if (kind !== "recordings") return [];
    return ids.map((id) => app.recordingMap.get(id)).filter(Boolean) as Recording[];
  });
  const meter = (r: Recording) => (r.beatsPerBar ? `${r.beatsPerBar} beats a bar${r.meterSource === "user" ? " (set by you)" : ""}` : "unknown");
  const sourceText = (r: Recording) => r.sources.map((s) => (s.kind === "spotify" ? "Spotify" : "file")).join(" + ");

  const fields = $derived.by((): Field[] => {
    const out: Field[] = [];
    const add = (key: string, label: string, values: Field["values"]) => {
      if (new Set(values.map((v) => v.text)).size > 1) out.push({ key, label, values });
    };
    if (kind === "songs") {
      const of = (t: Track) => t.title;
      add("title", "Title", songs.map((t) => ({ id: t.id, text: t.title, of: of(t) })));
      add("artist", "Artist", songs.map((t) => ({ id: t.id, text: t.artist, of: of(t) })));
      add("album", "Album", songs.map((t) => ({ id: t.id, text: t.album, of: of(t) })));
      add("genre", "Genre", songs.map((t) => ({ id: t.id, text: t.genre ?? "", of: of(t) })));
      add("rating", "Rating", songs.map((t) => ({ id: t.id, text: t.rating ? "★".repeat(t.rating) : "", of: of(t) })));
      add("notes", "Notes", songs.map((t) => ({ id: t.id, text: t.notes ?? "", of: of(t) })));
    } else {
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

  let busy = $state(false);
  async function merge() {
    busy = true;
    try {
      if (kind === "songs") {
        const [keep, ...others] = songs;
        const from = (f: string) => songs.find((t) => t.id === (picks[f] ?? keep.id))!;
        const patch: TrackPatch = {};
        for (const f of fields) {
          const t = from(f.key);
          if (f.key === "title") patch.title = t.title;
          if (f.key === "artist") patch.artist = t.artist;
          if (f.key === "album") patch.album = t.album;
          if (f.key === "genre") patch.genre = t.genre ?? "";
          if (f.key === "rating") patch.rating = t.rating;
          if (f.key === "notes") patch.notes = t.notes ?? "";
        }
        await app.run(api.combineSongs(keep.id, others.map((t) => t.id)));
        if (Object.keys(patch).length) await app.run(api.updateTrack(keep.id, patch));
      } else {
        const [keep, ...others] = recordings;
        const from = (f: string) => recordings.find((r) => r.id === (picks[f] ?? keep.id))!;
        const patch: RecordingPatch = {};
        for (const f of fields) {
          const r = from(f.key);
          if (f.key === "bpm") { patch.bpm = r.bpm; patch.bpmSource = r.bpmSource ?? "user"; }
          if (f.key === "meter") patch.beatsPerBar = r.beatsPerBar ?? null;
          if (f.key === "gainDb") patch.gainDb = r.gainDb ?? 0;
        }
        // Hot cues come from the kept recording: keep the chosen one's by keeping it.
        const cuesFrom = picks["cues"];
        const kept = cuesFrom && cuesFrom !== keep.id ? recordings.find((r) => r.id === cuesFrom)! : keep;
        const rest = recordings.filter((r) => r.id !== kept.id);
        await app.run(api.mergeRecordings(kept.id, rest.map((r) => r.id)));
        if (Object.keys(patch).length) await app.run(api.updateRecording(kept.id, patch));
      }
      app.mergeDialog = null;
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (app.mergeDialog = null)}>
  <form class="sheet" onsubmit={(e) => { e.preventDefault(); merge(); }}>
    {#if kind === "songs"}
      <h2>Combine {songs.length} Songs into One</h2>
      <p class="dim">
        Their versions become versions of one song (each keeps its own recording), and count as the same song in a set.
        Setlists and crates keep working. To undo, select a version and choose “Separate into Its Own Song”.
      </p>
    {:else}
      <h2>Mark {recordings.length} Recordings as the Same Audio</h2>
      <p class="dim">
        Their sources ({recordings.map(sourceText).join("; ")}) become one recording, and every version playing them plays it.
        The songs stay as they are. To undo, split a source off in Info → Recording.
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
      <button type="submit" class="primary" disabled={busy}>{kind === "songs" ? "Combine" : "Mark as Same"}</button>
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

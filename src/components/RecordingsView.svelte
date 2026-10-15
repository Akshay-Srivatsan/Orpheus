<script lang="ts">
  // Library → Recordings: every recording, with the songs that play it.
  // Recordings no song plays are hidden from the library; a song can be made
  // for one again, or (if its file isn't in the library folder) it can be deleted.
  import { api, on } from "../lib/api";
  import { app, confirmAction } from "../lib/state.svelte";
  import { fmtBpm, fmtTime } from "../lib/tempo";
  import type { Recording } from "../lib/types";
  import Icon from "./Icon.svelte";

  let all = $state<Recording[]>([]);
  let hiddenOnly = $state(true);

  async function load() {
    all = (await app.run(api.recordings())) ?? [];
  }
  $effect(() => {
    void load();
    const offs = [on("recordings-changed", load), on("library-changed", load)];
    return () => offs.forEach((p) => p.then((f) => f()));
  });

  const shown = $derived.by(() => {
    const q = app.search.trim().toLowerCase();
    return all
      .filter((r) => !hiddenOnly || !r.songs.length)
      .filter((r) => !q || `${r.title} ${r.artist} ${r.album}`.toLowerCase().includes(q));
  });
  const hiddenCount = $derived(all.filter((r) => !r.songs.length).length);

  async function makeSong(r: Recording) {
    const t = await app.run(api.songFromRecording(r.id));
    if (t) {
      app.inspectTrack = t.id;
      app.rightTab = "info";
    }
  }

  async function remove(r: Recording) {
    if (await confirmAction(`Delete the recording “${r.title}”? It can be added again from Spotify.`)) await app.run(api.deleteRecordings([r.id]));
  }

  const sourceText = (r: Recording) => r.sources.map((s) => (s.kind === "spotify" ? "Spotify" : "file")).join(" + ");
</script>

<div class="recs">
  <div class="bar">
    <label class="small"><input type="checkbox" bind:checked={hiddenOnly} /> Only hidden ({hiddenCount})</label>
    <span class="faint small">A recording no song plays is hidden from the library; its file stays in your library folder.</span>
  </div>
  <div class="table">
    <div class="row head"><span>Title</span><span>Artist</span><span class="num">Time</span><span class="num">BPM</span><span>Source</span><span>Songs</span><span></span></div>
    {#each shown as r (r.id)}
      <div class="row">
        <span class="t" title={r.title}>{#if r.missing}<span title="File missing on this computer"><Icon name="warning" size={12} /></span>{/if}{r.title}</span>
        <span class="t" title={r.artist}>{r.artist}</span>
        <span class="num">{fmtTime(r.durationMs)}</span>
        <span class="num">{fmtBpm(r.bpm)}</span>
        <span class="faint" title={r.files.join("\n")}>{sourceText(r)}</span>
        <span class="t">
          {#if r.songs.length}
            {#each r.songs as id, i (id)}{i ? ", " : ""}<button class="plain link" onclick={() => { app.inspectTrack = id; app.rightTab = "info"; }}>{app.trackMap.get(id)?.title ?? id}</button>{/each}
          {:else}<span class="faint">hidden</span>{/if}
        </span>
        <span class="actions">
          <button class="small" title="A new song (in your library) playing this recording" onclick={() => makeSong(r)}>Make a Song</button>
          {#if !r.songs.length && !r.files.length}
            <button class="plain icon danger" title="Delete this recording" onclick={() => remove(r)}><Icon name="trash" size={12} /></button>
          {/if}
        </span>
      </div>
    {:else}
      <div class="empty faint">{hiddenOnly ? "No hidden recordings: every recording is played by a song." : "No recordings."}</div>
    {/each}
  </div>
</div>

<style>
  .recs { height: 100%; display: flex; flex-direction: column; min-height: 0; }
  .bar { display: flex; gap: 12px; align-items: center; padding: 6px 10px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .table { overflow: auto; flex: 1; font-size: var(--fs-sm); }
  .row { display: grid; grid-template-columns: minmax(0, 2fr) minmax(0, 1.4fr) 50px 54px 100px minmax(0, 1.6fr) 130px; gap: 8px; align-items: center; padding: 3px 10px; border-bottom: 1px solid rgba(255, 255, 255, 0.04); }
  .head { position: sticky; top: 0; background: var(--bg); color: var(--text-faint); font-weight: 600; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .actions { display: flex; gap: 4px; justify-content: flex-end; }
  .link { padding: 0; height: auto; color: var(--blue); display: inline; }
  .small { font-size: var(--fs-sm); }
  .danger { color: var(--red); }
  .empty { padding: 16px; }
</style>

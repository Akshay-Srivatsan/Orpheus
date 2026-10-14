<script lang="ts">
  // Spotify search / playlist results. Previewing or adding a song to the set
  // does NOT add it to the library; use the + button for that.
  import { api } from "../lib/api";
  import { addToSet, app, audition, spotifyVersions } from "../lib/state.svelte";
  import { fmtTime } from "../lib/tempo";
  import type { Id, SpotifyTrack } from "../lib/types";
  import Icon from "./Icon.svelte";

  interface Props { tracks: SpotifyTrack[]; compact?: boolean }
  let { tracks, compact = false }: Props = $props();

  /** Spotify URI → track, for songs Orpheus already knows. */
  const known = $derived.by(() => {
    const m = new Map<string, { id: Id; inLibrary: boolean }>();
    for (const t of app.tracks)
      for (const s of t.sources) if (s.kind === "spotify") m.set(s.key, { id: t.id, inLibrary: t.inLibrary !== false });
    return m;
  });

  async function preview(t: SpotifyTrack) {
    const [v] = await spotifyVersions([t], false);
    if (v) await audition(v);
  }

  async function toSet(t: SpotifyTrack, where: "next" | "end") {
    const vs = await spotifyVersions([t], false);
    if (vs.length) await addToSet(vs, where);
  }

  async function toLibrary(t: SpotifyTrack) {
    await spotifyVersions([t], true);
    app.toast(`Added “${t.title}” to your library`, "info");
  }

  function dragStart(e: DragEvent, t: SpotifyTrack) {
    e.dataTransfer?.setData("application/x-orpheus-spotify", JSON.stringify([t]));
    e.dataTransfer!.effectAllowed = "copy";
  }
</script>

<div class="list" class:compact>
  {#each tracks as t (t.uri)}
    {@const k = known.get(t.uri)}
    <div class="item" role="listitem" draggable="true" ondragstart={(e) => dragStart(e, t)} ondblclick={() => preview(t)}>
      {#if t.artworkUrl}<img src={t.artworkUrl} alt="" />{:else}<div class="noart"></div>{/if}
      <div class="txt">
        <div class="t">{t.title}</div>
        <div class="a dim">{t.artist}{#if !compact} · {t.album}{/if}</div>
      </div>
      <span class="num dim">{fmtTime(t.durationMs)}</span>
      <button class="plain icon" title="Preview in headphones" onclick={() => preview(t)}><Icon name="headphones" size={14} /></button>
      <button class="small" title="Play next (not added to your library)" onclick={() => toSet(t, "next")}>Next</button>
      <button class="small" title="Add to the end of the set (not added to your library)" onclick={() => toSet(t, "end")}>End</button>
      {#if k?.inLibrary}
        <span class="inlib" title="In your library"><Icon name="check" size={13} /></span>
      {:else}
        <button class="plain icon" title="Add to library" onclick={() => toLibrary(t)}><Icon name="plus" size={14} /></button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .item { display: flex; align-items: center; gap: 8px; padding: 5px 10px; border-bottom: 1px solid rgba(255,255,255,0.03); }
  .item:hover { background: var(--bg-hover); }
  img, .noart { width: 36px; height: 36px; border-radius: 4px; object-fit: cover; background: var(--bg-elev-3); flex: none; }
  .compact img, .compact .noart { width: 26px; height: 26px; }
  .compact .item { padding: 3px 10px; }
  .txt { flex: 1; min-width: 0; }
  .t, .a { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .a { font-size: var(--fs-sm); }
  .inlib { color: var(--green); display: flex; width: 26px; justify-content: center; }
  .small { font-size: var(--fs-sm); padding: 0 8px; }
</style>

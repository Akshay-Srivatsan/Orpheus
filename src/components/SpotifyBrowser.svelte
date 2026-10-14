<script lang="ts">
  import { api } from "../lib/api";
  import { addToSet, app } from "../lib/state.svelte";
  import { fmtTime } from "../lib/tempo";
  import type { Id, SpotifyTrack } from "../lib/types";
  import Icon from "./Icon.svelte";

  let query = $state("");
  let results = $state<SpotifyTrack[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const inLibrary = $derived.by(() => {
    const m = new Map<string, Id>();
    for (const t of app.tracks) for (const s of t.sources) if (s.kind === "spotify") m.set(s.key, t.id);
    return m;
  });

  const playlist = $derived(app.view.kind === "spotify-playlist" ? app.view : null);

  $effect(() => {
    const pl = playlist;
    if (!pl) return;
    loading = true;
    error = null;
    results = [];
    api.spotifyPlaylistTracks(pl.uri)
      .then((r) => { if (playlist?.uri === pl.uri) results = r; })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  });

  async function search() {
    const q = query.trim();
    if (!q) return;
    loading = true;
    error = null;
    try {
      const isLink = /^(spotify:|https?:\/\/open\.spotify\.com\/|open\.spotify\.com\/)/.test(q);
      results = isLink ? await api.spotifyResolve(q) : await api.spotifySearch(q);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function importTracks(ts: SpotifyTrack[]): Promise<Id[]> {
    return (await app.run(api.spotifyImport(ts))) ?? [];
  }

  /** Imports and returns the default version id of each track. */
  async function versionsFor(ts: SpotifyTrack[]): Promise<Id[]> {
    const ids = await importTracks(ts);
    await app.reloadTracks(ids);
    return ids.map((id) => app.trackMap.get(id)?.versions[0]?.id).filter(Boolean) as Id[];
  }

  async function preview(t: SpotifyTrack) {
    const [v] = await versionsFor([t]);
    if (v) app.run(api.previewLoad(v, { play: true }));
  }

  async function newSetFromPlaylist() {
    if (!playlist) return;
    const vids = await versionsFor(results);
    const set = await app.run(api.createSetlist({ name: playlist.name, mode: "preset", profileId: app.settings?.activeProfileId ?? "dancebreak" }));
    if (!set) return;
    await app.run(api.insertEntries(set.id, vids.map((versionId) => ({ versionId }))));
    await app.run(api.performOpen(set.id));
    app.toast(`Created setlist “${playlist.name}” with ${vids.length} songs. Label their dance styles in the set panel.`, "info");
  }
</script>

<div class="sp">
  <div class="bar">
    {#if playlist}
      <div class="pl-title"><Icon name="spotify" /> {playlist.name}</div>
      <span class="spacer"></span>
      <button disabled={!results.length} onclick={() => importTracks(results).then((ids) => ids.length && app.toast(`Added ${ids.length} songs to your library`, "info"))}>Add All to Library</button>
      <button class="primary" disabled={!results.length} onclick={newSetFromPlaylist}>New Setlist from Playlist</button>
    {:else}
      <input type="search" placeholder="Search Spotify, or paste a track / album / playlist link" bind:value={query}
        onkeydown={(e) => e.key === "Enter" && search()} style="flex:1" />
      <button onclick={search}>Search</button>
    {/if}
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  <div class="list">
    {#if loading}<div class="empty">Loading…</div>{/if}
    {#each results as t (t.uri)}
      {@const lib = inLibrary.get(t.uri)}
      <div class="item" role="listitem" ondblclick={() => preview(t)}>
        {#if t.artworkUrl}<img src={t.artworkUrl} alt="" />{:else}<div class="noart"></div>{/if}
        <div class="txt">
          <div class="t">{t.title}</div>
          <div class="a dim">{t.artist} · {t.album}</div>
        </div>
        <span class="num dim">{fmtTime(t.durationMs)}</span>
        {#if lib}<span class="inlib" title="In your library"><Icon name="check" size={13} /></span>{/if}
        <button class="plain icon" title="Preview in headphones" onclick={() => preview(t)}><Icon name="headphones" size={14} /></button>
        <button class="plain icon" title="Add to library" disabled={!!lib} onclick={() => importTracks([t])}><Icon name="plus" size={14} /></button>
        <button class="small" title="Play next" onclick={async () => addToSet(await versionsFor([t]), "next")}>Next</button>
      </div>
    {:else}
      {#if !loading && !playlist}
        <div class="empty">Search Spotify by song or artist. You can also paste an open.spotify.com link.</div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .sp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bar { display: flex; gap: 6px; padding: 8px 10px; border-bottom: 1px solid var(--border); align-items: center; }
  .pl-title { display: flex; gap: 6px; align-items: center; font-weight: 600; }
  .spacer { flex: 1; }
  .list { flex: 1; overflow-y: auto; }
  .item { display: flex; align-items: center; gap: 10px; padding: 5px 10px; border-bottom: 1px solid rgba(255,255,255,0.03); }
  .item:hover { background: var(--bg-hover); }
  img, .noart { width: 36px; height: 36px; border-radius: 4px; object-fit: cover; background: var(--bg-elev-3); flex: none; }
  .txt { flex: 1; min-width: 0; }
  .t, .a { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .a { font-size: var(--fs-sm); }
  .inlib { color: var(--green); display: flex; }
  .error { color: var(--red); padding: 8px 12px; }
  .small { font-size: var(--fs-sm); }
</style>

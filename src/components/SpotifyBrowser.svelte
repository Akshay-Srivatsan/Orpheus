<script lang="ts">
  import { api } from "../lib/api";
  import { app, spotifyVersions } from "../lib/state.svelte";
  import type { SpotifyTrack } from "../lib/types";
  import Icon from "./Icon.svelte";
  import SpotifyTrackList from "./SpotifyTrackList.svelte";

  let query = $state(app.search);
  let results = $state<SpotifyTrack[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

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

  $effect(() => {
    if (!playlist && query.trim() && !results.length) search();
  });

  async function newSetFromPlaylist() {
    if (!playlist) return;
    // Songs go into the set only; the library stays as it is.
    const vids = await spotifyVersions(results, false);
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
      <button disabled={!results.length} onclick={async () => { const v = await spotifyVersions(results, true); app.toast(`Added ${v.length} songs to your library`, "info"); }}>Add All to Library</button>
      <button class="primary" disabled={!results.length} onclick={newSetFromPlaylist}>New Setlist from Playlist</button>
    {:else}
      <!-- svelte-ignore a11y_autofocus -->
      <input type="search" placeholder="Search Spotify, or paste a track / album / playlist link" bind:value={query} autofocus
        onkeydown={(e) => e.key === "Enter" && search()} style="flex:1" />
      <button onclick={search}>Search</button>
    {/if}
  </div>
  {#if error}<div class="error">{error}</div>{/if}
  <div class="scroll">
    {#if loading}<div class="empty">Loading…</div>{/if}
    <SpotifyTrackList tracks={results} />
    {#if !loading && !results.length && !playlist}
      <div class="empty">Search Spotify by song or artist, or paste an open.spotify.com link.<br />
        <span class="faint">Previewing or adding a song to your set doesn't add it to your library.</span></div>
    {/if}
  </div>
</div>

<style>
  .sp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bar { display: flex; gap: 6px; padding: 8px 10px; border-bottom: 1px solid var(--border); align-items: center; }
  .pl-title { display: flex; gap: 6px; align-items: center; font-weight: 600; }
  .spacer { flex: 1; }
  .scroll { flex: 1; overflow-y: auto; }
  .error { color: var(--red); padding: 8px 12px; }
</style>

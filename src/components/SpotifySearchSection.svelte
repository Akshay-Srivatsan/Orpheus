<script lang="ts">
  // Spotify results shown under the library while searching.
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { SpotifyTrack } from "../lib/types";
  import Icon from "./Icon.svelte";
  import SpotifyTrackList from "./SpotifyTrackList.svelte";

  interface Props { query: string }
  let { query }: Props = $props();

  let results = $state<SpotifyTrack[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let open = $state(true);

  $effect(() => {
    const q = query.trim();
    if (q.length < 3) {
      results = [];
      return;
    }
    const t = setTimeout(async () => {
      loading = true;
      error = null;
      try {
        const r = await api.spotifySearch(q);
        if (query.trim() === q) results = r;
      } catch (e) {
        error = String(e);
      } finally {
        loading = false;
      }
    // Wait until typing pauses; the backend also caches results and backs off when rate-limited.
    }, 800);
    return () => clearTimeout(t);
  });
</script>

{#if query.trim().length >= 3}
  <section class="sps" class:open>
    <button class="head plain" onclick={() => (open = !open)}>
      <Icon name="spotify" size={14} />
      <span>Spotify</span>
      <span class="faint small">{loading ? "searching…" : error ? "" : `${results.length} results`}</span>
      <span class="spacer"></span>
      <span class="faint small">{open ? "Hide" : "Show"}</span>
    </button>
    {#if open}
      <div class="body">
        {#if error}<div class="err small">{error}</div>{/if}
        <SpotifyTrackList tracks={results.slice(0, 20)} compact />
        {#if results.length > 20}
          <button class="plain small more" onclick={() => (app.view = { kind: "spotify-search" })}>Show all in Spotify search…</button>
        {/if}
      </div>
    {/if}
  </section>
{/if}

<style>
  .sps { border-top: 1px solid var(--border-strong); display: flex; flex-direction: column; min-height: 0; }
  .sps.open { flex: 0 1 40%; }
  .head { justify-content: flex-start; gap: 6px; height: 28px; padding: 0 10px; border-radius: 0; background: var(--bg-elev); width: 100%; }
  .head :global(svg) { color: var(--spotify); }
  .spacer { flex: 1; }
  .small { font-size: var(--fs-sm); }
  .body { overflow-y: auto; min-height: 0; }
  .err { color: var(--red); padding: 6px 10px; }
  .more { margin: 4px 10px; }
</style>

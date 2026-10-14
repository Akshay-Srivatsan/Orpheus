<script lang="ts">
  import { api } from "../lib/api";
  import { app, type SourceView } from "../lib/state.svelte";
  import type { Id, SpotifyPlaylist } from "../lib/types";
  import Icon from "./Icon.svelte";

  const styleCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const r of app.rows) if (r.version.style) m.set(r.version.style, (m.get(r.version.style) ?? 0) + 1);
    return m;
  });
  const unstyled = $derived(app.tracks.filter((t) => t.versions.every((v) => !v.style)).length);
  const missing = $derived(app.tracks.filter((t) => t.missing).length);
  const extraStyles = $derived([...styleCounts.keys()].filter((k) => !app.styleMap.has(k)));

  let playlists = $state<SpotifyPlaylist[] | null>(null);
  let loadingPlaylists = $state(false);
  let renaming = $state<Id | null>(null);
  let dropTarget = $state<string | null>(null);

  $effect(() => {
    if (app.spotify?.connected && playlists === null && !loadingPlaylists) {
      loadingPlaylists = true;
      api.spotifyPlaylists().then((p) => (playlists = p)).catch(() => (playlists = [])).finally(() => (loadingPlaylists = false));
    }
  });

  function is(v: SourceView): boolean {
    const c = app.view;
    if (c.kind !== v.kind) return false;
    if (c.kind === "style" && v.kind === "style") return c.key === v.key;
    if (c.kind === "crate" && v.kind === "crate") return c.id === v.id;
    if (c.kind === "setlist" && v.kind === "setlist") return c.id === v.id;
    if (c.kind === "spotify-playlist" && v.kind === "spotify-playlist") return c.uri === v.uri;
    return true;
  }

  function go(v: SourceView) {
    app.view = v;
    app.selected = [];
    app.reloadBrowsed();
  }

  async function newCrate() {
    const id = await app.run(api.createCrate("New crate"));
    if (id) renaming = id;
  }

  function versionsFromDrag(e: DragEvent): Id[] {
    try {
      return JSON.parse(e.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
    } catch {
      return [];
    }
  }

  async function dropOnCrate(e: DragEvent, id: Id) {
    e.preventDefault();
    dropTarget = null;
    const ids = versionsFromDrag(e);
    if (ids.length) await app.run(api.addToCrate(id, ids));
  }

  async function assignStyle(e: DragEvent, key: string) {
    e.preventDefault();
    dropTarget = null;
    // Dropping songs on a dance style labels them: unstyled versions get the
    // style; styled ones get a new version for this style.
    for (const vid of versionsFromDrag(e)) {
      const r = app.versionMap.get(vid);
      if (!r) continue;
      if (!r.version.style) await app.run(api.updateVersion(vid, { style: key }));
      else if (r.version.style !== key)
        await app.run(api.createVersion(r.track.id, { name: app.styleName(key), style: key, comment: "" }));
    }
  }
</script>

<nav class="sidebar">
  <div class="group">
    <div class="section-title">Library</div>
    <button class="item" class:sel={is({ kind: "all" })} onclick={() => go({ kind: "all" })}>
      <Icon name="music" size={14} /> All Songs <span class="count">{app.rows.length}</span>
    </button>
    {#if unstyled}
      <button class="item" class:sel={is({ kind: "unstyled" })} onclick={() => go({ kind: "unstyled" })}>
        <Icon name="tag" size={14} /> No Dance Style <span class="count">{unstyled}</span>
      </button>
    {/if}
    <button class="item" class:sel={is({ kind: "history" })} onclick={() => go({ kind: "history" })}>
      <Icon name="history" size={14} /> History
    </button>
    {#if missing}
      <button class="item warn" class:sel={is({ kind: "missing" })} onclick={() => go({ kind: "missing" })}>
        <Icon name="warning" size={14} /> Missing Files <span class="count">{missing}</span>
      </button>
    {/if}
  </div>

  <div class="group">
    <div class="section-title">Dance Styles</div>
    {#each app.profile?.styles ?? [] as s (s.key)}
      <button class="item" class:sel={is({ kind: "style", key: s.key })} class:zero={!styleCounts.get(s.key)}
        class:drop={dropTarget === "s:" + s.key}
        onclick={() => go({ kind: "style", key: s.key })}
        ondragover={(e) => { e.preventDefault(); dropTarget = "s:" + s.key; }}
        ondragleave={() => (dropTarget = null)}
        ondrop={(e) => assignStyle(e, s.key)}
        title="Drop songs here to label them {s.name}">
        <span class="sdot" style:background={s.color}></span> {s.name}
        <span class="count">{styleCounts.get(s.key) ?? ""}</span>
      </button>
    {/each}
    {#each extraStyles as k}
      <button class="item" class:sel={is({ kind: "style", key: k })} onclick={() => go({ kind: "style", key: k })}
        title="Not defined in the current club profile">
        <span class="sdot" style:background="#8e8e93"></span> {app.styleName(k)} <span class="count">{styleCounts.get(k)}</span>
      </button>
    {/each}
  </div>

  <div class="group">
    <div class="section-title row">Crates <button class="plain icon add" title="New crate" onclick={newCrate}><Icon name="plus" size={13} /></button></div>
    {#each app.crates as c (c.id)}
      {#if renaming === c.id}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="rename" value={c.name} autofocus
          onkeydown={(e) => {
            if (e.key === "Enter") { api.renameCrate(c.id, e.currentTarget.value); renaming = null; }
            if (e.key === "Escape") renaming = null;
          }}
          onblur={(e) => { api.renameCrate(c.id, e.currentTarget.value); renaming = null; }} />
      {:else}
        <button class="item" class:sel={is({ kind: "crate", id: c.id })} class:drop={dropTarget === "c:" + c.id}
          onclick={() => go({ kind: "crate", id: c.id })} ondblclick={() => (renaming = c.id)}
          ondragover={(e) => { e.preventDefault(); dropTarget = "c:" + c.id; }}
          ondragleave={() => (dropTarget = null)}
          ondrop={(e) => dropOnCrate(e, c.id)}>
          <Icon name="crate" size={14} /> {c.name} <span class="count">{c.versionIds.length}</span>
        </button>
      {/if}
    {:else}
      <div class="hint">Drag songs onto a crate to collect them.</div>
    {/each}
  </div>

  <div class="group">
    <div class="section-title row">Setlists <button class="plain icon add" title="New setlist" onclick={() => (app.newSetlistOpen = true)}><Icon name="plus" size={13} /></button></div>
    {#each app.setlists as s (s.id)}
      <button class="item" class:sel={is({ kind: "setlist", id: s.id })} onclick={() => go({ kind: "setlist", id: s.id })}
        ondblclick={() => api.performOpen(s.id)} title="Double-click to open in the set panel">
        <Icon name="list" size={14} />
        <span class="name">{s.name}</span>
        {#if app.perform?.setlistId === s.id}<span class="cur" title="Open in the set panel">●</span>{/if}
        <span class="count">{s.entryCount}</span>
      </button>
    {/each}
  </div>

  <div class="group">
    <div class="section-title">Spotify</div>
    {#if app.spotify?.connected}
      <button class="item" class:sel={is({ kind: "spotify-search" })} onclick={() => go({ kind: "spotify-search" })}>
        <Icon name="search" size={14} /> Search Spotify
      </button>
      {#if loadingPlaylists}<div class="hint">Loading playlists…</div>{/if}
      {#each playlists ?? [] as pl (pl.uri)}
        <button class="item" class:sel={is({ kind: "spotify-playlist", uri: pl.uri, name: pl.name })}
          onclick={() => go({ kind: "spotify-playlist", uri: pl.uri, name: pl.name })}>
          <Icon name="spotify" size={14} /> <span class="name">{pl.name}</span>
        </button>
      {/each}
    {:else}
      <button class="item" onclick={() => (app.settingsOpen = "spotify")}>
        <Icon name="spotify" size={14} /> {app.spotify?.connecting ? "Connecting…" : "Connect Spotify…"}
      </button>
    {/if}
  </div>
</nav>

<style>
  .sidebar { overflow-y: auto; padding: 10px 6px; display: flex; flex-direction: column; gap: 14px; background: var(--bg-elev); border-right: 1px solid var(--border); }
  .group { display: flex; flex-direction: column; gap: 1px; }
  .section-title { padding: 0 8px 4px; }
  .section-title.row { display: flex; align-items: center; justify-content: space-between; }
  .add { width: 18px; height: 18px; color: var(--text-dim); }
  .item { justify-content: flex-start; height: 24px; padding: 0 8px; background: transparent; border: none; border-radius: 5px; color: var(--text); gap: 7px; font-size: var(--fs); width: 100%; }
  .item :global(svg) { color: var(--text-dim); flex: none; }
  .item:hover { background: var(--bg-hover); }
  .item.sel { background: var(--bg-selected); }
  .item.zero { color: var(--text-faint); }
  .item.warn { color: var(--yellow); }
  .item.drop { outline: 2px solid var(--blue); outline-offset: -2px; }
  .item .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
  .count { margin-left: auto; color: var(--text-faint); font-size: var(--fs-xs); font-variant-numeric: tabular-nums; }
  .cur { color: var(--green); font-size: 9px; }
  .sdot { width: 9px; height: 9px; border-radius: 50%; flex: none; }
  .hint { color: var(--text-faint); font-size: var(--fs-xs); padding: 2px 8px; }
  .rename { margin: 1px 4px; height: 24px; }
</style>

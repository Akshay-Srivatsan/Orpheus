<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { evaluate } from "../lib/smart";
  import { app, type SourceView, confirmAction } from "../lib/state.svelte";
  import type { Crate, Id, SpotifyPlaylist, SetlistSummary } from "../lib/types";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";

  const missing = $derived(app.libraryTracks.filter((t) => t.missing).length);

  /** A crate file another library can open: its songs link to the same audio there. */
  async function exportCrate(c: Crate) {
    const path = await saveDialog({ defaultPath: `${c.name}.orpheus-crate.toml`, filters: [{ name: "Orpheus crate", extensions: ["toml"] }] });
    if (path && (await app.run(api.exportCrate(c.id, path))) !== undefined) app.toast(`Exported “${c.name}”.`, "info");
  }

  async function importCrate() {
    const path = await openDialog({ multiple: false, directory: false, title: "Import a crate", filters: [{ name: "Orpheus crate", extensions: ["toml"] }] });
    if (typeof path !== "string") return;
    const r = await app.run(api.importCrate(path));
    if (!r) return;
    app.view = { kind: "crate", id: r.setlistId };
    const parts = [`${r.linked} linked to your songs`, r.spotify ? `${r.spotify} from Spotify` : "", r.missing ? `${r.missing} not here yet (they link up when you add the audio)` : ""];
    app.toast(`Imported: ${parts.filter(Boolean).join(", ")}.`, r.missing ? "error" : "info");
  }

  /** Opens a set file: its songs link to this library's audio (the same file, another encoding, or Spotify). */
  async function importSet() {
    const path = await openDialog({ multiple: false, directory: false, title: "Import a set", filters: [{ name: "Orpheus set", extensions: ["toml"] }] });
    if (typeof path !== "string") return;
    const r = await app.run(api.importSetlist(path));
    if (!r) return;
    await app.run(api.performOpen(r.setlistId));
    const parts = [`${r.linked} linked to your songs`, r.spotify ? `${r.spotify} from Spotify` : "", r.missing ? `${r.missing} not here yet (they link up when you add the audio, or a file that sounds the same)` : ""];
    app.toast(`Imported: ${parts.filter(Boolean).join(", ")}.`, r.missing ? "error" : "info");
  }

  let playlists = $state<SpotifyPlaylist[] | null>(null);
  let loadingPlaylists = $state(false);
  let renaming = $state<Id | null>(null);
  let dropTarget = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  $effect(() => {
    if (app.spotify?.connected && playlists === null && !loadingPlaylists) {
      loadingPlaylists = true;
      api.spotifyPlaylists().then((p) => (playlists = p)).catch(() => (playlists = [])).finally(() => (loadingPlaylists = false));
    }
  });


  function count(c: Crate): number {
    return c.smart ? evaluate(c.smart, app.rows, app.smartCtx).length : c.versionIds.length;
  }

  function is(v: SourceView): boolean {
    const c = app.view;
    if (c.kind !== v.kind) return false;
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

  function addMenu(e: MouseEvent) {
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "New Crate", action: newCrate },
        { label: "New Smart Crate…", action: () => (app.smartEditor = "new") },
        { separator: true },
        { label: "Import Crate…", action: importCrate },
      ],
    };
  }

  function crateMenu(e: MouseEvent, c: Crate) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Rename", action: () => (renaming = c.id) },
        ...(c.smart ? [{ label: "Edit Rules…", action: () => (app.smartEditor = c.id) }] : []),
        { separator: true },
        { label: "Export Crate…", action: () => exportCrate(c) },
        { label: "Delete Crate…", danger: true, action: () => confirmAction(`Delete the crate “${c.name}”? Its songs stay in your library.`).then((ok) => { if (ok) api.deleteCrate(c.id); }) },
      ],
    };
  }

  let renamingSet = $state<Id | null>(null);

  function setlistMenu(e: MouseEvent, s: SetlistSummary) {
    e.preventDefault();
    const isOpen = app.perform?.setlistId === s.id;
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: isOpen ? "Close in Set Panel" : "Open in Set Panel", action: () => api.performOpen(isOpen ? null : s.id) },
        { label: "Rename", action: () => (renamingSet = s.id) },
        { label: "Duplicate", action: () => api.duplicateSetlist(s.id, `${s.name} (copy)`) },
        { separator: true },
        {
          label: "Delete Setlist…",
          danger: true,
          disabled: isOpen && app.decks.some((d) => d.onAir && d.status === "playing" && !!d.entryId),
          action: () =>
            confirmAction(`Delete the setlist “${s.name}” (${s.entryCount} entries)? The songs stay in your library.`).then(async (ok) => {
              if (!ok) return;
              await app.run(api.deleteSetlist(s.id));
              if (app.view.kind === "setlist" && app.view.id === s.id) app.view = { kind: "all" };
            }),
        },
      ],
    };
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
</script>

<nav class="sidebar">
  <div class="group">
    <div class="section-title">Library</div>
    <button class="item" class:sel={is({ kind: "all" })} onclick={() => go({ kind: "all" })}>
      <Icon name="music" size={14} /> All Songs <span class="count">{app.libraryTracks.length}</span>
    </button>
    <button class="item" class:sel={is({ kind: "history" })} onclick={() => go({ kind: "history" })}>
      <Icon name="history" size={14} /> History
    </button>
    <button class="item" class:sel={is({ kind: "files" })} onclick={() => go({ kind: "files" })} title="The audio files in your library folders: add ones removed from the library, or remove some">
      <Icon name="disc" size={14} /> Files
    </button>
    {#if missing}
      <button class="item warn" class:sel={is({ kind: "missing" })} onclick={() => go({ kind: "missing" })}>
        <Icon name="warning" size={14} /> Missing Files <span class="count">{missing}</span>
      </button>
    {/if}
  </div>

  <div class="group">
    <div class="section-title row">Crates <button class="plain icon add" title="New crate or smart crate" onclick={addMenu}><Icon name="plus" size={13} /></button></div>
    {#each app.crates as c (c.id)}
      {#if renaming === c.id}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="rename" value={c.name} autofocus
          onkeydown={(e) => {
            if (e.key === "Enter") { api.renameCrate(c.id, e.currentTarget.value); renaming = null; }
            if (e.key === "Escape") renaming = null;
          }}
          onblur={(e) => { api.renameCrate(c.id, e.currentTarget.value); renaming = null; }} />
      {:else if c.smart}
        <button class="item" class:sel={is({ kind: "crate", id: c.id })}
          onclick={() => go({ kind: "crate", id: c.id })} ondblclick={() => (app.smartEditor = c.id)}
          oncontextmenu={(e) => crateMenu(e, c)} title="Smart crate — double-click to edit its rules">
          <Icon name="auto" size={14} /> <span class="name">{c.name}</span> <span class="count">{count(c)}</span>
        </button>
      {:else}
        <button class="item" class:sel={is({ kind: "crate", id: c.id })} class:drop={dropTarget === "c:" + c.id}
          onclick={() => go({ kind: "crate", id: c.id })} ondblclick={() => (renaming = c.id)}
          oncontextmenu={(e) => crateMenu(e, c)}
          ondragover={(e) => { e.preventDefault(); dropTarget = "c:" + c.id; }}
          ondragleave={() => (dropTarget = null)}
          ondrop={(e) => dropOnCrate(e, c.id)}>
          <Icon name="crate" size={14} /> <span class="name">{c.name}</span> <span class="count">{c.versionIds.length}</span>
        </button>
      {/if}
    {:else}
      <div class="hint">Crates are your own collections (any mix of dance styles). Drag songs onto one to add them; smart crates fill themselves from rules.</div>
    {/each}
  </div>

  <div class="group">
    <div class="section-title row">Setlists
      <span>
        <button class="plain icon add" title="Import a set file (its songs link to your audio)" onclick={importSet}><Icon name="download" size={13} /></button>
        <button class="plain icon add" title="New setlist" onclick={() => (app.newSetlistOpen = true)}><Icon name="plus" size={13} /></button>
      </span>
    </div>
    {#each app.setlists as s (s.id)}
      {#if renamingSet === s.id}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="rename" value={s.name} autofocus
          onkeydown={(e) => {
            if (e.key === "Enter") { api.updateSetlist(s.id, { name: e.currentTarget.value }); renamingSet = null; }
            if (e.key === "Escape") renamingSet = null;
          }}
          onblur={(e) => { api.updateSetlist(s.id, { name: e.currentTarget.value }); renamingSet = null; }} />
      {:else}
        <button class="item" class:sel={is({ kind: "setlist", id: s.id })} onclick={() => go({ kind: "setlist", id: s.id })}
          ondblclick={() => api.performOpen(s.id)} oncontextmenu={(e) => setlistMenu(e, s)}
          title="Double-click to open in the set panel; right-click for more">
          <Icon name="list" size={14} />
          <span class="name">{s.name}</span>
          {#if app.perform?.setlistId === s.id}<span class="cur" title="Open in the set panel">●</span>{/if}
          <span class="count">{s.entryCount}</span>
        </button>
      {/if}
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

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

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
  .item.warn { color: var(--yellow); }
  .item.drop { outline: 2px solid var(--blue); outline-offset: -2px; }
  .item .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
  .count { margin-left: auto; color: var(--text-faint); font-size: var(--fs-xs); font-variant-numeric: tabular-nums; }
  .cur { color: var(--green); font-size: 9px; }
  .hint { color: var(--text-faint); font-size: var(--fs-xs); padding: 2px 8px; }
  .rename { margin: 1px 4px; height: 24px; }
</style>

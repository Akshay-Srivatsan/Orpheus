<script lang="ts">
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { checkSetlist } from "../lib/rules";
  import { app } from "../lib/state.svelte";
  import Deck from "./Deck.svelte";
  import Inspector from "./Inspector.svelte";
  import LibraryTable from "./LibraryTable.svelte";
  import NewSetlistDialog from "./NewSetlistDialog.svelte";
  import SetlistChecks from "./SetlistChecks.svelte";
  import SetPanel from "./SetPanel.svelte";
  import SettingsDialog from "./SettingsDialog.svelte";
  import Sidebar from "./Sidebar.svelte";
  import SpotifyBrowser from "./SpotifyBrowser.svelte";
  import Toasts from "./Toasts.svelte";
  import TopBar from "./TopBar.svelte";
  import Transport from "./Transport.svelte";
  import Icon from "./Icon.svelte";

  let ready = $state(false);
  let dragging = $state(false);
  let searchEl: HTMLInputElement | undefined = $state();

  onMount(() => {
    app.init().then(() => (ready = true)).catch((e) => app.toast(`Failed to start: ${e}`));
    // Drop files or folders from Finder / Explorer to import them.
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "over" || e.payload.type === "enter") dragging = true;
      else if (e.payload.type === "leave") dragging = false;
      else if (e.payload.type === "drop") {
        dragging = false;
        if (e.payload.paths.length) {
          api.importPaths(e.payload.paths);
          app.toast(`Importing ${e.payload.paths.length} item(s)…`, "info");
        }
      }
    });
    return () => { un.then((f) => f()); };
  });

  const checks = $derived.by(() => {
    if (!app.current || !app.profile) return null;
    const tracks = app.trackMap;
    return checkSetlist({
      profile: app.profile,
      setlist: app.current,
      tracks,
      versions: app.versionMap,
      fadeSeconds: app.fadeSeconds,
      trimSilence: app.settings?.trimSilence ?? true,
    });
  });
  const problems = $derived(checks?.results.filter((r) => !r.ok && r.severity !== "info").length ?? 0);
  const spotifyView = $derived(app.view.kind === "spotify-search" || app.view.kind === "spotify-playlist");

  const viewTitle = $derived.by(() => {
    const v = app.view;
    switch (v.kind) {
      case "all": return "All Songs";
      case "style": return app.styleName(v.key);
      case "unstyled": return "No Dance Style";
      case "crate": return app.crates.find((c) => c.id === v.id)?.name ?? "Crate";
      case "setlist": return app.setlists.find((s) => s.id === v.id)?.name ?? "Setlist";
      case "history": return "History";
      case "missing": return "Missing Files";
      case "spotify-search": return "Spotify";
      case "spotify-playlist": return v.name;
    }
  });
  const viewStyle = $derived(app.view.kind === "style" ? app.style(app.view.key) : undefined);

  function typing(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    return t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable;
  }

  function keydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === "Enter") { e.preventDefault(); app.run(api.playNext()); return; }
    if (mod && e.key === ",") { e.preventDefault(); app.settingsOpen = "general"; return; }
    if (mod && e.key.toLowerCase() === "f") { e.preventDefault(); searchEl?.focus(); searchEl?.select(); return; }
    if (mod && e.key.toLowerCase() === "n") { e.preventDefault(); app.newSetlistOpen = true; return; }
    if (mod && e.key.toLowerCase() === "i") { e.preventDefault(); app.rightTab = "info"; return; }
    if (mod && e.key === ".") { e.preventDefault(); api.deckAction(0, "fadeOut"); return; }
    if (typing(e)) {
      if (e.key === "Escape") (e.target as HTMLElement).blur();
      return;
    }
    if (e.key === " ") { e.preventDefault(); api.deckAction(1, "toggle"); }
  }
</script>

<svelte:window onkeydown={keydown} />

{#if ready && app.settings}
  <div class="app">
    <TopBar />
    <div class="decks">
      <Deck deck={0} />
      <Transport />
      <Deck deck={1} />
    </div>
    <div class="bottom">
      <Sidebar />
      <main class="browser">
        {#if spotifyView}
          <SpotifyBrowser />
        {:else}
          <div class="bhead">
            <div class="vt">
              {#if viewStyle}<span class="sdot" style:background={viewStyle.color}></span>{/if}
              {viewTitle}
              {#if viewStyle?.sweet}<span class="faint small num">{viewStyle.sweet[0]}–{viewStyle.sweet[1]} BPM{#if viewStyle.extended} (ok {viewStyle.extended[0]}–{viewStyle.extended[1]}){/if}</span>{/if}
            </div>
            <div class="search">
              <Icon name="search" size={13} />
              <input type="search" placeholder="Search" bind:value={app.search} bind:this={searchEl} />
            </div>
          </div>
          <LibraryTable />
        {/if}
      </main>
      <aside class="right">
        <div class="tabs segmented">
          <button class:on={app.rightTab === "set"} onclick={() => (app.rightTab = "set")}>Set</button>
          <button class:on={app.rightTab === "checks"} onclick={() => (app.rightTab = "checks")}>
            Checks {#if problems}<span class="pill">{problems}</span>{/if}
          </button>
          <button class:on={app.rightTab === "info"} onclick={() => (app.rightTab = "info")}>Info</button>
        </div>
        <div class="rbody">
          {#if app.rightTab === "set"}
            <SetPanel {checks} />
          {:else if app.rightTab === "checks"}
            {#if checks}
              <div class="checks"><SetlistChecks result={checks} onSelectEntries={(ids) => { app.selectedEntries = ids; app.rightTab = "set"; }} /></div>
            {:else}
              <div class="empty">Open a setlist to check it against {app.profile?.name ?? "the club"}'s guidelines.</div>
            {/if}
          {:else}
            <Inspector />
          {/if}
        </div>
      </aside>
    </div>
  </div>
  {#if app.settingsOpen}<SettingsDialog />{/if}
  {#if app.newSetlistOpen}<NewSetlistDialog />{/if}
  {#if dragging}<div class="dropzone">Drop music files or folders to add them to your library</div>{/if}
{:else}
  <div class="loading">Loading Orpheus…</div>
{/if}
<Toasts />

<style>
  .app { display: grid; grid-template-rows: 42px 262px 1fr; height: 100vh; }
  .decks { display: grid; grid-template-columns: minmax(0, 1.3fr) 250px minmax(0, 1fr); gap: 8px; padding: 8px; min-height: 0; }
  .decks > :global(*) { min-height: 0; overflow: hidden; }
  .bottom { display: grid; grid-template-columns: 220px minmax(0, 1fr) 380px; min-height: 0; border-top: 1px solid var(--border); }
  .browser { display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .bhead { display: flex; align-items: center; gap: 10px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
  .vt { flex: 1; display: flex; align-items: center; gap: 8px; font: 600 var(--fs-md) var(--font-display); min-width: 0; }
  .sdot { width: 10px; height: 10px; border-radius: 50%; }
  .small { font-size: var(--fs-sm); font-weight: 400; }
  .search { display: flex; align-items: center; gap: 4px; color: var(--text-faint); }
  .search input { width: 220px; }
  .right { display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--border); background: var(--bg-elev); }
  .tabs { margin: 8px; display: flex; }
  .tabs button { flex: 1; }
  .pill { background: var(--red); color: white; border-radius: 8px; padding: 0 5px; font-size: 10px; margin-left: 4px; }
  .rbody { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .checks { overflow-y: auto; padding: 0 8px 8px; }
  .loading { display: grid; place-items: center; height: 100vh; color: var(--text-dim); }
  .dropzone { position: fixed; inset: 12px; z-index: 90; border: 3px dashed var(--blue); border-radius: 16px; background: rgba(10,132,255,0.08);
    display: grid; place-items: center; font: 600 var(--fs-lg) var(--font-display); pointer-events: none; }
</style>

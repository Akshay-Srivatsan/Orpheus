<script lang="ts">
  // Library → Files: the audio files in your library folders. A file removed
  // from the library stays on disk, and scanning skips it until it's added
  // again here (like finding a Spotify song again). Files elsewhere can be
  // added too.
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, on } from "../lib/api";
  import { app, confirmAction } from "../lib/state.svelte";
  import type { LibraryFile } from "../lib/types";

  let files = $state<LibraryFile[]>([]);
  let show = $state<"all" | "in" | "out">("all");
  /** Not in the library: removed, or (e.g. its recording now plays from Spotify) not added. */
  const out = (f: LibraryFile) => f.removed || !f.recording;

  async function load() {
    files = (await app.run(api.libraryFiles())) ?? [];
  }
  $effect(() => {
    void load();
    const offs = [on("recordings-changed", load), on("library-changed", load), on("scan-progress", (p) => { if (!p.running) void load(); })];
    return () => offs.forEach((p) => p.then((f) => f()));
  });

  /** "folder/sub/file.flac" within its library folder. */
  const rel = (f: LibraryFile) => (f.root && f.path.startsWith(f.root) ? f.path.slice(f.root.length).replace(/^[/\\]+/, "") : f.path);
  const shown = $derived.by(() => {
    const q = app.search.trim().toLowerCase();
    return files
      .filter((f) => show === "all" || (show === "out") === out(f))
      .filter((f) => !q || f.path.toLowerCase().includes(q) || f.songs.some((id) => app.trackMap.get(id)?.title.toLowerCase().includes(q)));
  });
  const outCount = $derived(files.filter(out).length);

  async function add(paths: string[]) {
    await app.run(api.addFiles(paths));
    app.toast(paths.length === 1 ? "Adding it to the library…" : `Adding ${paths.length} files to the library…`, "info");
  }

  async function remove(f: LibraryFile) {
    const titles = f.songs.map((id) => app.trackMap.get(id)?.title ?? id);
    const msg = titles.length
      ? `Remove “${rel(f)}” from the library? It's played by ${titles.map((t) => `“${t}”`).join(", ")}; those versions go too. The file stays on disk, and you can add it again here.`
      : `Remove “${rel(f)}” from the library? The file stays on disk.`;
    if (f.recording && (await confirmAction(msg, "Remove"))) await app.run(api.removeRecordings([f.recording]));
  }

  async function addOther() {
    const picked = await openDialog({ multiple: true, directory: false, title: "Add audio files to the library" });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length) await add(paths);
  }
</script>

<div class="files">
  <div class="bar">
    <div class="segmented small">
      <button class:on={show === "all"} onclick={() => (show = "all")}>All</button>
      <button class:on={show === "in"} onclick={() => (show = "in")}>In the library</button>
      <button class:on={show === "out"} onclick={() => (show = "out")}>Not in the library ({outCount})</button>
    </div>
    {#if show === "out" && outCount}
      <button class="small" onclick={() => add(shown.map((f) => f.path))}>Add All {shown.length} Back</button>
    {/if}
    <span class="grow"></span>
    <button class="small" onclick={addOther}>Add Files…</button>
  </div>
  <div class="table">
    <div class="row head"><span>File</span><span>Folder</span><span>In the library as</span><span></span></div>
    {#each shown as f (f.path)}
      <div class="row" class:removed={out(f)}>
        <span class="t" title={f.path}>{rel(f).split(/[/\\]/).pop()}</span>
        <span class="t faint" title={f.root ?? ""}>{rel(f).split(/[/\\]/).slice(0, -1).join("/") || (f.root ? "" : "(added from elsewhere)")}</span>
        <span class="t">
          {#if f.removed}<span class="faint">removed</span>
          {:else if !f.recording}<span class="faint">not added</span>
          {:else}
            {#each f.songs as id, i (id)}{i ? ", " : ""}<button class="plain link" onclick={() => { app.inspectTrack = id; app.rightTab = "info"; }}>{app.trackMap.get(id)?.title ?? id}</button>{/each}
          {/if}
        </span>
        <span class="actions">
          {#if out(f)}
            <button class="small" onclick={() => add([f.path])}>Add to Library</button>
          {:else if f.recording}
            <button class="plain small danger" onclick={() => remove(f)}>Remove</button>
          {/if}
        </span>
      </div>
    {:else}
      <div class="empty faint">{show === "out" ? "Every file in your library folders is in the library." : "No audio files yet: add a music folder in Settings → Library, or Add Files…"}</div>
    {/each}
  </div>
</div>

<style>
  .files { height: 100%; display: flex; flex-direction: column; min-height: 0; }
  .bar { display: flex; gap: 8px; align-items: center; padding: 6px 10px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .grow { flex: 1; }
  .table { overflow: auto; flex: 1; font-size: var(--fs-sm); }
  .row { display: grid; grid-template-columns: minmax(0, 2fr) minmax(0, 1.4fr) minmax(0, 1.8fr) 120px; gap: 8px; align-items: center; padding: 3px 10px; border-bottom: 1px solid rgba(255, 255, 255, 0.04); }
  .row.removed .t:first-child { color: var(--text-faint); }
  .head { position: sticky; top: 0; background: var(--bg); color: var(--text-faint); font-weight: 600; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .actions { display: flex; gap: 4px; justify-content: flex-end; }
  .link { padding: 0; height: auto; color: var(--blue); display: inline; }
  .small { font-size: var(--fs-sm); }
  .danger { color: var(--red); }
  .empty { padding: 16px; }
</style>

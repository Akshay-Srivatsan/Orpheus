<script lang="ts">
  // Settings → Clubs: clubs' guidelines (TOML files, see examples/clubs/),
  // their versions, importing from a file or a web address, and an editor
  // (a form, or the file itself). Sets keep the version they were planned
  // with; saving edits to a version that sets use makes a new version.
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { allSongs, emptyClub, fmtSecs, hasCriteria, type Club, type ClubGroup, type ClubRule, type ClubStyle, type ClubTag, type Range } from "../lib/clubs";
  import { app, confirmAction, versionDate } from "../lib/state.svelte";
  import { NEUTRAL } from "../lib/styles";
  import { primaryVersion, type ClubInfo, type ClubVersionInfo } from "../lib/types";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import RangeInput from "./RangeInput.svelte";

  let selId = $state<string>(app.clubRef?.id ?? app.clubs[0]?.id ?? "");
  const sel = $derived<ClubInfo | undefined>(app.clubs.find((c) => c.id === selId) ?? app.clubs[0]);
  let versionId = $state<string>("");
  const version = $derived<ClubVersionInfo | undefined>(sel?.versions.find((v) => v.id === versionId) ?? sel?.versions[0]);
  let usage = $state<Record<string, number>>({});
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let urlOpen = $state(false);
  let url = $state("");
  let busy = $state(false);
  /** For each style with suggested songs: each song's matching library tracks (as saved). */
  let entries = $state<Record<string, string[][]>>({});

  // The version being edited: its file text and (if it parses) its definition.
  let text = $state("");
  let draft = $state<Club | null>(null);
  let error = $state<string | null>(null);
  let dirty = $state(false);
  let view = $state<"form" | "file">("form");

  $effect(() => {
    const c = sel;
    const v = version;
    dirty = false;
    if (!c || !v) {
      draft = null;
      text = "";
      return;
    }
    api.clubUsage(c.id).then((u) => (usage = u));
    api.clubSongEntries(c.id, v.id).then((e) => (entries = e)).catch(() => (entries = {}));
    api.club(c.id, v.id).then((r) => {
      text = r.text;
      draft = r.club;
      error = r.error;
    });
  });

  const versionLabel = (v: ClubVersionInfo, i: number) => v.name || (i === 0 ? `Latest (${versionDate(sel!, v)})` : versionDate(sel!, v));
  const fromLabel = (f: string) => (f === "edited" ? "edited here" : f === "example" ? "example" : f === "new" ? "new" : f);

  function picked(c: { clubId: string; versionId: string; unchanged: boolean }, what: string) {
    selId = c.clubId;
    versionId = c.versionId;
    app.toast(c.unchanged ? `${what}: no changes since the latest version.` : `${what}: added.`, "info");
  }

  async function importFile() {
    const path = await openDialog({ multiple: false, filters: [{ name: "Club guidelines", extensions: ["toml"] }] });
    if (typeof path !== "string") return;
    const r = await app.run(api.clubImportFile(path));
    if (r) picked(r, "Club imported");
  }

  async function importUrl() {
    if (!url.trim()) return;
    busy = true;
    const r = await app.run(api.clubImportUrl(url.trim()));
    busy = false;
    if (r) {
      picked(r, "Club imported");
      urlOpen = false;
      url = "";
    }
  }

  async function addMenu(ev: MouseEvent) {
    const examples = (await app.run(api.clubExamples())) ?? [];
    menu = {
      x: ev.clientX,
      y: ev.clientY,
      items: [
        { label: "New Club…", action: newClub },
        { label: "Import File…", action: importFile },
        { label: "Import from a Web Address…", action: () => (urlOpen = true) },
        { separator: true },
        ...examples.map(([id, name]) => ({
          label: `Example: ${name}`,
          action: async () => {
            const r = await app.run(api.clubImportExample(id));
            if (r) picked(r, name);
          },
        })),
      ],
    };
  }

  let naming = $state(false);
  let newName = $state("");
  async function newClub() {
    naming = true;
  }
  async function createClub() {
    if (!newName.trim()) return;
    const r = await app.run(api.clubNew(newName.trim()));
    naming = false;
    newName = "";
    if (r) {
      selId = r.clubId;
      versionId = r.versionId;
    }
  }

  async function checkUpdates() {
    if (!sel) return;
    busy = true;
    const r = await app.run(api.clubUpdate(sel.id));
    busy = false;
    if (r) picked(r, "Update");
  }

  async function exportVersion() {
    if (!sel || !version) return;
    const path = await saveDialog({ defaultPath: `${sel.id}.toml`, filters: [{ name: "Club guidelines", extensions: ["toml"] }] });
    if (path) await app.run(api.clubExport(sel.id, version.id, path));
  }

  async function renameVersion(v: ClubVersionInfo) {
    const name = window.prompt("Name this version (empty: by its date)", v.name);
    if (name != null && sel) await app.run(api.clubRenameVersion(sel.id, v.id, name));
  }

  /** Older versions no set uses. */
  const unusedOld = $derived((sel?.versions ?? []).slice(1).filter((v) => !usage[v.id]));

  async function deleteUnused() {
    if (!sel || !unusedOld.length) return;
    if (!(await confirmAction(`Delete ${unusedOld.length} older version(s) of ${sel.name} that no set uses?`))) return;
    for (const v of unusedOld) await app.run(api.clubDeleteVersion(sel.id, v.id));
    versionId = "";
  }

  async function deleteVersion(v: ClubVersionInfo) {
    if (!sel) return;
    const n = usage[v.id] ?? 0;
    const last = sel.versions.length === 1;
    const msg = last
      ? `Delete ${sel.name}?${n ? ` ${n} set(s) use it and will have no club.` : ""}`
      : `Delete this version of ${sel.name}?${n ? ` ${n} set(s) use it and will have no club.` : ""}`;
    if (!(await confirmAction(msg))) return;
    await app.run(api.clubDeleteVersion(sel.id, v.id));
    versionId = "";
  }

  // ---------------------------------------------------------------- editing

  /** Applies a change to the form and keeps the file text in step. */
  async function edit(change: (c: Club) => void) {
    if (!draft) return;
    const next = structuredClone($state.snapshot(draft)) as Club;
    change(next);
    draft = next;
    dirty = true;
    const t = await app.run(api.clubFormat(next));
    if (t != null) {
      text = t;
      error = null;
    }
  }

  let parseTimer: ReturnType<typeof setTimeout> | undefined;
  function editText(t: string) {
    text = t;
    dirty = true;
    clearTimeout(parseTimer);
    parseTimer = setTimeout(async () => {
      try {
        draft = await api.clubParse(t);
        error = null;
      } catch (e) {
        error = String(e);
      }
    }, 400);
  }

  const savesNewVersion = $derived(!!version && (!!usage[version.id] || version.id !== sel?.versions[0]?.id));

  async function save() {
    if (!sel || !version || error) return;
    const r = await app.run(api.clubSave(sel.id, version.id, text));
    if (r) {
      dirty = false;
      versionId = r.versionId;
      if (r.versionId !== version.id) app.toast("Saved as a new version: sets planned with the old one keep it.", "info");
    }
  }

  function slugify(name: string, taken: Record<string, unknown>): string {
    const base = name.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "style";
    let id = base;
    for (let i = 2; id in taken; i++) id = `${base}-${i}`;
    return id;
  }

  const blankStyle = (name: string): ClubStyle => ({
    name, meter: [], tempo: null, preferred: null, length: null, count: null, share: null, when: [], first: false, last: false,
  });

  let openStyle = $state<string | null>(null);

  // ---------------------------------------------------------------- suggested songs

  /** The songs playing a recording (or the recording's own title, if none does). */
  const trackTitle = (recId: string) => {
    const songs = app.tracks.filter((t) => t.versions.some((v) => v.recording === recId));
    const rec = app.tracks.flatMap((t) => t.recordings).find((r) => r.id === recId);
    const name = (x: { title: string; artist: string }) => `${x.title}${x.artist ? ` — ${x.artist}` : ""}`;
    return songs.length ? songs.map(name).join(", ") : rec ? `${name(rec)} (hidden)` : recId;
  };

  const primaryOf = (trackId: string) => { const t = app.trackMap.get(trackId); return t ? primaryVersion(t)?.id : undefined; };

  /** Adds the song version selected in the library to a style's suggested songs. */
  async function addSelectedSong(styleId: string) {
    const vid = app.selected[app.selected.length - 1] ?? (app.inspectTrack ? primaryOf(app.inspectTrack) : undefined);
    if (!vid) {
      app.toast("Select a song in the library first.", "info");
      return;
    }
    const song = await app.run(api.clubSongForVersion(vid));
    if (song) edit((d) => (d.style[styleId].song = [...(d.style[styleId].song ?? []), song]));
  }

  async function addFromSpotify(uri: string) {
    const found = await app.run(api.spotifyResolve(uri));
    if (found?.length) {
      await app.run(api.spotifyImport(found));
      app.toast("Added to your library from Spotify.", "info");
    }
  }

  async function download(url: string, hash: string) {
    busy = true;
    const path = await app.run(api.clubSongDownload(url, hash));
    busy = false;
    if (path) app.toast(`Downloaded and added to your library (${path}).`, "info");
  }

  /** "3" or "2, 4" → [3] / [2, 4]. */
  const meters = (t: string) => t.split(/[ ,/]+/).map(Number).filter((n) => n > 0 && Number.isInteger(n));
  /** "0–40, 90–100" → ranges of percent. */
  function whenParse(t: string): Range[] {
    return t
      .split(/[,;]/)
      .map((p) => p.trim().replace(/%/g, ""))
      .filter(Boolean)
      .map((p) => {
        const [a, b] = p.split(/\s*[–-]\s*/);
        const min = a === "" ? null : Number(a);
        const max = b === undefined ? min : b === "" ? null : Number(b);
        return { min, max };
      });
  }
  const whenText = (w: Range[]) => w.map((r) => `${r.min ?? ""}–${r.max ?? ""}`).join(", ");
  const n = (v: string): number | null => (v.trim() === "" ? null : Number(v));
</script>

<h2>Clubs</h2>
<p class="dim">
  A club's guidelines: its dance styles (by meter, tempo and length) and what its sets should contain. Clubs can publish
  them as a file on their website (TOML; see the examples); import it from the web address, and check for updates later.
  Each set keeps the guidelines it was planned with until you move it to a newer version.
</p>

<div class="layout">
  <aside class="list">
    {#each app.clubs as c (c.id)}
      <button class="plain club" class:sel={sel?.id === c.id} onclick={() => { selId = c.id; versionId = ""; }}>
        <span>{c.name}</span>
        {#if app.settings?.defaultClub === c.id}<span class="faint small">default</span>{/if}
      </button>
    {:else}
      <span class="faint small">No clubs yet.</span>
    {/each}
    <button class="small add" onclick={addMenu}><Icon name="plus" size={12} /> Add Club</button>
    {#if naming}
      <form class="row" onsubmit={(e) => { e.preventDefault(); createClub(); }}>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" placeholder="Club name" bind:value={newName} autofocus />
        <button type="submit" class="small" disabled={!newName.trim()}>Create</button>
      </form>
    {/if}
    {#if urlOpen}
      <form class="col" onsubmit={(e) => { e.preventDefault(); importUrl(); }}>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="url" placeholder="https://club.example/orpheus.toml" bind:value={url} autofocus />
        <span class="row"><button type="submit" class="small primary" disabled={!url.trim() || busy}>{busy ? "Fetching…" : "Import"}</button>
          <button type="button" class="small" onclick={() => (urlOpen = false)}>Cancel</button></span>
      </form>
    {/if}
  </aside>

  {#if sel && version}
    <section class="detail">
      <div class="head">
        <h3>{sel.name}</h3>
        <span class="spacer"></span>
        {#if app.settings?.defaultClub !== sel.id}
          <button class="small" onclick={() => app.saveSettings({ defaultClub: sel.id })} title="New sets use this club's latest guidelines">Make Default</button>
        {/if}
        <button class="small" disabled={busy} onclick={checkUpdates} title={sel.url ? `Fetch ${sel.url} again` : "Fetch the file again from where it was imported"}>
          <Icon name="reload" size={12} /> {busy ? "Checking…" : "Check for Updates"}
        </button>
        <button class="small" onclick={exportVersion} title="Save this version as a file (to share or publish)">Export…</button>
        {#if unusedOld.length}
          <button class="small" onclick={deleteUnused} title="Delete the {unusedOld.length} older version(s) no set uses">Delete Unused Old Versions</button>
        {/if}
      </div>
      {#if draft?.website}<a class="small" href={draft.website} target="_blank" rel="noreferrer">{draft.website}</a>{/if}

      <table class="versions small">
        <thead><tr><th>Version</th><th>From</th><th>Sets</th><th></th></tr></thead>
        <tbody>
          {#each sel.versions as v, i (v.id)}
            <tr class:on={v.id === version.id} onclick={() => (versionId = v.id)}>
              <td>{versionLabel(v, i)}{#if v.error}<span class="err" title={v.error}> (can't be read)</span>{/if}</td>
              <td class="from" title={v.from}>{fromLabel(v.from)}</td>
              <td class="num">{usage[v.id] ?? 0}</td>
              <td class="nowrap">
                <button class="plain icon" title="Name this version" onclick={(e) => { e.stopPropagation(); renameVersion(v); }}><Icon name="edit" size={12} /></button>
                <button class="plain icon danger" title="Delete this version" onclick={(e) => { e.stopPropagation(); deleteVersion(v); }}><Icon name="x" size={12} /></button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>

      <div class="editbar">
        <div class="segmented">
          <button class:on={view === "form"} onclick={() => (view = "form")}>Form</button>
          <button class:on={view === "file"} onclick={() => (view = "file")}>File (TOML)</button>
        </div>
        <span class="spacer"></span>
        {#if dirty}<span class="faint small">{savesNewVersion ? "Saving makes a new version (sets keep theirs)" : "Unsaved changes"}</span>{/if}
        <button class="small primary" disabled={!dirty || !!error} onclick={save}>Save</button>
      </div>
      {#if error}<div class="err small">{error}</div>{/if}

      {#if view === "file" || !draft}
        <textarea class="file" spellcheck="false" value={text} oninput={(e) => editText(e.currentTarget.value)}></textarea>
      {:else}
        {@const c = draft}
        <div class="form">
          <span class="l">Name</span>
          <input type="text" value={c.name} onchange={(e) => edit((d) => (d.name = e.currentTarget.value))} />
          <span class="l">Description</span>
          <textarea rows="2" value={c.description} onchange={(e) => edit((d) => (d.description = e.currentTarget.value))}></textarea>
          <span class="l">Guidelines page</span>
          <input type="url" placeholder="https://… (for people)" value={c.website ?? ""} onchange={(e) => edit((d) => (d.website = e.currentTarget.value || null))} />
          <span class="l" title="Where this file is published; Check for Updates fetches it">File's web address</span>
          <input type="url" placeholder="https://…/club.toml" value={c.url ?? ""} onchange={(e) => edit((d) => (d.url = e.currentTarget.value || null))} />
        </div>

        <h4>The set</h4>
        <div class="form">
          <span class="l">Minutes</span><RangeInput value={c.set.minutes} onchange={(r) => edit((d) => (d.set.minutes = r))} />
          <span class="l">Songs</span><RangeInput value={c.set.songs} onchange={(r) => edit((d) => (d.set.songs = r))} />
          <span class="l">Pause (seconds)</span>
          <span class="row">
            <input type="number" class="w5" placeholder="target" value={c.set.pause.target ?? ""} onchange={(e) => edit((d) => (d.set.pause.target = n(e.currentTarget.value)))} />
            <span class="faint">between</span>
            <RangeInput value={{ min: c.set.pause.min, max: c.set.pause.max }} onchange={(r) => edit((d) => { d.set.pause.min = r?.min ?? null; d.set.pause.max = r?.max ?? null; })} />
          </span>
          <span class="l">Fade-out (seconds)</span>
          <input type="number" class="w5" value={c.set.fade_out ?? ""} onchange={(e) => edit((d) => (d.set.fade_out = n(e.currentTarget.value)))} />
          <span class="l">Tempo change allowed</span>
          <span class="row">
            faster <input type="number" class="w5" placeholder="%" value={c.set.tempo_change?.faster ?? ""}
              onchange={(e) => edit((d) => { const v = n(e.currentTarget.value); d.set.tempo_change = v == null && !d.set.tempo_change?.slower ? null : { faster: v ?? 0, slower: d.set.tempo_change?.slower ?? 0 }; })} /> %,
            slower <input type="number" class="w5" placeholder="%" value={c.set.tempo_change?.slower ?? ""}
              onchange={(e) => edit((d) => { const v = n(e.currentTarget.value); d.set.tempo_change = v == null && !d.set.tempo_change?.faster ? null : { faster: d.set.tempo_change?.faster ?? 0, slower: v ?? 0 }; })} /> %
            <span class="faint small">(empty: your tempo slider range)</span>
          </span>
          <span class="l">Same style in a row</span>
          <span class="row">at most <input type="number" class="w5" min="1" value={c.set.max_in_a_row ?? ""} onchange={(e) => edit((d) => (d.set.max_in_a_row = n(e.currentTarget.value)))} /></span>
          <span class="l">Auto mode</span>
          <select value={c.set.auto_advance == null ? "" : String(c.set.auto_advance)} onchange={(e) => edit((d) => (d.set.auto_advance = e.currentTarget.value === "" ? null : e.currentTarget.value === "true"))}>
            <option value="">Not specified</option><option value="false">Manual (Play Next)</option><option value="true">Automatic</option>
          </select>
        </div>

        <h4>Styles</h4>
        <table class="styles small">
          <thead><tr><th></th><th>Name</th><th>Short</th><th title="Beats per bar, e.g. 3 or 2, 4">Meter</th><th>Tempo (BPM)</th><th>Preferred</th><th>Per set</th><th></th></tr></thead>
          <tbody>
            {#each Object.entries(c.style) as [id, st] (id)}
              <tr>
                <td><input type="color" value={st.color || NEUTRAL} onchange={(e) => edit((d) => (d.style[id].color = e.currentTarget.value))} /></td>
                <td><input type="text" value={st.name} title="id: {id}" onchange={(e) => edit((d) => (d.style[id].name = e.currentTarget.value))} /></td>
                <td><input type="text" class="w5" value={st.short ?? ""} onchange={(e) => edit((d) => (d.style[id].short = e.currentTarget.value || null))} /></td>
                <td><input type="text" class="w4" placeholder="any" value={st.meter.join(", ")} onchange={(e) => edit((d) => (d.style[id].meter = meters(e.currentTarget.value)))} /></td>
                <td><RangeInput value={st.tempo} onchange={(r) => edit((d) => (d.style[id].tempo = r))} /></td>
                <td><RangeInput value={st.preferred} onchange={(r) => edit((d) => (d.style[id].preferred = r))} /></td>
                <td><RangeInput value={st.count} onchange={(r) => edit((d) => (d.style[id].count = r))} /></td>
                <td class="nowrap">
                  <button class="plain icon" class:on={openStyle === id} title="More: length, share, where in the set, notes, suggested songs" onclick={() => (openStyle = openStyle === id ? null : id)}>{#if allSongs(st).length}<span class="nsongs">♪{allSongs(st).length}</span>{:else}<Icon name="dots" size={12} />{/if}</button>
                  <button class="plain icon danger" title="Remove" onclick={() => edit((d) => delete d.style[id])}><Icon name="x" size={12} /></button>
                </td>
              </tr>
              {#if openStyle === id}
                <tr class="more">
                  <td></td>
                  <td colspan="7">
                    <span class="row">
                      Length <RangeInput kind="time" value={st.length} onchange={(r) => edit((d) => (d.style[id].length = r))} />
                      Share <RangeInput kind="pct" value={st.share} onchange={(r) => edit((d) => (d.style[id].share = r))} />
                      In a row, at most <input type="number" class="w4" min="1" value={st.max_in_a_row ?? ""} onchange={(e) => edit((d) => (d.style[id].max_in_a_row = n(e.currentTarget.value)))} />
                    </span>
                    <span class="row">
                      Where in the set <input type="text" placeholder="e.g. 0–40, 90–100 (%)" value={whenText(st.when)} onchange={(e) => edit((d) => (d.style[id].when = whenParse(e.currentTarget.value)))} />
                      <label><input type="checkbox" checked={st.first} onchange={(e) => edit((d) => (d.style[id].first = e.currentTarget.checked))} /> first song</label>
                      <label><input type="checkbox" checked={st.last} onchange={(e) => edit((d) => (d.style[id].last = e.currentTarget.checked))} /> last song</label>
                    </span>
                    <input type="text" class="note" placeholder="Note" value={st.note ?? ""} onchange={(e) => edit((d) => (d.style[id].note = e.currentTarget.value || null))} />
                    <div class="songs">
                      <span class="faint">Suggested songs{#if !hasCriteria(st)}{" (this style has no meter, tempo or length: only these songs fit it)"}{/if}</span>
                      {#each allSongs(st) as song, i (i)}
                        {@const own = i < (st.song?.length ?? 0)}
                        {@const have = entries[id]?.[i] ?? []}
                        <div class="song">
                          <span class="stitle">{song.title || "Untitled"}{#if song.artist}<span class="faint">{" — "}{song.artist}</span>{/if}{#if song.length}<span class="faint">{" · "}{fmtSecs(song.length)}</span>{/if}{#if song.note}<span class="faint">{" · "}{song.note}</span>{/if}</span>
                          <span class="ids faint">{[song.fingerprint && "fingerprint", song.spotify && "Spotify", song.hash && "hash"].filter(Boolean).join(" · ")}</span>
                          {#if have.length}
                            <span class="ok" title={have.map(trackTitle).join("\n")}><Icon name="check" size={11} /> in your library</span>
                          {:else}
                            <span class="faint">not in your library</span>
                            {#if song.spotify}<button class="plain small" onclick={() => addFromSpotify(song.spotify!)}>Add from Spotify</button>{/if}
                            {#each song.download ?? [] as url, k (k)}
                              <button class="plain small" disabled={busy || !song.hash} title={url} onclick={() => download(url, song.hash!)}>Download{(song.download?.length ?? 0) > 1 ? ` (${k + 1})` : ""}</button>
                            {/each}
                          {/if}
                          {#if own}
                            <button class="plain icon danger" title="Remove" onclick={() => edit((d) => d.style[id].song!.splice(i, 1))}><Icon name="x" size={11} /></button>
                            <textarea class="links" rows="1" placeholder="Download links, one per line (need the file's hash)" value={(song.download ?? []).join("\n")}
                              onchange={(e) => edit((d) => (d.style[id].song![i].download = e.currentTarget.value.split("\n").map((l) => l.trim()).filter(Boolean)))}></textarea>
                          {:else}
                            <span class="faint" title="From the club's song library ({c.song_library?.join(', ')})">song library: {song.id}</span>
                            <button class="plain icon danger" title="Don't suggest it for this style" onclick={() => edit((d) => (d.style[id].suggested = (d.style[id].suggested ?? []).filter((x) => x !== song.id)))}><Icon name="x" size={11} /></button>
                          {/if}
                        </div>
                      {/each}
                      <button class="small" onclick={() => addSelectedSong(id)} title="The song selected in the library (its fingerprint, file hash and Spotify link)"><Icon name="plus" size={11} /> Add the Selected Song</button>
                    </div>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
        <button class="small" onclick={() => edit((d) => { const name = "New style"; d.style[slugify(name, d.style)] = blankStyle(name); })}><Icon name="plus" size={12} /> Add Style</button>

        <h4>Groups <span class="faint small">(styles counted together)</span></h4>
        {#each Object.entries(c.group) as [id, g] (id)}
          {@const gg: ClubGroup = g}
          <div class="group row small">
            <input type="text" value={gg.name} onchange={(e) => edit((d) => (d.group[id].name = e.currentTarget.value))} />
            <span class="chips">
              {#each Object.entries(c.style) as [sid, st] (sid)}
                {@const on = gg.styles.includes(sid)}
                <button class="badge chip" class:off={!on} style:--c={st.color || NEUTRAL}
                  onclick={() => edit((d) => (d.group[id].styles = on ? d.group[id].styles.filter((x) => x !== sid) : [...d.group[id].styles, sid]))}>{st.short || st.name}</button>
              {/each}
            </span>
            per set <RangeInput value={gg.count} onchange={(r) => edit((d) => (d.group[id].count = r))} />
            share <RangeInput kind="pct" value={gg.share} onchange={(r) => edit((d) => (d.group[id].share = r))} />
            <button class="plain icon danger" title="Remove" onclick={() => edit((d) => delete d.group[id])}><Icon name="x" size={12} /></button>
          </div>
        {/each}
        <button class="small" disabled={!Object.keys(c.style).length}
          onclick={() => edit((d) => { d.group[slugify("group", d.group)] = { name: "New group", styles: [Object.keys(d.style)[0]], count: null, share: null, when: [] }; })}>
          <Icon name="plus" size={12} /> Add Group
        </button>

        <h4>Rules for part of the set</h4>
        {#each Object.entries(c.rule) as [id, r] (id)}
          {@const rr: ClubRule = r}
          <div class="row small rule">
            <input type="text" value={rr.name} onchange={(e) => edit((d) => (d.rule[id].name = e.currentTarget.value))} />
            songs <RangeInput value={rr.songs} onchange={(x) => edit((d) => (d.rule[id].songs = x))} />
            each <RangeInput kind="time" value={rr.length} onchange={(x) => edit((d) => (d.rule[id].length = x))} /> long
            <button class="plain icon danger" title="Remove" onclick={() => edit((d) => delete d.rule[id])}><Icon name="x" size={12} /></button>
          </div>
        {/each}
        <button class="small" onclick={() => edit((d) => { d.rule[slugify("rule", d.rule)] = { name: "First songs", songs: { min: 1, max: 10 }, when: [], length: { min: null, max: 210 }, styles: [], count: null }; })}>
          <Icon name="plus" size={12} /> Add Rule
        </button>
        <span class="faint small">Rules can also count styles in part of the set (edit the file: <code>styles = […]</code>, <code>count = …</code>).</span>

        <h4>Tags</h4>
        {#each Object.entries(c.tag) as [id, t] (id)}
          {@const tt: ClubTag = t}
          <div class="row small">
            <input type="color" value={tt.color || NEUTRAL} onchange={(e) => edit((d) => (d.tag[id].color = e.currentTarget.value))} />
            <input type="text" value={tt.name} onchange={(e) => edit((d) => (d.tag[id].name = e.currentTarget.value))} />
            <label><input type="checkbox" checked={tt.announce} onchange={(e) => edit((d) => (d.tag[id].announce = e.currentTarget.checked))} /> on the projector</label>
            <input type="text" class="note" placeholder="Note" value={tt.note ?? ""} onchange={(e) => edit((d) => (d.tag[id].note = e.currentTarget.value || null))} />
            <button class="plain icon danger" title="Remove" onclick={() => edit((d) => delete d.tag[id])}><Icon name="x" size={12} /></button>
          </div>
        {/each}
        <button class="small" onclick={() => edit((d) => { d.tag[slugify("tag", d.tag)] = { name: "New tag", announce: true }; })}><Icon name="plus" size={12} /> Add Tag</button>

        <h4>Announcements</h4>
        <textarea rows="3" placeholder="One per line: reminders to announce (Announce menu)" value={c.announcements.join("\n")}
          onchange={(e) => edit((d) => (d.announcements = e.currentTarget.value.split("\n").map((l) => l.trim()).filter(Boolean)))}></textarea>
      {/if}
    </section>
  {:else}
    <section class="detail dim">
      Add a club: start from an example, import a club's file, or make your own.
    </section>
  {/if}
</div>

{#if menu}<ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

<style>
  h2 { margin: 0 0 8px; font: 600 var(--fs-lg) var(--font-display); }
  h3 { margin: 0; font-size: var(--fs-md); }
  h4 { margin: 16px 0 6px; font-size: var(--fs-sm); color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
  p { margin: 0 0 14px; max-width: 720px; }
  .layout { display: grid; grid-template-columns: 150px minmax(0, 1fr); gap: 16px; align-items: start; }
  .list { display: flex; flex-direction: column; gap: 2px; }
  .club { justify-content: space-between; text-align: left; padding: 5px 8px; border-radius: var(--radius-sm); }
  .club.sel { background: var(--bg-selected); }
  .add { margin-top: 6px; align-self: flex-start; }
  .detail { min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  .head { display: flex; gap: 6px; align-items: center; }
  .spacer { flex: 1; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .col { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
  .small { font-size: var(--fs-sm); }
  .versions { border-collapse: collapse; }
  .versions th { text-align: left; color: var(--text-faint); font-weight: 600; padding: 2px 8px; }
  .versions td { padding: 3px 8px; cursor: default; }
  .versions tr.on td { background: var(--bg-selected); }
  .from { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-dim); }
  .nowrap { white-space: nowrap; }
  .editbar { display: flex; gap: 8px; align-items: center; margin-top: 6px; }
  .err { color: var(--red); white-space: pre-wrap; }
  .file { width: 100%; min-height: 420px; font: var(--fs-sm) var(--font-mono); white-space: pre; }
  .form { display: grid; grid-template-columns: 150px minmax(0, 1fr); gap: 6px 12px; align-items: center; }
  .form .l { color: var(--text-dim); text-align: right; font-size: var(--fs-sm); }
  .w4 { width: 52px; }
  .w5 { width: 64px; }
  .styles { border-collapse: collapse; }
  .styles th { text-align: left; color: var(--text-faint); font-weight: 600; padding: 2px 4px; }
  .styles td { padding: 2px 3px; }
  .styles input[type="text"]:not(.w4):not(.w5):not(.end) { width: 124px; }
  .styles td :global(.end) { width: 42px; }
  .styles .w5 { width: 52px; }
  .styles .w4 { width: 44px; }
  .styles input[type="color"], .row input[type="color"] { width: 26px; height: 22px; padding: 0; border: none; background: none; }
  .more td { padding-bottom: 8px; }
  .more .row { margin-bottom: 4px; }
  .note, .styles .more input.note { width: 100%; }
  .chips { display: inline-flex; flex-wrap: wrap; gap: 3px; }
  .chip { height: 20px; cursor: pointer; }
  .chip.off { opacity: 0.35; filter: grayscale(0.7); }
  .danger { color: var(--red); }
  button.on { color: var(--accent); }
  code { font: var(--fs-xs) var(--font-mono); }
  .songs { display: flex; flex-direction: column; gap: 4px; margin-top: 6px; }
  .song { display: flex; flex-wrap: wrap; gap: 4px 8px; align-items: center; padding: 3px 0; border-top: 1px solid var(--border); }
  .stitle { min-width: 0; }
  .ids { font-size: var(--fs-xs); }
  .ok { color: var(--green); display: inline-flex; gap: 3px; align-items: center; }
  .links { width: 100%; font-size: var(--fs-xs); }
  .nsongs { font-size: var(--fs-xs); color: var(--accent); }
</style>

<script lang="ts">
  import { api, on } from "../lib/api";
  import { evaluate } from "../lib/smart";
  import { NO_STYLE, addToSet, app, audition, confirmAction, fillSlot, loadDeck } from "../lib/state.svelte";
  import type { SetlistEntry } from "../lib/types";
  import { fmtBpm, fmtPct, fmtTime, playback, playedDurationMs, tempoFit } from "../lib/tempo";
  import type { HistoryItem, Id, Row } from "../lib/types";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import StyleBadge from "./StyleBadge.svelte";
  import StyleFilterBar from "./StyleFilterBar.svelte";

  type SortKey = "position" | "title" | "artist" | "version" | "style" | "bpm" | "time" | "meter" | "plays" | "played";
  let sortKey = $state<SortKey>("artist");
  let sortAsc = $state(true);
  let history = $state<HistoryItem[]>([]);
  let historyRev = $state(0);
  $effect(() => {
    // History is derived from setlists: refresh when one changes.
    const un = [on("history-changed", () => historyRev++), on("setlist-changed", () => historyRev++), on("setlists-changed", () => historyRev++)];
    return () => un.forEach((u) => u.then((f) => f()));
  });
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewH = $state(400);
  let anchor = $state<number | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const ROW_H = 26;

  $effect(() => {
    void historyRev;
    if (app.view.kind === "history") api.history(300).then((h) => (history = h));
  });

  interface ViewRow extends Row {
    key: string;
    position?: number;
    playedAt?: number;
    /** The setlist entry this history row comes from (for "Remove from History"). */
    historyEntryId?: Id;
    /** Not labelled with the filtered style, but its tempo fits. */
    suggested?: boolean;
  }

  const baseRows = $derived.by((): ViewRow[] => {
    const v = app.view;
    const mk = (r: Row, extra: Partial<ViewRow> = {}): ViewRow => ({ ...r, key: r.version.id, ...extra });
    switch (v.kind) {
      case "all":
        return app.rows.map((r) => mk(r));
      case "missing":
        return app.rows.filter((r) => r.track.missing).map((r) => mk(r));
      case "crate": {
        const c = app.crates.find((c) => c.id === v.id);
        if (c?.smart) return evaluate(c.smart, app.rows, app.smartCtx).map((r) => mk(r));
        return (c?.versionIds ?? []).flatMap((id, i) => {
          const r = app.versionMap.get(id);
          return r ? [mk(r, { position: i + 1 })] : [];
        });
      }
      case "setlist":
        return (app.browsed?.entries ?? []).flatMap((e, i) => {
          const r = e.versionId ? app.versionMap.get(e.versionId) : undefined;
          return r ? [mk(r, { key: e.id, position: i + 1 })] : [];
        });
      case "history":
        return history.flatMap((h, i) => {
          const r = h.versionId ? app.versionMap.get(h.versionId) : undefined;
          const t = app.trackMap.get(h.trackId);
          const row = r ?? (t ? { track: t, version: t.versions[0] } : undefined);
          return row ? [mk(row, { key: `${h.entryId}-${i}`, playedAt: h.playedAt, historyEntryId: h.entryId })] : [];
        });
      default:
        return [];
    }
  });


  /** The current view's rows narrowed by the dance-style filter. */
  const styled = $derived.by((): ViewRow[] => {
    if (!app.styleFilter.length) return baseRows;
    const keys = new Set(app.styleFilter);
    const labelled = baseRows.filter((r) => (r.version.styles?.length ? r.version.styles.some((k) => keys.has(k)) : keys.has(NO_STYLE)));
    // With one style chosen, also suggest unlabelled songs from this view whose tempo fits it.
    const f = app.filterStyle;
    if (!f || !app.suggestByTempo || !app.clubTargets(f).some((t) => t.sweet || t.extended)) return labelled;
    const have = new Set(labelled.map((r) => r.track.id));
    const suggested = baseRows
      .filter((r) => !r.version.styles?.length && !have.has(r.track.id) && r.track.bpm)
      .map((r) => ({ ...r, key: "s-" + r.key, suggested: true }))
      .filter((r) => { const i = info(r); return !!i.st && i.fit !== "outside"; });
    return [...labelled, ...suggested];
  });

  const filtered = $derived.by(() => {
    const q = app.search.trim().toLowerCase();
    if (!q) return styled;
    const words = q.split(/\s+/);
    return styled.filter((r) => {
      const hay = `${r.track.title} ${r.track.artist} ${r.track.album} ${r.version.name} ${r.version.comment ?? ""} ${(r.version.styles ?? []).map((k) => app.styleName(k)).join(" ")} ${r.track.notes ?? ""}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  });

  /** How a row plays: tempo in the counting of its club style (the filtered one, if it has it), fit and length. */
  function info(r: ViewRow) {
    const f = app.filterStyle;
    const entry: Partial<SetlistEntry> | null = f && (r.suggested || r.version.styles?.includes(f)) ? { styles: [f] } : null;
    const pb = playback(r.track, r.version, entry, app.clubStyleOf);
    const st = app.clubStyleOf(r.version, entry, pb.target ?? r.track.bpm) ?? undefined;
    const dur = playedDurationMs(r.track, r.version, null, pb.rate, { fadeSeconds: app.fadeSeconds, trimSilence: app.settings?.trimSilence });
    return { pb, st, fit: tempoFit(pb.target, st), dur };
  }

  const sorted = $derived.by(() => {
    const rows = filtered.map((r) => ({ r, i: info(r) }));
    const dir = sortAsc ? 1 : -1;
    const naturalOrder = !!plainCrate || app.view.kind === "setlist" || app.view.kind === "history";
    const key = naturalOrder && sortKey === "position" ? "position" : sortKey;
    const val = (x: (typeof rows)[number]): string | number => {
      switch (key) {
        case "position": return x.r.position ?? 0;
        case "title": return x.r.track.title.toLowerCase();
        case "artist": return `${x.r.track.artist.toLowerCase()}\u0000${x.r.track.title.toLowerCase()}`;
        case "version": return x.r.version.name.toLowerCase();
        case "style": return (x.r.version.styles ?? []).map((k) => app.styleName(k)).join(" ");
        case "bpm": return x.i.pb.target ?? x.r.track.bpm ?? 0;
        case "time": return x.i.dur;
        case "meter": return x.r.track.beatsPerBar ?? 0;
        case "plays": return x.r.track.playCount;
        case "played": return x.r.playedAt ?? x.r.track.lastPlayedAt ?? 0;
      }
    };
    if (app.view.kind === "history" && sortKey === "artist") return rows;
    return rows.slice().sort((a, b) => {
      if (!!a.r.suggested !== !!b.r.suggested) return a.r.suggested ? 1 : -1;
      const va = val(a), vb = val(b);
      return (va < vb ? -1 : va > vb ? 1 : 0) * dir;
    });
  });

  $effect(() => {
    // Crates and setlists default to their own order.
    const k = app.view.kind;
    if ((k === "crate" && plainCrate) || k === "setlist") { sortKey = "position"; sortAsc = true; }
    else if (sortKey === "position") { sortKey = "artist"; sortAsc = true; }
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - 10));
  const end = $derived(Math.min(sorted.length, Math.ceil((scrollTop + viewH) / ROW_H) + 10));
  const visible = $derived(sorted.slice(start, end));
  const selSet = $derived(new Set(app.selected));
  const viewCrate = $derived(app.view.kind === "crate" ? app.crates.find((c) => c.id === (app.view as { id: Id }).id) : undefined);
  const plainCrate = $derived(viewCrate && !viewCrate.smart ? viewCrate : undefined);

  function sortBy(k: SortKey) {
    if (sortKey === k) sortAsc = !sortAsc;
    else { sortKey = k; sortAsc = true; }
  }

  function select(e: MouseEvent, idx: number) {
    const id = sorted[idx].r.version.id;
    if (e.shiftKey && anchor != null) {
      const [a, b] = [Math.min(anchor, idx), Math.max(anchor, idx)];
      app.selected = sorted.slice(a, b + 1).map((x) => x.r.version.id);
    } else if (e.metaKey || e.ctrlKey) {
      app.selected = selSet.has(id) ? app.selected.filter((x) => x !== id) : [...app.selected, id];
      anchor = idx;
    } else {
      app.selected = [id];
      anchor = idx;
    }
    app.inspectTrack = sorted[idx].r.track.id;
    app.inspectEntry = null;
  }

  /** Double-click / Enter: fill the slot being filled, else load onto the off-air deck (paused). */
  function choose(r: Row) {
    if (app.fillingSlot) return fillSlot(r.version.id);
    loadDeck(app.freeDeck, r.version.id);
  }

  function meterLabel(n: number | null | undefined): string {
    return n === 3 || n === 6 ? "triple" : n === 2 || n === 4 ? "duple" : "";
  }

  const slot = $derived(app.fillingSlot ? app.current?.entries.find((e) => e.id === app.fillingSlot) : undefined);
  const slotNumber = $derived(slot ? (app.current?.entries.filter((e) => e.status !== "skipped").indexOf(slot) ?? -1) + 1 : 0);
  $effect(() => {
    // The slot was filled or removed elsewhere.
    if (app.fillingSlot && (!slot || slot.versionId)) app.fillingSlot = null;
  });

  /** Play in the headphones on the off-air deck. */
  function preview(r: Row) {
    audition(r.version.id);
  }

  function dragStart(e: DragEvent, r: Row) {
    if (!selSet.has(r.version.id)) app.selected = [r.version.id];
    e.dataTransfer?.setData("application/x-orpheus-versions", JSON.stringify(app.selected));
    e.dataTransfer!.effectAllowed = "copy";
  }

  function scrollToIndex(i: number) {
    if (!scroller) return;
    const top = i * ROW_H;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW_H > scroller.scrollTop + viewH - ROW_H) scroller.scrollTop = top - viewH + ROW_H * 2;
  }

  function move(delta: number) {
    if (!sorted.length) return;
    const cur = sorted.findIndex((x) => x.r.version.id === app.selected[app.selected.length - 1]);
    const i = Math.max(0, Math.min(sorted.length - 1, (cur < 0 ? -1 : cur) + delta));
    app.selected = [sorted[i].r.version.id];
    anchor = i;
    app.inspectTrack = sorted[i].r.track.id;
    scrollToIndex(i);
  }

  function selectedRow(): Row | undefined {
    return app.selected.length ? app.versionMap.get(app.selected[app.selected.length - 1]) : undefined;
  }

  // Controller browse knob / load buttons.
  let lastNav = 0;
  $effect(() => {
    const m = app.midiNav;
    if (m.n === lastNav) return;
    lastNav = m.n;
    if (m.action === "browse.scroll") move(m.delta ?? 0);
    else if (m.action === "browse.audition") { const r = selectedRow(); if (r) preview(r); }
    else if (m.action === "deck.load" && m.deck != null) { const r = selectedRow(); if (r) loadDeck(m.deck, r.version.id); }
    else if (m.action === "browse.queueNext" && app.selected.length) addToSet(app.selected, "next");
    else if (m.action === "browse.queueEnd" && app.selected.length) addToSet(app.selected, "end");
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) { const r = selectedRow(); if (r) choose(r); }
    else if (e.key === "Escape" && app.fillingSlot) app.fillingSlot = null;
    else if ((e.key === "Backspace" || e.key === "Delete") && plainCrate && app.selected.length) {
      api.removeFromCrate(plainCrate.id, app.selected);
    }
  }

  function openMenu(e: MouseEvent, r: ViewRow, idx: number) {
    e.preventDefault();
    if (!selSet.has(r.version.id)) select(e, idx);
    const ids = app.selected.slice();
    const historyIds = sorted.filter((x) => selSet.has(x.r.version.id) && x.r.historyEntryId).map((x) => x.r.historyEntryId!);
    const historyItems: MenuItem[] = r.historyEntryId
      ? [
          {
            label: "Remove from History",
            action: () => app.run(api.removeHistory(historyIds.length ? historyIds : [r.historyEntryId!])),
          },
          { separator: true },
        ]
      : [];
    const items: MenuItem[] = [
      ...historyItems,
      ...(app.fillingSlot ? [{ label: "Use for the Slot", action: () => fillSlot(r.version.id) }, { separator: true } as MenuItem] : []),
      { label: "Preview in Headphones (free deck)", action: () => preview(r) },
      { label: "Load onto Deck A", disabled: app.decks[0].onAir && app.decks[0].status === "playing", action: () => loadDeck(0, r.version.id) },
      { label: "Load onto Deck B", disabled: app.decks[1].onAir && app.decks[1].status === "playing", action: () => loadDeck(1, r.version.id) },
      { label: "Play Next", action: () => addToSet(ids, "next") },
      { label: "Add to End of Set", action: () => addToSet(ids, "end") },
      { separator: true },
      ...app.crates.filter((c) => !c.smart).map((c) => ({ label: `Add to Crate “${c.name}”`, action: () => api.addToCrate(c.id, ids) })),
      ...(plainCrate ? [{ label: "Remove from Crate", action: () => api.removeFromCrate(plainCrate.id, ids) }] : []),
      { separator: true },
      ...styleMenu(ids, r),
      ...tagMenu(ids),
      { separator: true },
      { label: "New Version of This Song…", action: () => { app.inspectTrack = r.track.id; app.rightTab = "info"; } },
      { label: "Re-analyze", action: () => api.analyze([...new Set(ids.map((id) => app.versionMap.get(id)?.track.id).filter(Boolean) as Id[])]) },
      ...(new Set(ids.map((id) => app.versionMap.get(id)?.track.id)).size > 1
        ? [{ label: "Merge Selected Songs (same recording)", action: () => mergeSelected(ids) }]
        : []),
      { separator: true },
      { label: "Remove from Library…", danger: true, action: () => deleteSelected(ids) },
    ];
    menu = { x: e.clientX, y: e.clientY, items };
  }

  /** Add / remove your tags on the selected songs. */
  function tagMenu(ids: Id[]): MenuItem[] {
    if (!app.myTags.length) return [];
    const versions = ids.map((id) => app.versionMap.get(id)?.version).filter(Boolean) as { tags?: string[] }[];
    return [
      { separator: true },
      ...app.myTags.map((t) => {
        const all = versions.every((v) => v.tags?.includes(t.key));
        return { label: `Tag: ${t.name}`, checked: all, action: () => app.run(api.setTag(t.key, ids, !all)) };
      }),
    ];
  }

  /** Add / remove your dance styles on the selected songs. */
  function styleMenu(ids: Id[], r: ViewRow): MenuItem[] {
    const versions = ids.map((id) => app.versionMap.get(id)?.version).filter(Boolean) as { styles?: string[] }[];
    const has = (k: string) => versions.filter((v) => v.styles?.includes(k)).length;
    const items: MenuItem[] = app.myStyles.map((s) => {
      const all = has(s.key) === versions.length;
      return {
        label: s.name,
        checked: all,
        action: () => app.run(all ? api.styleRemove(ids, s.key) : api.styleAdd(s.key, ids)),
      };
    });
    if (versions.some((v) => v.styles?.length)) items.push({ label: "Remove All Dance Styles", action: () => app.run(api.styleRemove(ids, null)) });
    if (!items.length) items.push({ label: "Add Dance Styles…", action: () => (app.settingsOpen = "styles") });
    else items.push({ label: "More in Info…", action: () => { app.inspectTrack = r.track.id; app.rightTab = "info"; } });
    return items;
  }

  async function mergeSelected(ids: Id[]) {
    const tids = [...new Set(ids.map((id) => app.versionMap.get(id)!.track.id))];
    if (!(await confirmAction(`Merge ${tids.length} songs into one? Their sources and versions are combined; setlists keep working.`, "Merge"))) return;
    await app.run(api.mergeTracks(tids[0], tids.slice(1)));
  }

  async function deleteSelected(ids: Id[]) {
    const tids = [...new Set(ids.map((id) => app.versionMap.get(id)!.track.id))];
    if (!(await confirmAction(`Remove ${tids.length} song(s) and all their versions from the library? Your music files are not touched.`, "Remove"))) return;
    await app.run(api.deleteTracks(tids));
  }

  const showPos = $derived(!!plainCrate || app.view.kind === "setlist");
</script>

<StyleFilterBar rows={baseRows} />
{#if slot}
  <div class="filling" role="status">
    <Icon name="list" size={13} />
    <span>Choosing a song for slot {slotNumber}{slot.styles?.length ? ` (${slot.styles.map((k) => app.styleName(k)).join(" / ")})` : ""}: double-click a song, press <Icon name="check" size={11} />, or drag it onto the slot.</span>
    {#if app.styleFilter.length}<button class="plain small" onclick={() => (app.styleFilter = [])}>Show All Songs</button>{/if}
    <button class="plain small" onclick={() => (app.fillingSlot = null)}>Cancel</button>
  </div>
{/if}
<div class="table" role="grid" tabindex="0" onkeydown={keydown} aria-label="Library">
  <div class="thead">
    {#if showPos}<button class="c pos" onclick={() => sortBy("position")}>#</button>{/if}
    <button class="c title" onclick={() => sortBy("title")}>Title {sortKey === "title" ? (sortAsc ? "▲" : "▼") : ""}</button>
    <button class="c artist" onclick={() => sortBy("artist")}>Artist {sortKey === "artist" ? (sortAsc ? "▲" : "▼") : ""}</button>
    <button class="c version" onclick={() => sortBy("version")}>Version</button>
    <button class="c style" onclick={() => sortBy("style")}>Style</button>
    <button class="c bpm" onclick={() => sortBy("bpm")}>BPM {sortKey === "bpm" ? (sortAsc ? "▲" : "▼") : ""}</button>
    <button class="c time" onclick={() => sortBy("time")}>Time {sortKey === "time" ? (sortAsc ? "▲" : "▼") : ""}</button>
    <button class="c meter" onclick={() => sortBy("meter")} title="Meter: triple (3/4, 6/8) or duple (2/4, 4/4)">Meter</button>
    <button class="c plays" onclick={() => sortBy(app.view.kind === "history" ? "played" : "plays")}>{app.view.kind === "history" ? "Played" : "Plays"}</button>
    <span class="c actions"></span>
  </div>
  <div class="tbody" bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
    <div style:height="{sorted.length * ROW_H}px" style:position="relative">
      {#each visible as { r, i: inf }, k (r.key)}
        {@const idx = start + k}
        {@const fit = inf.fit}
        <div
          class="tr"
          class:sel={selSet.has(r.version.id)}
          class:missing={r.track.missing}
          class:suggested={r.suggested}
          class:inset={app.view.kind !== "history" && app.playedInSet.has(r.track.id)}
          title={r.suggested ? `Not labelled ${app.styleName(app.filterStyle)} yet — its tempo fits` : undefined}
          class:playing={app.decks.some((d) => d.versionId === r.version.id && d.onAir && d.status === "playing")}
          class:previewing={app.decks.some((d) => d.versionId === r.version.id && !(d.onAir && d.status === "playing"))}
          style:top="{idx * ROW_H}px"
          role="row"
          tabindex="-1"
          draggable="true"
          ondragstart={(e) => dragStart(e, r)}
          onclick={(e) => select(e, idx)}
          ondblclick={() => choose(r)}
          oncontextmenu={(e) => openMenu(e, r, idx)}
          onkeydown={() => {}}
        >
          {#if showPos}<span class="c pos num faint">{r.position}</span>{/if}
          <span class="c title" title={r.track.title}>
            {#if r.track.missing}<span title="File missing on this computer"><Icon name="warning" size={12} /></span>{/if}
            {#if r.track.sources.some((s) => s.kind === "spotify") && !r.track.files.length}<span class="sp" title="Spotify">●</span>{/if}
            {r.track.title}
          </span>
          <span class="c artist" title={r.track.artist}>{r.track.artist}</span>
          <span class="c version faint" title={r.version.comment ?? ""}>{app.versionLabel(r.version, r.track)}</span>
          <span class="c style">{#if r.suggested}<span class="sugg">tempo fits</span>{:else}<StyleBadge styles={(r.version.styles ?? []).map((k) => app.mine(k))} short />{/if}</span>
          <span class="c bpm num" class:fit-sweet={fit === "sweet"} class:fit-out={fit === "outside"}>
            {fmtBpm(inf.pb.target ?? r.track.bpm)}
            {#if Math.abs(inf.pb.rate - 1) > 0.001}<span class="rate">{fmtPct(inf.pb.rate)}</span>{/if}
            {#if !r.track.bpm && !r.track.analyzed && r.track.files.length}<span class="faint">…</span>{/if}
          </span>
          <span class="c time num" class:long={inf.dur > 270000}>{fmtTime(inf.dur)}</span>
          <span class="c meter num faint" title={r.track.meterSource === "user" ? "set by you" : r.track.beatsPerBar ? "detected" : ""}>{meterLabel(r.track.beatsPerBar)}</span>
          <span class="c plays num faint">
            {#if r.playedAt}{new Date(r.playedAt).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}{:else}{r.track.playCount || ""}{/if}
          </span>
          <span class="c actions">
            <button class="plain icon mini" title="Preview on the free deck" onclick={(e) => { e.stopPropagation(); preview(r); }}><Icon name="headphones" size={13} /></button>
            {#if app.fillingSlot}
              <button class="plain icon mini fill" title="Use this song for the slot" onclick={(e) => { e.stopPropagation(); fillSlot(r.version.id); }}><Icon name="check" size={13} /></button>
            {:else}
              <button class="plain icon mini" title="Play next" onclick={(e) => { e.stopPropagation(); addToSet([r.version.id], "next"); }}><Icon name="plus" size={13} /></button>
            {/if}
          </span>
        </div>
      {/each}
    </div>
    {#if sorted.length === 0}
      <div class="empty">
        {#if app.libraryTracks.length === 0}
          <p>Your library is empty.</p>
          <p class="dim">Add a music folder in Settings → Library, drag files onto this window, or connect Spotify.</p>
          <button onclick={() => (app.settingsOpen = "library")}>Add Music Folder…</button>
        {:else if app.search}
          No songs match “{app.search}”.
        {:else if app.filterStyle}
          Nothing here is labelled {app.styleName(app.filterStyle)}{app.suggestByTempo ? ", and no unlabelled songs fit its tempo" : ""}. Drag songs onto the style above to label them.
        {:else if app.styleFilter.length}
          No songs here match the style filter.
        {:else if viewCrate?.smart}
          No songs match this smart crate's rules yet.
        {:else}
          Nothing here yet.
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .table { display: flex; flex-direction: column; min-height: 0; flex: 1; outline: none; }
  .thead, .tr { display: grid; grid-template-columns: var(--cols); align-items: center; column-gap: 8px; padding: 0 10px; }
  .table { --cols: minmax(160px, 2.2fr) minmax(110px, 1.4fr) minmax(60px, 0.8fr) 92px 92px 52px 46px 70px 52px; }
  .table:has(.pos) { --cols: 30px minmax(160px, 2.2fr) minmax(110px, 1.4fr) minmax(60px, 0.8fr) 92px 92px 52px 46px 70px 52px; }
  .thead { height: 26px; border-bottom: 1px solid var(--border); background: var(--bg-elev); }
  .thead .c { height: 26px; padding: 0; background: none; border: none; justify-content: flex-start; font-size: var(--fs-xs); color: var(--text-faint); font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; }
  .tbody { flex: 1; overflow-y: auto; position: relative; }
  .tr { position: absolute; left: 0; right: 0; height: 26px; border-bottom: 1px solid rgba(255,255,255,0.03); }
  .tr:nth-child(even) { background: rgba(255,255,255,0.015); }
  .tr:hover { background: var(--bg-hover); }
  .tr.sel { background: var(--bg-selected); }
  .tr.missing { color: var(--text-faint); }
  .tr.suggested { color: var(--text-dim); font-style: italic; }
  .tr.suggested:first-of-type, .tr:not(.suggested) + .tr.suggested { border-top: 1px dashed var(--border-strong); }
  .sugg { font-size: var(--fs-xs); color: var(--text-faint); font-style: normal; }
  /* Already played in the open set. */
  .tr.inset { color: var(--text-faint); }
  .tr.inset .title { color: var(--text-faint); }
  .tr.playing .title { color: var(--main); }
  .tr.previewing .title { color: var(--preview); }
  .c { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .title { display: flex; align-items: center; gap: 5px; }
  .title :global(svg) { color: var(--yellow); flex: none; }
  .sp { color: #1db954; font-size: 9px; }
  .bpm { display: flex; gap: 4px; align-items: baseline; }
  .rate { font-size: var(--fs-xs); color: var(--teal); }
  .fit-sweet { color: var(--green); }
  .fit-out { color: var(--yellow); }
  .time.long { color: var(--yellow); }
  .actions { display: flex; gap: 2px; justify-content: flex-end; opacity: 0; }
  .tr:hover .actions, .tr.sel .actions { opacity: 1; }
  .mini { width: 22px; height: 20px; }
  .filling { display: flex; align-items: center; gap: 8px; padding: 5px 12px; font-size: var(--fs-sm); background: color-mix(in srgb, var(--yellow) 18%, var(--bg-elev)); border-bottom: 1px solid var(--border); }
  .filling span { flex: 1; }
  .fill { color: var(--yellow); }
  .empty { position: absolute; inset: 40px 0 auto; display: flex; flex-direction: column; align-items: center; gap: 8px; }
  .empty p { margin: 0; }
</style>

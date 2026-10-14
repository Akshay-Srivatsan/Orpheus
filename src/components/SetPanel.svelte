<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { addToSet, app, audition, spotifyVersions, confirmAction } from "../lib/state.svelte";
  import { entryStyleKeys, NEUTRAL } from "../lib/styles";
  import { fmtBpm, fmtTime, playback, playedDurationMs } from "../lib/tempo";
  import type { Id, SetlistEntry } from "../lib/types";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";

  const set = $derived(app.current);
  const selSet = $derived(new Set(app.selectedEntries));
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let dropIndex = $state<number | null>(null);
  let renaming = $state(false);
  let listEl: HTMLDivElement | undefined = $state();

  const rows = $derived(
    (set?.entries ?? []).map((e, i) => {
      const r = e.versionId ? app.versionMap.get(e.versionId) : undefined;
      const pb = r ? playback(r.track, r.version, e, app.clubStyleOf) : null;
      const dur = r ? playedDurationMs(r.track, r.version, e, pb!.rate, { fadeSeconds: app.fadeSeconds, trimSilence: app.settings?.trimSilence }) : 0;
      const shown = app.shownFor(r?.version, e, pb?.target ?? null);
      const label = e.label || shown.map((s) => s.short).join(" / ");
      // A song coming up that the room has already heard in this set.
      const repeat = e.status === "pending" && !!r && app.playedInSet.has(r.track.id);
      return { e, i, r, pb, dur, shown, label, repeat };
    }),
  );
  const totalMs = $derived.by(() => {
    const songs = rows.filter((x) => x.e.status !== "skipped");
    const gaps = Math.max(0, songs.length - 1) * (app.perform?.gapSeconds ?? 8) * 1000;
    return songs.reduce((a, x) => a + x.dur, 0) + gaps;
  });
  let number = $derived.by(() => {
    // Number songs, not skipped entries.
    const m = new Map<Id, number>();
    let n = 0;
    for (const x of rows) if (x.e.status !== "skipped") m.set(x.e.id, ++n);
    return m;
  });
  const remainingMs = $derived(
    rows.filter((x) => x.e.status === "pending").reduce((a, x) => a + x.dur + (app.perform?.gapSeconds ?? 8) * 1000, 0),
  );

  // Keep the playing entry in view during a set.
  $effect(() => {
    const id = app.perform?.currentEntryId;
    if (!id || !listEl) return;
    const el = listEl.querySelector(`[data-entry="${id}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  });

  function select(e: MouseEvent, entry: SetlistEntry) {
    if (e.metaKey || e.ctrlKey) {
      app.selectedEntries = selSet.has(entry.id) ? app.selectedEntries.filter((x) => x !== entry.id) : [...app.selectedEntries, entry.id];
    } else if (e.shiftKey && app.selectedEntries.length && set) {
      const ids = set.entries.map((x) => x.id);
      const a = ids.indexOf(app.selectedEntries[0]), b = ids.indexOf(entry.id);
      app.selectedEntries = ids.slice(Math.min(a, b), Math.max(a, b) + 1);
    } else {
      app.selectedEntries = [entry.id];
    }
    app.inspectEntry = entry.id;
    const r = entry.versionId ? app.versionMap.get(entry.versionId) : undefined;
    app.inspectTrack = r?.track.id ?? null;
    // Selecting an empty slot filters the library to its style.
    if (!entry.versionId && entry.styles?.length) {
      app.styleFilter = [...entry.styles];
      app.selected = [];
    }
  }

  function preview(entry: SetlistEntry) {
    if (entry.versionId) audition(entry.versionId, entry.id);
  }

  async function remove(ids: Id[]) {
    if (!set) return;
    const playing = app.decks.some((d) => d.entryId && ids.includes(d.entryId) && d.onAir && d.status === "playing");
    if (playing) return app.toast("That song is playing right now.");
    await app.run(api.deleteEntries(set.id, ids));
    app.selectedEntries = [];
  }

  async function toggleTag(entry: SetlistEntry, tag: string) {
    const tags = entry.tags ?? [];
    await app.run(api.updateEntry(entry.id, { tags: tags.includes(tag) ? tags.filter((t) => t !== tag) : [...tags, tag] }));
  }

  function openMenu(ev: MouseEvent, entry: SetlistEntry) {
    ev.preventDefault();
    if (!selSet.has(entry.id)) select(ev, entry);
    const ids = app.selectedEntries.slice();
    const isCurrent = entry.id === app.perform?.currentEntryId;
    const items: MenuItem[] = [
      { label: "Play Now", disabled: !entry.versionId || isCurrent, action: () => app.run(api.playEntry(entry.id)) },
      { label: "Preview in Headphones (free deck)", disabled: !entry.versionId, action: () => preview(entry) },
      ...([0, 1] as const).map((d) => ({
        label: `Load onto Deck ${d === 0 ? "A" : "B"}`,
        disabled: !entry.versionId || (app.decks[d].onAir && app.decks[d].status === "playing"),
        action: () => entry.versionId && api.deckLoad(d, entry.versionId, { entryId: entry.id }),
      })),
      { label: "Move to Play Next", disabled: isCurrent, action: () => moveNext(ids) },
      { separator: true },
      entry.status === "skipped"
        ? { label: "Unskip", action: () => api.updateEntry(entry.id, { status: "pending" }) }
        : { label: "Skip", disabled: isCurrent, action: () => ids.forEach((id) => api.updateEntry(id, { status: "skipped" })) },
      ...(entry.status === "played" ? [{ label: "Mark as Not Played", action: () => api.updateEntry(entry.id, { status: "pending" }) }] : []),
      { separator: true },
      ...(app.profile?.tags ?? []).map((t) => ({
        label: t.name, checked: (entry.tags ?? []).includes(t.id), action: () => toggleTag(entry, t.id),
      })),
      ...styleItems(entry),
      { separator: true },
      { label: "Edit Overrides for This Set…", action: () => { app.inspectEntry = entry.id; app.rightTab = "info"; } },
      { label: "Remove from Set", danger: true, disabled: isCurrent, action: () => remove(ids) },
    ];
    menu = { x: ev.clientX, y: ev.clientY, items };
  }

  /** Choose which of a song's styles this set shows (for songs danced as several). */
  function styleItems(entry: SetlistEntry): MenuItem[] {
    const v = entry.versionId ? app.versionMap.get(entry.versionId)?.version : undefined;
    const all = v?.styles ?? [];
    if (all.length < 2) return [];
    const shown = entryStyleKeys(v, entry);
    const toggle = (k: string) => {
      const next = shown.includes(k) ? shown.filter((x) => x !== k) : all.filter((x) => x === k || shown.includes(x));
      return api.updateEntry(entry.id, { styles: next.length === all.length || !next.length ? null : next });
    };
    return [{ separator: true }, ...all.map((k) => ({ label: `Show as ${app.styleName(k)}`, checked: shown.includes(k), action: () => toggle(k) }))];
  }

  async function moveNext(ids: Id[]) {
    if (!set) return;
    const order = set.entries.map((e) => e.id).filter((id) => !ids.includes(id));
    const cur = order.indexOf(app.perform?.currentEntryId ?? "");
    const lastDone = set.entries.filter((e) => !ids.includes(e.id)).findLastIndex((e) => e.status !== "pending");
    const at = Math.max(cur, lastDone) + 1;
    order.splice(at, 0, ...ids);
    await app.run(api.reorderEntries(set.id, order));
  }

  function dragStart(ev: DragEvent, entry: SetlistEntry) {
    if (!selSet.has(entry.id)) app.selectedEntries = [entry.id];
    ev.dataTransfer?.setData("application/x-orpheus-entries", JSON.stringify(app.selectedEntries));
    if (entry.versionId) ev.dataTransfer?.setData("application/x-orpheus-versions", JSON.stringify([entry.versionId]));
    ev.dataTransfer!.effectAllowed = "copyMove";
  }

  function dragOver(ev: DragEvent, i: number) {
    ev.preventDefault();
    const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    dropIndex = ev.clientY > rect.top + rect.height / 2 ? i + 1 : i;
  }

  async function drop(ev: DragEvent) {
    ev.preventDefault();
    const at = dropIndex ?? set?.entries.length ?? 0;
    dropIndex = null;
    if (!set) {
      const sp = JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-spotify") || "[]");
      const ids = sp.length ? await spotifyVersions(sp, false) : JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
      if (ids.length) addToSet(ids, "end");
      return;
    }
    const moving: Id[] = JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-entries") || "[]");
    if (moving.length) {
      const ids = set.entries.map((e) => e.id);
      const before = ids.slice(0, at).filter((id) => !moving.includes(id));
      const after = ids.slice(at).filter((id) => !moving.includes(id));
      await app.run(api.reorderEntries(set.id, [...before, ...moving, ...after]));
      return;
    }
    const spotify = JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-spotify") || "[]");
    if (spotify.length) {
      const vids = await spotifyVersions(spotify, false);
      await app.run(api.insertEntries(set.id, vids.map((v) => ({ versionId: v })), at));
      return;
    }
    const versions: Id[] = JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
    if (versions.length) await app.run(api.insertEntries(set.id, versions.map((v) => ({ versionId: v })), at));
  }

  async function addSlot(style: string) {
    if (!set) return;
    await app.run(api.insertEntries(set.id, [{ styles: [style] }]));
  }

  function slotMenu(ev: MouseEvent) {
    const items: MenuItem[] = app.myStyles.map((s) => {
      const club = app.show(s.key);
      return { label: club.club && club.name !== s.name ? `${s.name} (${club.name})` : s.name, action: () => addSlot(s.key) };
    });
    if (!items.length) items.push({ label: "Add dance styles in Settings → Dance Styles", action: () => (app.settingsOpen = "styles") });
    menu = { x: ev.clientX, y: ev.clientY, items };
  }

  function setMenu(ev: MouseEvent) {
    if (!set) return;
    const s = set;
    const items: MenuItem[] = [
      { label: "Rename", action: () => (renaming = true) },
      { label: "Duplicate", action: () => api.duplicateSetlist(s.id, `${s.name} (copy)`) },
      { label: "Reset Played / Skipped", action: () => confirmAction("Mark every song in this set as not played?", "Reset").then((ok) => { if (ok) api.resetSetlist(s.id); }) },
      { separator: true },
      { label: "Export as CSV…", action: exportCsv },
      { label: "Print Setlist…", action: () => window.print() },
      { separator: true },
      { label: "Close Set", action: () => api.performOpen(null) },
      { label: "Delete Set…", danger: true, action: () => confirmAction(`Delete “${s.name}”?`).then((ok) => { if (ok) api.deleteSetlist(s.id); }) },
    ];
    menu = { x: ev.clientX, y: ev.clientY, items };
  }

  async function exportCsv() {
    if (!set) return;
    const esc = (v: unknown) => `"${String(v ?? "").replace(/"/g, '""')}"`;
    const lines = [["#", "Style", "Title", "Artist", "Version", "BPM", "Length", "Tags", "Note", "Status"].map(esc).join(",")];
    for (const x of rows) {
      lines.push([
        number.get(x.e.id) ?? "", x.e.label || x.shown.map((s) => s.name).join(" / "), x.r?.track.title ?? "(slot)", x.r?.track.artist ?? "", x.r?.version.name ?? "",
        x.pb?.target ? fmtBpm(x.pb.target) : "", x.dur ? fmtTime(x.dur) : "",
        (x.e.tags ?? []).map((t) => app.profile?.tags.find((d) => d.id === t)?.name ?? t).join("; "), x.e.note ?? "", x.e.status,
      ].map(esc).join(","));
    }
    const path = await save({ defaultPath: `${set.name}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (path) await app.run(api.writeTextFile(path, lines.join("\n") + "\n"));
  }

  function keydown(e: KeyboardEvent) {
    if ((e.key === "Backspace" || e.key === "Delete") && app.selectedEntries.length) remove(app.selectedEntries);
  }
</script>

<div class="setpanel" role="region" aria-label="Setlist">
  {#if !set}
    <div class="noset">
      <Icon name="list" size={28} />
      <p>No set open</p>
      <p class="dim small">Live DJing: just press <b>+</b> on a song (or Play Next in the preview deck) — a live set is created for you.<br />Preset DJing: plan a setlist ahead of time.</p>
      <button class="primary" onclick={() => (app.newSetlistOpen = true)}>New Setlist…</button>
      {#if app.setlists.length}
        <div class="section-title" style="margin-top:16px">Recent</div>
        {#each app.setlists.slice(0, 6) as s (s.id)}
          <button class="plain recent" onclick={() => api.performOpen(s.id)}>{s.name} <span class="faint">{s.entryCount}</span></button>
        {/each}
      {/if}
    </div>
  {:else}
    <div class="head">
      {#if renaming}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="name-input" value={set.name} autofocus
          onkeydown={(e) => { if (e.key === "Enter") { api.updateSetlist(set.id, { name: e.currentTarget.value }); renaming = false; } if (e.key === "Escape") renaming = false; }}
          onblur={(e) => { api.updateSetlist(set.id, { name: e.currentTarget.value }); renaming = false; }} />
      {:else}
        <button class="plain name" ondblclick={() => (renaming = true)} title="Double-click to rename">{set.name}</button>
      {/if}
      <button class="plain icon" title="Setlist actions" onclick={setMenu}><Icon name="dots" /></button>
    </div>
    <div class="subhead">
      <div class="segmented" title="Live: build the set as you go. Preset: planned ahead. Affects the projector layout.">
        <button class:on={set.mode === "live"} onclick={() => api.updateSetlist(set.id, { mode: "live" })}>Live</button>
        <button class:on={set.mode === "preset"} onclick={() => api.updateSetlist(set.id, { mode: "preset" })}>Preset</button>
      </div>
      <select title="Club: whose style names, tempo ranges and tags this set uses" value={set.profileId}
        onchange={(e) => api.updateSetlist(set.id, { profileId: e.currentTarget.value })}>
        {#each app.profiles as p}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </div>

    <div class="entries" bind:this={listEl} role="listbox" aria-multiselectable="true" tabindex="0" onkeydown={keydown}
      ondragover={(e) => { e.preventDefault(); if (!set.entries.length) dropIndex = 0; }}
      ondragleave={(e) => { if (e.currentTarget === e.target) dropIndex = null; }}
      ondrop={drop}>
      {#each rows as x (x.e.id)}
        {@const isCur = x.e.id === app.perform?.currentEntryId}
        {@const isNext = x.e.id === app.perform?.nextEntryId}
        {#if dropIndex === x.i}<div class="drop-line"></div>{/if}
        <div
          class="entry {x.e.status}"
          class:cur={isCur}
          class:next={isNext}
          class:sel={selSet.has(x.e.id)}
          data-entry={x.e.id}
          style:--c={x.shown[0]?.color ?? NEUTRAL}
          role="option"
          aria-selected={selSet.has(x.e.id)}
          draggable="true"
          ondragstart={(ev) => dragStart(ev, x.e)}
          ondragover={(ev) => dragOver(ev, x.i)}
          onclick={(ev) => select(ev, x.e)}
          ondblclick={() => preview(x.e)}
          oncontextmenu={(ev) => openMenu(ev, x.e)}
          onkeydown={() => {}}
          tabindex="-1"
        >
          <span class="num n">{#if isCur && app.decks[app.liveDeck].status === "playing"}<Icon name="play" size={10} />{:else}{number.get(x.e.id) ?? ""}{/if}</span>
          <span class="stripe"></span>
          <div class="body">
            <div class="l1">
              <span class="sname" class:unmapped={!x.e.label && x.shown.some((s) => s.unmapped)}
                title={x.shown.map((s) => (s.unmapped ? `${s.name}: not a ${app.profile?.name} style` : s.club && s.name !== app.styleName(s.key) ? `${s.name} (your ${app.styleName(s.key)})` : s.name)).join("\n")}
                >{x.label || "—"}</span>
              {#if x.r}
                <span class="t" title={x.r.track.title}>{x.r.track.title}</span>
              {:else}
                <span class="t slot">{x.shown.length ? `Pick a ${x.shown.map((s) => s.name).join(" / ")}` : "Empty slot"}</span>
              {/if}
            </div>
            <div class="l2">
              {#if x.repeat}<span class="badge tag" style:--c="var(--yellow)" title="Already played in this set">played</span>{/if}
              {#if x.r}<span class="a">{x.r.track.artist}{#if app.versionLabel(x.r.version, x.r.track)} · {x.r.version.name}{/if}</span>{/if}
              {#each x.e.tags ?? [] as tag}
                {@const t = app.profile?.tags.find((d) => d.id === tag)}
                <span class="badge tag" style:--c={t?.color ?? "#999"}>{t?.name ?? tag}</span>
              {/each}
              {#if x.e.note}<span class="note" title={x.e.note}>✎ {x.e.note}</span>{/if}
            </div>
          </div>
          <div class="right num">
            <span class="bpm">{x.pb?.target ? fmtBpm(x.pb.target) : ""}</span>
            <span class="dur" class:long={x.dur > 270000}>{x.dur ? fmtTime(x.dur) : ""}</span>
          </div>
        </div>
      {/each}
      {#if dropIndex === rows.length}<div class="drop-line"></div>{/if}
      {#if !rows.length}
        <div class="empty small">Drag songs here, press <b>+</b> on a song, or add style slots below.</div>
      {/if}
    </div>

    <div class="foot">
      <button class="small" onclick={slotMenu} title="Add a placeholder for a dance style; choose the song live"><Icon name="plus" size={12} /> Style slot</button>
      <span class="spacer"></span>
      <span class="dim small num">{number.size} songs · {fmtTime(totalMs)}{#if remainingMs && app.perform?.startedAt} · {fmtTime(remainingMs)} left{/if}</span>
    </div>
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .setpanel { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .noset { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 6px; padding: 30px 20px; color: var(--text-dim); }
  .noset p { margin: 0; }
  .recent { width: 100%; justify-content: space-between; }
  .head { display: flex; align-items: center; gap: 4px; padding: 8px 8px 4px 10px; }
  .name { font: 600 var(--fs-md)/1.2 var(--font-display); padding: 0 2px; justify-content: flex-start; flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .name-input { flex: 1; }
  .subhead { display: flex; gap: 6px; padding: 0 10px 8px; border-bottom: 1px solid var(--border); }
  .subhead select { flex: 1; min-width: 0; }
  .entries { flex: 1; overflow-y: auto; padding: 4px 0; outline: none; }
  .entry { display: grid; grid-template-columns: 24px 4px 1fr auto; align-items: center; gap: 6px; padding: 4px 8px 4px 4px; min-height: 40px; border-bottom: 1px solid rgba(255,255,255,0.03); }
  .entry:hover { background: var(--bg-hover); }
  .entry.sel { background: var(--bg-selected); }
  .entry.cur { background: color-mix(in srgb, var(--main) 16%, transparent); }
  .entry.next { box-shadow: inset 2px 0 0 var(--green); }
  .entry.played { opacity: 0.45; }
  .entry.skipped { opacity: 0.35; }
  .entry.skipped .t { text-decoration: line-through; }
  .n { text-align: right; color: var(--text-faint); font-size: var(--fs-xs); display: flex; justify-content: flex-end; }
  .cur .n { color: var(--main); }
  .stripe { width: 4px; align-self: stretch; border-radius: 2px; background: var(--c); }
  .body { min-width: 0; }
  .l1, .l2 { display: flex; align-items: center; gap: 6px; min-width: 0; white-space: nowrap; }
  .sname { font-size: var(--fs-xs); font-weight: 700; color: color-mix(in srgb, var(--c) 85%, white); flex: none; }
  .sname.unmapped { font-style: italic; opacity: 0.7; }
  .t { overflow: hidden; text-overflow: ellipsis; font-weight: 500; }
  .t.slot { color: var(--yellow); font-style: italic; font-weight: 400; }
  .l2 { font-size: var(--fs-sm); color: var(--text-dim); margin-top: 1px; }
  .a { overflow: hidden; text-overflow: ellipsis; }
  .tag { height: 15px; font-size: 10px; flex: none; }
  .note { color: var(--text-faint); overflow: hidden; text-overflow: ellipsis; }
  .right { display: flex; flex-direction: column; align-items: flex-end; font-size: var(--fs-sm); color: var(--text-dim); }
  .dur.long { color: var(--yellow); }
  .drop-line { height: 2px; background: var(--blue); margin: -1px 8px; border-radius: 1px; }
  .foot { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-top: 1px solid var(--border); }
  .spacer { flex: 1; }
  .small { font-size: var(--fs-sm); }
  .empty { padding: 20px; }
</style>

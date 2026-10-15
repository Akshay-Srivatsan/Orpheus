<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { addToSet, app, audition, spotifyVersions, confirmAction, fillSlot, startFillingSlot } from "../lib/state.svelte";
  import { customLabel, entryStyleKeys, NEUTRAL } from "../lib/styles";
  import { fmtBpm, fmtTime, playback, playedDurationMs } from "../lib/tempo";
  import type { Id, SetlistEntry } from "../lib/types";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";

  const set = $derived(app.current);
  const selSet = $derived(new Set(app.selectedEntries));
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let dropIndex = $state<number | null>(null);
  /** A style slot a song is being dragged onto (fills it instead of inserting). */
  let dropSlot = $state<Id | null>(null);
  let renaming = $state(false);
  let listEl: HTMLDivElement | undefined = $state();

  const rows = $derived(
    (set?.entries ?? []).map((e, i) => {
      const isPause = e.kind === "pause";
      const r = e.versionId && !isPause ? app.versionMap.get(e.versionId) : undefined;
      const pb = r ? playback(r.track, r.version, e, app.clubStyleOf) : null;
      const dur = isPause
        ? (e.pauseBefore ?? app.perform?.gapSeconds ?? 0) * 1000
        : r ? playedDurationMs(r.track, r.version, e, pb!.rate, { fadeSeconds: app.fadeSeconds, trimSilence: app.settings?.trimSilence }) : 0;
      const shown = app.shownFor(r?.version, e, pb?.target ?? null);
      const label = customLabel(r?.version, e) || shown.map((s) => s.short).join(" / ");
      // A song coming up that the room has already heard in this set.
      const repeat = e.status === "pending" && !!r && app.playedInSet.has(r.track.id);
      return { e, i, r, pb, dur, shown, label, repeat, isPause };
    }),
  );
  const totalMs = $derived.by(() => {
    const kept = rows.filter((x) => x.e.status !== "skipped");
    const songs = kept.filter((x) => !x.isPause).length;
    const gaps = Math.max(0, songs - 1) * (app.perform?.gapSeconds ?? 8) * 1000;
    return kept.reduce((a, x) => a + x.dur, 0) + gaps;
  });
  let number = $derived.by(() => {
    // Number songs, not skipped entries.
    const m = new Map<Id, number>();
    let n = 0;
    for (const x of rows) if (x.e.status !== "skipped" && !x.isPause) m.set(x.e.id, ++n);
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
    // Selecting an empty slot starts choosing its song.
    if (!entry.versionId) startFillingSlot(entry);
  }

  function preview(entry: SetlistEntry) {
    if (entry.versionId) audition(entry.versionId, entry.id);
  }

  /** Double-click: load onto the off-air deck (paused); on a slot, choose its song. */
  function choose(entry: SetlistEntry) {
    if (!entry.versionId) return startFillingSlot(entry);
    api.deckLoad(app.freeDeck, entry.versionId, { entryId: entry.id, play: false });
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
      ...(!entry.versionId ? [{ label: "Choose Song for This Slot…", action: () => startFillingSlot(entry) }, { separator: true } as MenuItem] : []),
      { label: "Play Now", disabled: !entry.versionId || isCurrent, action: () => app.run(api.playEntry(entry.id)) },
      { label: "Preview in Headphones (free deck)", disabled: !entry.versionId, action: () => preview(entry) },
      ...([0, 1] as const).map((d) => ({
        label: `Load onto Deck ${d === 0 ? "A" : "B"}`,
        disabled: !entry.versionId || (app.decks[d].onAir && app.decks[d].status === "playing"),
        action: () => entry.versionId && api.deckLoad(d, entry.versionId, { entryId: entry.id }),
      })),
      { label: "Move to Play Next", disabled: isCurrent, action: () => moveNext(ids) },
      { label: "Auto Mode Stops Before This Song", checked: !!entry.hold, action: () => api.updateEntry(entry.id, { hold: !entry.hold }) },
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
    const entry = set?.entries[i];
    const songs = ev.dataTransfer?.types.some((t) => t === "application/x-orpheus-versions" || t === "application/x-orpheus-spotify");
    const moving = ev.dataTransfer?.types.includes("application/x-orpheus-entries");
    // Songs dropped on the middle of an empty slot fill it.
    const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    const y = (ev.clientY - rect.top) / rect.height;
    if (entry && !entry.versionId && songs && !moving && y > 0.2 && y < 0.8) {
      dropSlot = entry.id;
      dropIndex = null;
      return;
    }
    dropSlot = null;
    dropIndex = y > 0.5 ? i + 1 : i;
  }

  async function drop(ev: DragEvent) {
    ev.preventDefault();
    const at = dropIndex ?? set?.entries.length ?? 0;
    dropIndex = null;
    if (dropSlot) {
      const slotId = dropSlot;
      dropSlot = null;
      const sp = JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-spotify") || "[]");
      const ids: Id[] = sp.length ? await spotifyVersions(sp, false) : JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
      if (ids[0]) await fillSlot(ids[0], slotId);
      return;
    }
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

  async function addPause() {
    if (!set) return;
    const ids = await app.run(api.insertEntries(set.id, [{ kind: "pause", pauseBefore: 30 }]));
    if (ids?.[0]) {
      app.selectedEntries = [ids[0]];
      app.inspectEntry = ids[0];
      app.inspectTrack = null;
      app.rightTab = "info";
    }
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
        number.get(x.e.id) ?? "", customLabel(x.r?.version, x.e) || x.shown.map((s) => s.name).join(" / "), x.r?.track.title ?? "(slot)", x.r?.track.artist ?? "", x.r?.version.name ?? "",
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
        {#if !app.profiles.some((p) => p.id === set.profileId)}
          <option value={set.profileId} disabled>Missing club “{set.profileId}” (import it in Settings → Club Profiles)</option>
        {/if}
      </select>
    </div>

    <div class="entries" bind:this={listEl} role="listbox" aria-multiselectable="true" tabindex="0" onkeydown={keydown}
      ondragover={(e) => { e.preventDefault(); if (!set.entries.length) dropIndex = 0; }}
      ondragleave={(e) => { if (e.currentTarget === e.target) { dropIndex = null; dropSlot = null; } }}
      ondrop={drop}>
      {#each rows as x (x.e.id)}
        {@const isCur = x.e.id === app.perform?.currentEntryId}
        {@const isNext = x.e.id === app.perform?.nextEntryId}
        {#if dropIndex === x.i}<div class="drop-line"></div>{/if}
        {#if x.e.hold}
          <div class="hold" title="Auto mode stops here: start the next song with Play Next">
            <Icon name="pause" size={10} /> auto mode stops here
            <button class="plain x" title="Remove the stop" onclick={() => api.updateEntry(x.e.id, { hold: false })}>×</button>
          </div>
        {/if}
        <div
          class="entry {x.e.status}"
          class:cur={isCur}
          class:next={isNext}
          class:sel={selSet.has(x.e.id)}
          class:repeat={x.repeat}
          class:filling={app.fillingSlot === x.e.id}
          class:dropslot={dropSlot === x.e.id}
          data-entry={x.e.id}
          style:--c={x.shown[0]?.color ?? NEUTRAL}
          role="option"
          aria-selected={selSet.has(x.e.id)}
          draggable="true"
          ondragstart={(ev) => dragStart(ev, x.e)}
          ondragover={(ev) => dragOver(ev, x.i)}
          onclick={(ev) => select(ev, x.e)}
          ondblclick={() => choose(x.e)}
          oncontextmenu={(ev) => openMenu(ev, x.e)}
          onkeydown={() => {}}
          tabindex="-1"
        >
          <span class="num n">{#if isCur && app.decks[app.liveDeck].status === "playing"}<Icon name="play" size={10} />{:else}{number.get(x.e.id) ?? ""}{/if}</span>
          <span class="stripe"></span>
          <div class="body">
            <div class="l1">
              <span class="sname" class:unmapped={!customLabel(x.r?.version, x.e) && x.shown.some((s) => s.unmapped)}
                title={x.shown.map((s) => (s.unmapped ? `${s.name}: not a ${app.profile?.name} style` : s.club && s.name !== app.styleName(s.key) ? `${s.name} (your ${app.styleName(s.key)})` : s.name)).join("\n")}
                >{x.isPause ? "" : x.label || "—"}</span>
              {#if x.isPause}
                <span class="t pausename"><Icon name="pause" size={11} /> {x.e.label || "Pause"}</span>
              {:else if x.r}
                <span class="t" title={x.r.track.title}>{x.r.track.title}</span>
              {:else}
                <span class="t slot">{x.shown.length ? `${x.shown.map((s) => s.name).join(" / ")} slot` : "Empty slot"}</span>
              {/if}
            </div>
            <div class="l2">
              {#if x.e.announcement}<span class="say" title="Announced before it: {x.e.announcement}"><Icon name="speaker" size={10} /> {x.e.announcement}</span>
              {:else if x.e.announcement === ""}<span class="say faint">no announcement</span>{/if}
              {#if !x.r && !x.isPause}
                <button class="small pick" class:on={app.fillingSlot === x.e.id}
                  onclick={(ev) => { ev.stopPropagation(); if (app.fillingSlot === x.e.id) app.fillingSlot = null; else startFillingSlot(x.e); }}>
                  {app.fillingSlot === x.e.id ? "Choosing… (Cancel)" : "Choose Song…"}
                </button>
                {#if app.fillingSlot === x.e.id}<span class="hint">or drag a song here</span>{/if}
              {/if}
              {#if x.r}<span class="a">{x.r.track.artist}{#if app.versionLabel(x.r.version, x.r.track)}{" · "}{x.r.version.name}{/if}</span>{/if}
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
      <button class="small" onclick={addPause} title="Add a pause without a song: for a break, an extra pause, an announcement or a stop for auto mode"><Icon name="pause" size={12} /> Pause</button>
      <span class="spacer"></span>
      <span class="dim small num">{number.size} songs · {fmtTime(totalMs)}{#if remainingMs && app.perform?.startedAt}{" · "}{fmtTime(remainingMs)} left{/if}</span>
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
  /* Already played earlier in this set. */
  .entry.repeat .t, .entry.repeat .a { color: var(--text-faint); }
  .entry.filling { box-shadow: inset 0 0 0 1px var(--yellow); }
  .entry.dropslot { background: color-mix(in srgb, var(--blue) 25%, transparent); }
  .pick { height: 20px; padding: 0 8px; }
  .pick.on { border-color: var(--yellow); color: var(--yellow); }
  .hint { color: var(--text-faint); font-size: var(--fs-xs); }
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
  .pausename { color: var(--text-dim); font-style: italic; display: inline-flex; align-items: center; gap: 4px; }
  .say { color: var(--text-dim); overflow: hidden; text-overflow: ellipsis; display: inline-flex; align-items: center; gap: 3px; }
  .hold { display: flex; align-items: center; gap: 5px; margin: 2px 8px 2px 32px; padding: 1px 6px; font-size: var(--fs-xs);
    color: var(--yellow); border-top: 1px dashed color-mix(in srgb, var(--yellow) 60%, transparent); }
  .hold .x { padding: 0 3px; height: 14px; color: inherit; min-width: 0; }
  .drop-line { height: 2px; background: var(--blue); margin: -1px 8px; border-radius: 1px; }
  .foot { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-top: 1px solid var(--border); }
  .spacer { flex: 1; }
  .small { font-size: var(--fs-sm); }
  .empty { padding: 20px; }
</style>

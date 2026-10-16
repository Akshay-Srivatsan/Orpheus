<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { addToSet, app, audition, spotifyVersions, confirmAction, fillSlot, startFillingSlot } from "../lib/state.svelte";
  import { customLabel, entryStyleKeys, NEUTRAL } from "../lib/styles";
  import { assess, checkSet, type SetItem } from "../lib/clubs";
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
      const shown = app.shownFor(r?.track, r?.version, e);
      // The DJ's choice of club style that the song doesn't fit.
      const warnings = [...new Set(shown.flatMap((s) => s.problems))];
      if (r?.rec.missing) warnings.unshift(r.rec.removed || !r.rec.sources.length ? "Its audio was removed from the library (Library → Files adds it again)" : "Its file isn't on this computer");
      // In a club's set, a song that is none of its styles.
      const noStyle = app.hasClub && !!r && !shown.length && !customLabel(r.version, e);
      // Many automatic matches: the first two, and how many more (all in the tooltip).
      const many = shown.length > 2 && shown[0]?.auto;
      const label = customLabel(r?.version, e) || (many ? `${shown.slice(0, 2).map((s) => s.short).join(" / ")} +${shown.length - 2}` : shown.map((s) => s.short).join(" / "));
      // A song coming up that the room has already heard in this set.
      const repeat = e.status === "pending" && !!r && app.playedInSet.has(r.track.id);
      // Its recording was already played as another song (e.g. for another line dance).
      const heardAs = e.status === "pending" && r && !repeat ? app.playedRecordingsInSet.get(r.rec.id) : undefined;
      return { e, i, r, pb, dur, shown, label, repeat, heardAs, isPause, warnings, noStyle, many };
    }),
  );
  const clubValue = $derived(set?.club ? `${set.club.id}/${set.club.version}` : "");
  const latestOfClub = $derived(set?.club ? app.clubs.find((c) => c.id === set!.club!.id)?.versions[0]?.id : undefined);
  const isLatest = $derived(!set?.club || set.club.version === latestOfClub);

  /** The set against its club's guidelines (songs and slots, in order; skipped ones left out). */
  const check = $derived.by(() => {
    const club = app.club;
    if (!club || !set) return null;
    const gap = app.perform?.gapSeconds ?? 0;
    const items: SetItem[] = rows
      .filter((x) => !x.isPause && x.e.status !== "skipped")
      .map((x) => ({
        entryId: x.e.id,
        title: x.r?.track.title ?? `${x.shown.map((s) => s.name).join(" / ") || "Empty"} slot`,
        ids: x.shown.map((s) => s.key),
        fits: Object.fromEntries(x.shown.map((s) => [s.key, { id: s.key, ok: !s.problems.length, fit: s.fit ?? "unchecked", rate: s.rate, bpm: null, problems: s.problems }])),
        lengthSecs: x.r ? x.dur / 1000 : null,
        pauseSecs: (x.e.pauseBefore ?? gap) + rows.filter((p) => p.isPause && p.i === x.i - 1).reduce((a, p) => a + p.dur / 1000, 0),
      }));
    // A set whose songs have all been played, or a planned set with no slots left, is treated as finished.
    const complete = set.mode === "preset" ? items.every((it) => rows.find((r) => r.e.id === it.entryId)?.r) : rows.every((r) => r.e.status !== "pending");
    const result = checkSet(club, items, { complete });
    // Songs that match several styles with nothing to choose between them.
    for (const x of rows) {
      if (x.many && x.e.status !== "skipped") {
        result.issues.push({ level: "note", text: `${x.r?.track.title}: matches ${x.shown.length} styles (${x.shown.map((s) => s.short).join(", ")}); choose with right-click → Show as, or map your styles (Settings → Styles & Tags)`, entries: [x.e.id] });
      }
    }
    return result;
  });
  const warnCount = $derived(check?.issues.filter((i) => i.level === "warn").length ?? 0);
  let showCheck = $state(false);

  function setClub(v: string) {
    if (!set) return;
    const [id, version] = v.split("/");
    app.run(api.updateSetlist(set.id, { club: id && version ? { id, version } : null }));
  }

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
      ...Object.entries(app.club?.tag ?? {}).map(([id, t]) => ({
        label: t.name, checked: (entry.tags ?? []).includes(id), action: () => toggleTag(entry, id),
      })),
      ...styleItems(entry),
      { separator: true },
      { label: "Edit Overrides for This Set…", action: () => { app.inspectEntry = entry.id; app.rightTab = "info"; } },
      { label: "Remove from Set", danger: true, disabled: isCurrent, action: () => remove(ids) },
    ];
    menu = { x: ev.clientX, y: ev.clientY, items };
  }

  /** Choose which of a song's styles this set shows. In a club's set: any of
   * the club's styles (those the song matches first; others with a warning). */
  function styleItems(entry: SetlistEntry): MenuItem[] {
    const club = app.club;
    if (club) {
      const r = entry.versionId ? app.versionMap.get(entry.versionId) : undefined;
      const facts = app.factsFor(r?.track, r?.version, entry);
      const shown = app.shownFor(r?.track, r?.version, entry).map((s) => s.key);
      const tc = app.clubCtx!.tempoChange;
      const ok = (id: string) => !facts || assess(id, club.style[id], facts, tc).ok;
      const ids = Object.keys(club.style);
      const set = (next: string[]) => api.updateEntry(entry.id, { clubStyles: next.length ? next : null });
      const toggle = (id: string) => set(shown.includes(id) ? shown.filter((x) => x !== id) : [...shown, id]);
      const item = (id: string): MenuItem => ({
        label: ok(id) ? `Show as ${club.style[id].name}` : `Show as ${club.style[id].name} (doesn't fit)`,
        checked: shown.includes(id),
        action: () => toggle(id),
      });
      return [
        { separator: true },
        ...ids.filter(ok).map(item),
        ...(entry.clubStyles ? [{ label: "Show What It Matches", action: () => api.updateEntry(entry.id, { clubStyles: null }) }] : []),
        ...(ids.some((id) => !ok(id)) ? [{ separator: true } as MenuItem, ...ids.filter((id) => !ok(id)).map(item)] : []),
      ];
    }
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
      await app.run(api.insertEntries(set.id, await app.withClubTempo(vids.map((v) => ({ versionId: v })), set.club), at));
      return;
    }
    const versions: Id[] = JSON.parse(ev.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
    if (versions.length) await app.run(api.insertEntries(set.id, await app.withClubTempo(versions.map((v) => ({ versionId: v })), set.club), at));
  }

  /** A slot for one of your styles, or (in a club's set) one of the club's. */
  async function addSlot(style: string) {
    if (!set) return;
    await app.run(api.insertEntries(set.id, [app.club ? { clubStyles: [style] } : { styles: [style] }]));
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
    const club = app.club;
    const items: MenuItem[] = club
      ? Object.entries(club.style).map(([id, s]) => ({ label: s.name, action: () => addSlot(id) }))
      : app.myStyles.map((s) => ({ label: s.name, action: () => addSlot(s.key) }));
    if (!items.length) {
      items.push(club
        ? { label: `${club.name} has no styles (Settings → Clubs)`, action: () => (app.settingsOpen = "clubs") }
        : { label: "Add dance styles in Settings → Dance Styles", action: () => (app.settingsOpen = "styles") });
    }
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
      { label: "Export Set…", action: exportSet },
      { label: "Export as CSV…", action: exportCsv },
      { label: "Print Setlist…", action: () => window.print() },
      { separator: true },
      { label: "Close Set", action: () => api.performOpen(null) },
      { label: "Delete Set…", danger: true, action: () => confirmAction(`Delete “${s.name}”?`).then((ok) => { if (ok) api.deleteSetlist(s.id); }) },
    ];
    menu = { x: ev.clientX, y: ev.clientY, items };
  }

  /** A set file another library can open: its songs link to the same audio there (any encoding, or Spotify). */
  async function exportSet() {
    if (!set) return;
    const path = await save({ defaultPath: `${set.name}.orpheus-set.toml`, filters: [{ name: "Orpheus set", extensions: ["toml"] }] });
    if (path && (await app.run(api.exportSetlist(set.id, path))) !== undefined) app.toast(`Exported “${set.name}”.`, "info");
  }

  async function exportCsv() {
    if (!set) return;
    const esc = (v: unknown) => `"${String(v ?? "").replace(/"/g, '""')}"`;
    const lines = [["#", "Style", "Title", "Artist", "Version", "BPM", "Length", "Tags", "Note", "Status"].map(esc).join(",")];
    for (const x of rows) {
      lines.push([
        number.get(x.e.id) ?? "", customLabel(x.r?.version, x.e) || x.shown.map((s) => s.name).join(" / "), x.r?.track.title ?? "(slot)", x.r?.track.artist ?? "", x.r?.version.name ?? "",
        x.pb?.target ? fmtBpm(x.pb.target) : "", x.dur ? fmtTime(x.dur) : "",
        (x.e.tags ?? []).map((t) => app.club?.tag[t]?.name ?? t).join("; "), x.e.note ?? "", x.e.status,
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
      <select title="Club: whose guidelines (styles, tempos, what a set should contain) this set follows" value={clubValue}
        onchange={(e) => setClub(e.currentTarget.value)}>
        <option value="">No club</option>
        {#each app.clubs as c}
          <option value="{c.id}/{c.versions[0]?.id}">{c.name}</option>
        {/each}
        {#if set.club && !isLatest}<option value={clubValue}>{app.clubLabel(set.club)}</option>{/if}
      </select>
      {#if set.club && !isLatest && latestOfClub}
        <button class="plain small newer" title="This set follows an older version of {app.clubLabel({ id: set.club.id, version: latestOfClub })}'s guidelines"
          onclick={() => setClub(`${set.club!.id}/${latestOfClub}`)}>Use newer guidelines</button>
      {/if}
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
              <span class="sname" class:unmapped={x.noStyle}
                title={x.noStyle ? `Not one of ${app.club?.name}'s styles (right-click: Show as…)` : x.shown.map((s) => s.name).join("\n")}
                >{x.isPause ? "" : x.label || "—"}</span>
              {#if x.warnings.length}
                <span class="warn-mark" title={x.warnings.join("\n")}><Icon name="warning" size={11} /></span>
              {/if}
              {#if x.heardAs}
                <span class="heard" title="This recording was already played in this set (as “{x.heardAs.title}”)"><Icon name="history" size={11} /></span>
              {/if}
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
                {@const t = app.club?.tag[tag]}
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

    {#if check && showCheck}
      <div class="check">
        {#if check.tallies.length}
          <div class="tallies">
            {#each check.tallies as t (t.key)}
              <span class="tally" class:bad={!t.ok} title="{app.club?.name} expects {t.want}">{t.name} <b class="num">{t.have}</b><span class="faint"> / {t.want}</span></span>
            {/each}
          </div>
        {/if}
        {#each check.issues as i, n (n)}
          <button class="plain issue {i.level}" disabled={!i.entries.length} onclick={() => (app.selectedEntries = i.entries)}>
            <Icon name={i.level === "warn" ? "warning" : "info"} size={11} /> {i.text}
          </button>
        {:else}
          <div class="ok small"><Icon name="check" size={12} /> Follows {app.club?.name}'s guidelines.</div>
        {/each}
      </div>
    {/if}
    <div class="foot">
      <button class="small" onclick={slotMenu} title="Add a placeholder for a dance style; choose the song live"><Icon name="plus" size={12} /> Style slot</button>
      <button class="small" onclick={addPause} title="Add a pause without a song: for a break, an extra pause, an announcement or a stop for auto mode"><Icon name="pause" size={12} /> Pause</button>
      <span class="spacer"></span>
      {#if check}
        <button class="small" class:warned={warnCount > 0} class:on={showCheck} onclick={() => (showCheck = !showCheck)}
          title="How this set follows {app.club?.name}'s guidelines">
          <Icon name={warnCount ? "warning" : "check"} size={12} /> {warnCount ? `${warnCount} to check` : "Guidelines"}
        </button>
      {/if}
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
  /* min-height 0: the list scrolls instead of pushing the buttons below out of view. */
  .entries { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; outline: none; }
  .entry { display: grid; grid-template-columns: 24px 4px 1fr auto; align-items: center; gap: 6px; padding: 4px 8px 4px 4px; min-height: 40px; border-bottom: 1px solid rgba(255,255,255,0.03); }
  .entry:hover { background: var(--bg-hover); }
  .entry.sel { background: var(--bg-selected); }
  .entry.cur { background: color-mix(in srgb, var(--main) 16%, transparent); }
  .entry.next { box-shadow: inset 2px 0 0 var(--green); }
  .entry.played { opacity: 0.45; }
  .entry.skipped { opacity: 0.35; }
  .entry.skipped .t { text-decoration: line-through; }
  .heard { color: var(--text-faint); display: inline-flex; flex: none; }
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
  .warn-mark { color: var(--yellow); display: inline-flex; flex: none; }
  .newer { color: var(--accent); white-space: nowrap; }
  button.warned { color: var(--yellow); border-color: color-mix(in srgb, var(--yellow) 45%, transparent); }
  .check { flex: none; max-height: 40%; overflow: auto; padding: 8px 10px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 4px; font-size: var(--fs-sm); }
  .tallies { display: flex; flex-wrap: wrap; gap: 4px 10px; margin-bottom: 4px; }
  .tally { white-space: nowrap; color: var(--text-dim); }
  .tally.bad b { color: var(--yellow); }
  .issue { text-align: left; justify-content: flex-start; gap: 6px; padding: 2px 4px; font-size: var(--fs-sm); color: var(--text-dim); white-space: normal; height: auto; }
  .issue.warn { color: var(--text); }
  .issue.warn :global(svg) { color: var(--yellow); flex: none; }
  .ok { color: var(--green); display: flex; gap: 4px; align-items: center; }
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
  .foot { flex: none; display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 8px; border-top: 1px solid var(--border); }
  .spacer { flex: 1; }
  .small { font-size: var(--fs-sm); }
  .empty { padding: 20px; }
</style>

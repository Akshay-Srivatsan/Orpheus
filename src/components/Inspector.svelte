<script lang="ts">
  // Info tab: the selected song, its versions and the recordings they play,
  // and — when a set entry is selected — that entry's per-setlist overrides.
  import { api } from "../lib/api";
  import { app, audition, confirmAction } from "../lib/state.svelte";
  import { fmtBpm, fmtTime, foldBpm, playback, playedDurationMs, rateToFit, resolveSettings, styleCenter } from "../lib/tempo";
  import { primaryVersion, recordingOf, type Id, type Overrides, type Recording, type RecordingPatch, type SetlistEntry, type SourceRef, type Track, type Version } from "../lib/types";
  import Icon from "./Icon.svelte";
  import StyleChips from "./StyleChips.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import { assess, matches } from "../lib/clubs";
  import { NEUTRAL } from "../lib/styles";
  import TimeField from "./TimeField.svelte";

  const entry = $derived(app.current?.entries.find((e) => e.id === app.inspectEntry));
  const track = $derived.by((): Track | undefined => {
    if (entry?.versionId) return app.versionMap.get(entry.versionId)?.track;
    return app.inspectTrack ? app.trackMap.get(app.inspectTrack) : undefined;
  });
  const entryVersion = $derived(entry?.versionId ? app.versionMap.get(entry.versionId)?.version : undefined);

  let taps = $state<number[]>([]);
  let linkInput = $state("");

  // ---------------------------------------------------------------- club styles for a set entry

  let clubMenu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  /** The club styles an entry can show: those it fits, and those shown now (in the club's order). */
  function clubChoices(t: Track | undefined, v: Version | undefined, e: SetlistEntry) {
    const club = app.club;
    const ctx = app.clubCtx;
    if (!club || !ctx) return { shown: [] as string[], fit: [] as string[], other: [] as string[] };
    const shown = app.shownFor(t, v, e).map((s) => s.key);
    const facts = app.factsFor(t, v, e);
    const fit = facts ? matches(club, facts, ctx.tempoChange).map((a) => a.id) : [];
    const ids = Object.keys(club.style);
    const cand = ids.filter((id) => fit.includes(id) || shown.includes(id));
    return { shown, fit: cand, other: ids.filter((id) => !cand.includes(id)) };
  }

  /**
   * Plays another version of the song in this entry (e.g. another edit of a
   * line dance). Settings tied to the old audio (tempo, start/end, fade) are
   * dropped, and the club's tempo rule is applied afresh.
   */
  async function switchVersion(e: SetlistEntry, versionId: Id) {
    const { targetBpm: _t, cueInMs: _i, cueOutMs: _o, fadeOutMs: _f, ...keep } = e.overrides ?? {};
    const [next] = await app.withClubTempo([{ versionId, styles: e.styles, clubStyles: e.clubStyles, overrides: keep }], app.current?.club);
    await app.run(api.updateEntry(e.id, { versionId, overrides: next.overrides ?? {} }));
  }

  /** How a version would play in an entry: tempo, length, and whether it fits the entry's styles. */
  function versionChoice(t: Track, v: Version, e: SetlistEntry): string {
    const at = { ...e, versionId: v.id, overrides: {} };
    const pb = playback(t, v, at, app.clubStyleOf);
    const shown = app.shownFor(t, v, at);
    const rec = recordingOf(t, v);
    const len = rec ? fmtTime(playedDurationMs(t, v, at, pb.rate, { fadeSeconds: app.fadeSeconds, trimSilence: app.settings?.trimSilence })) : "";
    const warn = shown.some((x) => x.problems.length) ? " ⚠" : "";
    return `${v.name} — ${fmtBpm(pb.target ?? rec?.bpm)} BPM, ${len}${warn}${v.hidden ? " (not in the library)" : ""}`;
  }

  function setClubStyles(entryId: Id, ids: string[]) {
    api.updateEntry(entryId, { clubStyles: ids.length ? ids : null });
  }

  function otherClubStyles(ev: MouseEvent, entryId: Id, shown: string[], other: string[], t: Track | undefined, v: Version | undefined, e: SetlistEntry) {
    const club = app.club!;
    const facts = app.factsFor(t, v, e);
    const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = other.map((id) => {
      const ok = !facts || assess(id, club.style[id], facts, app.clubCtx!.tempoChange).ok;
      return { label: ok ? club.style[id].name : `${club.style[id].name} (doesn't fit)`, action: () => setClubStyles(entryId, [...shown, id]) };
    });
    clubMenu = { x: r.left, y: r.bottom + 2, items };
  }

  /** Copies text for a club file (a song's fingerprint, hash or whole entry). */
  async function copy(text: string | null | undefined, what: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      app.toast(`${what} copied.`, "info");
    } catch (e) {
      app.toast(`Couldn't copy: ${e}`);
    }
  }

  function patchTrack(patch: Parameters<typeof api.updateTrack>[1]) {
    if (track) app.run(api.updateTrack(track.id, patch));
  }

  function patchRecording(rec: Recording, patch: RecordingPatch) {
    app.run(api.updateRecording(rec.id, patch));
  }

  function patchVersion(v: Version, patch: Partial<Version>) {
    app.run(api.updateVersion(v.id, patch));
  }

  function patchEntry(ov: Partial<Overrides>) {
    if (!entry) return;
    const next = { ...(entry.overrides ?? {}), ...ov };
    for (const k of Object.keys(next) as (keyof Overrides)[]) if (next[k] == null) delete next[k];
    app.run(api.updateEntry(entry.id, { overrides: next }));
  }

  function tap(rec: Recording) {
    const now = performance.now();
    const recent = [...taps.filter((t) => now - t < 2500), now].slice(-9);
    taps = recent;
    if (recent.length >= 4) {
      const iv = (recent[recent.length - 1] - recent[0]) / (recent.length - 1);
      const bpm = Math.round((60000 / iv) * 10) / 10;
      patchRecording(rec, { bpm, bpmSource: "tap" });
    }
  }

  function scaleBpm(rec: Recording, f: number) {
    if (rec.bpm) patchRecording(rec, { bpm: Math.round(rec.bpm * f * 10) / 10, bpmSource: "user" });
  }

  /** Splitting a source off a recording (undoes "Same Recording"): what then plays it. */
  function splitMenu(ev: MouseEvent, rec: Recording, src: SourceRef) {
    const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    const playing = (track?.versions ?? []).filter((v) => v.recording === rec.id);
    const split = (versions: Id[]) => app.run(api.splitSource(rec.id, src, versions));
    const items: MenuItem[] = [
      { label: "Split Off as a Song of Its Own", action: () => app.run(api.splitSource(rec.id, src, [], true)) },
      ...playing.map((v) => ({ label: `Split Off for Version “${v.name}”`, action: () => split([v.id]) })),
    ];
    clubMenu = { x: r.left, y: r.bottom + 2, items };
  }

  /** Version id → its audio details shown (default: when the song has one version). */
  let openAudio = $state<Record<Id, boolean>>({});
  const audioOpen = (v: Version) => openAudio[v.id] ?? (track?.versions.length ?? 0) === 1;

  /** "FLAC" / "Spotify" chips for an audio's files and Spotify link. */
  function sourceChips(rec: Recording): { label: string; title: string }[] {
    const ext = (p: string) => p.match(/\.([a-z0-9]+)$/i)?.[1]?.toUpperCase() ?? "file";
    return rec.sources.map((x) => {
      if (x.kind === "spotify") return { label: "Spotify", title: x.key };
      const p = rec.filePaths?.[x.key]?.[0];
      return p ? { label: ext(p), title: p } : { label: "file missing", title: "Not found on this computer: add its folder in Settings → Library" };
    });
  }

  /** Other songs playing a recording. */
  const otherSongs = (rec: Recording) => rec.songs.filter((id) => id !== track?.id).map((id) => app.trackMap.get(id)).filter(Boolean) as Track[];

  async function newVersion() {
    if (!track) return;
    const style = app.filterStyle;
    const v = await app.run(api.createVersion(track.id, { name: style ? app.styleName(style) : "New version", styles: style ? [style] : [] }));
    if (v) app.selected = [v.id];
  }

  async function duplicateVersion(v: Version) {
    if (!track) return;
    const { id: _id, ...rest } = $state.snapshot(v) as Version;
    await app.run(api.createVersion(track.id, { ...rest, name: `${v.name} (copy)` }));
  }

  function meterName(n: number): string {
    return n === 3 || n === 6 ? "triple" : "duple";
  }

  function fitToStyle(v: Version) {
    const s = app.clubStyleOf(track, v, null);
    const bpm = recordingOf(track, v)?.bpm;
    if (!bpm || !s) return;
    const c = styleCenter(s);
    if (!c) return;
    const base = foldBpm(bpm, c);
    const rate = rateToFit(base, s);
    patchVersion(v, { targetBpm: rate === 1 ? null : Math.round(base * rate * 10) / 10 });
  }

  /** Uses the preview deck's position, if it has this version loaded. */
  function previewPos(v: Version): number | null {
    const d = app.decks.find((x) => x.versionId === v.id && x.status !== "empty");
    return d ? d.positionMs : null;
  }

</script>

{#snippet audioDetails(rec: Recording)}
  {@const preferred = rec.preferredSource ?? ""}
  {@const job = app.stems?.jobs[rec.id]}
    <div class="grid">
      <span class="lbl">BPM</span>
      <span class="row">
        <input type="number" step="0.1" value={rec.bpm ?? ""} onchange={(e) => patchRecording(rec, { bpm: e.currentTarget.value ? +e.currentTarget.value : null })} />
        <button class="small" onclick={() => tap(rec)} title="Tap along to the beat (4+ taps)">Tap</button>
        <span class="mult">
          <button class="plain small" onclick={() => scaleBpm(rec, 0.5)} title="Half">½</button>
          <button class="plain small" onclick={() => scaleBpm(rec, 2 / 3)} title="×⅔ (triple meter)">⅔</button>
          <button class="plain small" onclick={() => scaleBpm(rec, 1.5)} title="×1.5 (triple meter)">1½</button>
          <button class="plain small" onclick={() => scaleBpm(rec, 2)} title="Double">2×</button>
        </span>
      </span>
      <span></span>
      <span class="row">
        {#if rec.bpmSource !== "detected"}
          <button class="plain small" title="Use the BPM found by analysis"
            onclick={() => app.run(api.resetBpm(rec.id, "detected"))}>
            ↺ Analysed{rec.detectedBpm != null ? ` (${fmtBpm(rec.detectedBpm)})` : ""}
          </button>
        {/if}
        {#if rec.bpmSource !== "tag" && (rec.tagBpm != null || rec.files.length)}
          <button class="plain small" title="Use the BPM stored in the file's tags"
            onclick={() => app.run(api.resetBpm(rec.id, "tag"))}>
            ↺ File tag{rec.tagBpm != null ? ` (${fmtBpm(rec.tagBpm)})` : ""}
          </button>
        {/if}
      </span>
      <span></span>
      <span class="faint small">{rec.bpmSource ? `from ${({ detected: "analysis", tag: "file tag", tap: "tapping", user: "you" } as Record<string, string>)[rec.bpmSource] ?? rec.bpmSource}` : rec.analyzed ? "not detected" : "analysing…"}{#if rec.loudnessDb != null}{" · "}{rec.loudnessDb.toFixed(1)} LUFS{/if} · {fmtTime(rec.durationMs)}</span>
      <span class="lbl" title="Beats per bar. Detected from the audio, on the beat grid of the BPM above (fix the BPM first if it's off)">Meter</span>
      <span class="row">
        <select value={rec.meterSource === "user" ? String(rec.beatsPerBar ?? "") : "auto"}
          onchange={(e) => { const v = e.currentTarget.value; patchRecording(rec, { beatsPerBar: v === "auto" ? null : +v }); }}>
          <option value="auto">Detected{rec.detectedMeter ? `: ${meterName(rec.detectedMeter)}` : rec.analyzed ? ": unclear" : ""}</option>
          <option value="3">Triple (3/4, 6/8)</option>
          <option value="4">Duple (2/4, 4/4)</option>
        </select>
      </span>
      <span class="lbl">Gain</span>
      <span class="row"><input type="number" step="0.5" value={rec.gainDb ?? 0} onchange={(e) => patchRecording(rec, { gainDb: +e.currentTarget.value })} /> dB</span>
    </div>
    <div class="section-title sub">Files &amp; Spotify</div>
    {#each rec.sources as s}
      <div class="source">
        <Icon name={s.kind === "spotify" ? "spotify" : "music"} size={13} />
        {#if s.kind === "file"}
          {@const paths = rec.filePaths?.[s.key] ?? []}
          <div class="src-body">
            {#if paths.length}
              {#each paths as p}<div class="path" title={p}>{p.split(/[/\\]/).pop()}</div>{/each}
            {:else}
              <div class="warn small">File not found on this computer (content id {s.key.slice(0, 10)}…). Add its folder to your library to relink it.</div>
            {/if}
          </div>
        {:else}
          <div class="src-body path" title={s.key}>{s.key}</div>
        {/if}
        {#if rec.sources.length > 1}
          <button class="plain small" title="This is different audio: give it its own (undoes Combine as the same recording)"
            onclick={(ev) => splitMenu(ev, rec, s)}>Split off…</button>
          <button class="plain icon" title="Remove source" onclick={() => app.run(api.removeSource(rec.id, s))}><Icon name="x" size={12} /></button>
        {/if}
      </div>
    {/each}
    {#if !rec.sources.some((s) => s.kind === "spotify")}
      <div class="row">
        <input type="text" placeholder="Link a Spotify track (URL or URI) of this audio" bind:value={linkInput} style="flex:1" />
        <button class="small" disabled={!linkInput.trim()} onclick={async () => { if (await app.run(api.linkSpotify(rec.id, linkInput))) linkInput = ""; }}>
          <Icon name="link" size={12} /> Link
        </button>
      </div>
    {/if}
    {#if rec.sources.some((x) => x.kind === "spotify") && rec.sources.some((x) => x.kind === "file")}
      <div class="row small">
        Play from
        <div class="segmented">
          <button class:on={preferred !== "spotify"} onclick={() => patchRecording(rec, { preferredSource: null })}>File</button>
          <button class:on={preferred === "spotify"} onclick={() => patchRecording(rec, { preferredSource: "spotify" })}>Spotify</button>
        </div>
      </div>
    {/if}
    {#if rec.files.length}
      <div class="row small">
        {#if app.stemsReady.has(rec.id)}
          Stems ready
          <button class="plain small" onclick={() => app.run(api.stemsDelete(rec.id))}>Delete stems</button>
        {:else if job && job.state !== "failed"}
          <span class="num">Preparing stems… {job.state === "running" ? `${Math.round(job.percent)}%` : "(queued)"}</span>
        {:else}
          <button class="small" onclick={() => app.run(api.stemsPrepare([rec.id]))}>Prepare Stems</button>
          {#if job?.state === "failed"}<span class="warn">{job.message}</span>{/if}
        {/if}
      </div>
    {/if}
    <div class="row small faint">
      {rec.analyzed ? "Analysed" : "Not analysed yet"}
      <button class="plain small" onclick={() => api.analyze([rec.id])}><Icon name="reload" size={11} /> Re-analyse</button>
      <button class="plain small" title="A new song (in your library) playing this audio, e.g. for another line dance"
        onclick={() => app.run(api.songFromRecording(rec.id)).then((t) => { if (t) app.inspectTrack = t.id; })}>Make a Song from It</button>
    </div>
    <div class="row small" title="To suggest it in a club's file: the fingerprint recognises this audio in any format; the hash, exactly this file">
      <span class="faint">For a club file:</span>
      <button class="plain small" disabled={!rec.fingerprint} title={rec.fingerprint ? "" : "Analyse it first"}
        onclick={() => copy(rec.fingerprint, "Fingerprint")}>Fingerprint</button>
      {#if rec.sources.find((x) => x.kind === "file")}
        <button class="plain small" onclick={() => copy(rec.sources.find((x) => x.kind === "file")?.key, "File hash")}>Hash</button>
      {/if}
      {#if app.songMatches[rec.id]?.length && app.club}
        <span class="ok" title="{app.club.name} suggests this recording">★ {app.songMatches[rec.id].map((id) => app.club?.style[id]?.name ?? id).join(", ")}</span>
      {/if}
    </div>
{/snippet}

<div class="insp">
  {#if !track && !entry}
    <div class="empty">Select a song to see and edit its details.</div>
  {:else}
    {#if entry}
      {@const r = entryVersion && track ? playback(track, entryVersion, entry, app.clubStyleOf) : null}
      {@const shownKeys = entry.styles ?? entryVersion?.styles ?? []}
      {@const isPause = entry.kind === "pause"}
      {@const ann = app.settings?.announce}
      {@const speaks = entry.announcement != null ? entry.announcement !== "" : !!ann?.enabled && !isPause}
      {@const defBefore = speaks || ann?.enabled ? (ann?.pauseBeforeSecs ?? 3) : (app.perform?.gapSeconds ?? 8)}
      <section class="box entrybox">
        <div class="section-title">{isPause ? "Pause in this set" : "In this set"}</div>
        <div class="grid">
          {#if !isPause && track && entryVersion && track.versions.length > 1}
            <span class="lbl" title="Which version of the song plays (e.g. which edit of a line dance)">Version</span>
            <select value={entryVersion.id} onchange={(e) => switchVersion(entry, e.currentTarget.value)}>
              {#each track.versions as v (v.id)}<option value={v.id}>{versionChoice(track, v, entry)}</option>{/each}
            </select>
          {/if}
          {#if isPause}
            <span class="lbl">Name</span>
            <input type="text" value={entry.label ?? ""} placeholder="Pause (e.g. Break, Lesson)"
              onchange={(e) => api.updateEntry(entry.id, { label: e.currentTarget.value.trim() || null })} />
          {:else}
          <span class="lbl">{entryVersion ? "Show as" : "Slot"}</span>
          {#if app.club}
            {@const cc = clubChoices(track, entryVersion, entry)}
            <span class="clubchips">
              {#each cc.fit as id (id)}
                {@const on = cc.shown.includes(id)}
                <button class="badge chip" class:off={!on} style:--c={app.club.style[id].color || NEUTRAL}
                  title={on ? "Shown; click to hide" : "Fits; click to show it too"}
                  onclick={() => setClubStyles(entry.id, on ? cc.shown.filter((x) => x !== id) : [...cc.shown, id])}>{app.club.style[id].name}</button>
              {/each}
              {#if cc.other.length}
                <button class="small add" title="Any of {app.club.name}'s styles, even one this song doesn't fit"
                  onclick={(ev) => otherClubStyles(ev, entry.id, cc.shown, cc.other, track, entryVersion, entry)}>+</button>
              {/if}
              {#if entry.clubStyles}
                <button class="plain small link" title="Show what it fits (and your mapping picks)" onclick={() => setClubStyles(entry.id, [])}>Automatic</button>
              {/if}
            </span>
          {:else if entryVersion && (entryVersion.styles?.length ?? 0) > 0}
            <StyleChips value={shownKeys} choices={entryVersion.styles} bpm={r?.target ?? null}
              onchange={(keys) => api.updateEntry(entry.id, { styles: keys.length === entryVersion.styles?.length || !keys.length ? null : keys })} />
          {:else}
            <StyleChips value={shownKeys} bpm={r?.target ?? null}
              onchange={(keys) => api.updateEntry(entry.id, { styles: keys.length ? keys : null })} />
          {/if}
          <span class="lbl" title="Text shown instead of the style names (setlist and projector)">Label</span>
          <input type="text" value={entry.label ?? ""}
            placeholder={app.shownFor(track, entryVersion, entry).map((s) => s.name).join(" / ") || "e.g. Mixer"}
            onchange={(e) => api.updateEntry(entry.id, { label: e.currentTarget.value.trim() || null })} />
          <span class="lbl">Tempo</span>
          <span class="row">
            <input type="number" step="0.5" placeholder={entryVersion?.targetBpm ? fmtBpm(entryVersion.targetBpm) : "orig."}
              value={entry.overrides?.targetBpm ?? ""}
              onchange={(e) => patchEntry({ targetBpm: e.currentTarget.value ? +e.currentTarget.value : null })} /> BPM
            {#if r?.target}<span class="faint num">→ {fmtBpm(r.target)} ({((r.rate - 1) * 100).toFixed(1)}%)</span>{/if}
          </span>
          <span class="lbl">Start / end</span>
          <span class="row">
            <TimeField value={entry.overrides?.cueInMs} placeholder="inherit" onchange={(v) => patchEntry({ cueInMs: v })} />
            <TimeField value={entry.overrides?.cueOutMs} placeholder="inherit" onchange={(v) => patchEntry({ cueOutMs: v })} />
          </span>
          <span class="lbl">Fade at</span>
          <TimeField value={entry.overrides?.fadeOutMs} placeholder="inherit" onchange={(v) => patchEntry({ fadeOutMs: v })} />
          <span class="lbl">Gain</span>
          <span class="row"><input type="number" step="0.5" placeholder="inherit" value={entry.overrides?.gainDb ?? ""}
            onchange={(e) => patchEntry({ gainDb: e.currentTarget.value ? +e.currentTarget.value : null })} /> dB</span>
          {/if}
          <span class="lbl">Note</span>
          <input type="text" value={entry.note ?? ""} placeholder="e.g. announce the jam"
            onchange={(e) => api.updateEntry(entry.id, { note: e.currentTarget.value })} />
          <span class="lbl">Tags</span>
          <span class="tags">
            {#each Object.entries(app.club?.tag ?? {}) as [id, t]}
              {@const on = (entry.tags ?? []).includes(id)}
              <button class="badge tagbtn" class:off={!on} style:--c={t.color ?? "#999"} title={t.note ?? ""}
                onclick={() => api.updateEntry(entry.id, { tags: on ? (entry.tags ?? []).filter((x) => x !== id) : [...(entry.tags ?? []), id] })}>
                {t.name}
              </button>
            {/each}
          </span>
        </div>
        <div class="section-title sub">Auto mode</div>
        <div class="grid">
          <span class="lbl" title={isPause ? "How long the pause lasts" : "Pause before this song (before its announcement)"}>{isPause ? "Length" : "Pause before"}</span>
          <span class="row"><input type="number" min="0" step="1" placeholder={String(defBefore)} value={entry.pauseBefore ?? ""}
            onchange={(e) => api.updateEntry(entry.id, { pauseBefore: e.currentTarget.value === "" ? null : +e.currentTarget.value })} /> s</span>
          <span class="lbl">Announce</span>
          <span class="col">
            <select value={isPause ? (entry.announcement ? "custom" : "none") : entry.announcement == null ? "default" : entry.announcement === "" ? "none" : "custom"}
              onchange={(e) => {
                const v = e.currentTarget.value;
                api.updateEntry(entry.id, { announcement: v === "default" ? null : v === "none" ? "" : (entry.announcement || (isPause ? "We'll take a short break." : "The next dance is a {dance}.")) });
              }}>
              {#if !isPause}<option value="default">{ann?.enabled ? "A random sentence" : "Nothing (announcements off)"}</option>{/if}
              <option value="custom">This text:</option>
              <option value="none">{isPause ? "Nothing" : "Nothing for this song"}</option>
            </select>
            {#if entry.announcement}
              <input type="text" value={entry.announcement} onchange={(e) => api.updateEntry(entry.id, { announcement: e.currentTarget.value })} />
              <span class="faint small">{"{dance}"} is replaced by the dance's name.</span>
            {/if}
          </span>
          {#if speaks}
            <span class="lbl" title="Pause between the announcement and the start">Then pause</span>
            <span class="row"><input type="number" min="0" step="1" placeholder={String(ann?.pauseAfterSecs ?? 4)} value={entry.pauseAfter ?? ""}
              onchange={(e) => api.updateEntry(entry.id, { pauseAfter: e.currentTarget.value === "" ? null : +e.currentTarget.value })} /> s</span>
          {/if}
          <span class="lbl"></span>
          <label class="small"><input type="checkbox" checked={!!entry.hold} onchange={(e) => api.updateEntry(entry.id, { hold: e.currentTarget.checked })} />
            Stop before {isPause ? "this pause" : "this song"} (start it with Play Next)</label>
        </div>
        {#if entry.overrides && Object.keys(entry.overrides).length}
          <button class="plain small" onclick={() => api.updateEntry(entry.id, { overrides: {} })}>Clear overrides</button>
        {/if}
      </section>
    {/if}

    {#if track}
      {#if track.inLibrary === false}
        <section class="box setonly">
          <span>Only in setlists — not in your library.</span>
          <button class="small" onclick={() => app.run(api.setInLibrary(track.id, true))}><Icon name="plus" size={12} /> Add to Library</button>
        </section>
      {/if}
      <section class="box">
        <div class="section-title">Song</div>
        <input class="big" type="text" value={track.title} onchange={(e) => patchTrack({ title: e.currentTarget.value })} aria-label="Title" />
        <input type="text" value={track.artist} placeholder="Artist" onchange={(e) => patchTrack({ artist: e.currentTarget.value })} />
        <input type="text" value={track.album} placeholder="Album" onchange={(e) => patchTrack({ album: e.currentTarget.value })} />
        <textarea rows="2" placeholder="Notes" value={track.notes ?? ""} onchange={(e) => patchTrack({ notes: e.currentTarget.value })}></textarea>
        <div class="row small faint">{track.versions.length} version{track.versions.length === 1 ? "" : "s"} · {track.recordings.length} audio · played {track.playCount}×</div>
      </section>

      <section class="box">
        <div class="section-title row">Versions <button class="plain small" onclick={newVersion}><Icon name="plus" size={12} /> New</button></div>
        {#each track.versions as v (v.id)}
          {@const pb = playback(track, v, null, app.clubStyleOf)}
          {@const vrec = recordingOf(track, v)}
          <div class="version" class:hl={app.selected.includes(v.id) || entry?.versionId === v.id} style:--c={app.styleColor(v.styles?.[0])}>
            <div class="vhead">
              {#if track.versions.length > 1}
                {@const isPrimary = primaryVersion(track)?.id === v.id}
                <button class="plain icon star" class:on={isPrimary} title={isPrimary ? "The primary version: it stands for the song in the library" : "Make this the primary version"}
                  disabled={isPrimary} onclick={() => patchTrack({ primary: v.id })}>{isPrimary ? "★" : "☆"}</button>
              {/if}
              <input type="text" class="vname" value={v.name} onchange={(e) => patchVersion(v, { name: e.currentTarget.value })} />
              <button class="plain icon" title="Preview this version on the free deck" onclick={() => audition(v.id)}><Icon name="headphones" size={13} /></button>
              <button class="plain icon" title="Duplicate" onclick={() => duplicateVersion(v)}><Icon name="copy" size={13} /></button>
              {#if !v.hidden}
                <button class="plain icon danger" title="Remove this version from the library (sets that play it keep it)"
                  onclick={() => confirmAction(track.versions.filter((x) => !x.hidden).length < 2
                    ? `Remove “${track.title}” from the library? Sets that play it keep it.`
                    : `Remove version “${v.name}” from the library? Sets that play it keep it.`, "Remove").then((ok) => { if (ok) app.run(api.deleteVersion(v.id)); })}><Icon name="trash" size={13} /></button>
              {/if}
            </div>
            {#if v.hidden}
              <div class="row small warn">
                Not in the library: only the sets that play it keep it.
                <button class="plain small" onclick={() => patchVersion(v, { hidden: false })}>Show in Library</button>
              </div>
            {/if}
            {#if vrec}
              {@const open = audioOpen(v)}
              {@const sharedHere = track.versions.filter((x) => x.id !== v.id && x.recording === vrec.id)}
              {@const others = otherSongs(vrec)}
              <div class="audio">
                <button class="plain small atoggle" title={open ? "Hide the audio's details" : "BPM, meter, files, stems… of the audio this version plays"}
                  onclick={() => (openAudio = { ...openAudio, [v.id]: !open })}>{open ? "▾" : "▸"} Audio</button>
                {#if track.recordings.length > 1}
                  <select class="apick" value={v.recording} title="The audio this version plays" onchange={(e) => patchVersion(v, { recording: e.currentTarget.value })}>
                    {#each track.recordings as r (r.id)}<option value={r.id}>{r.title}</option>{/each}
                  </select>
                {:else}<span class="atitle" title={vrec.title}>{vrec.title}</span>{/if}
                <span class="faint num">{fmtTime(vrec.durationMs)}{vrec.bpm ? ` · ${fmtBpm(vrec.bpm)} BPM` : ""}</span>
                {#each sourceChips(vrec) as c}<span class="chip" title={c.title}>{c.label}</span>{/each}
              </div>
              {#if vrec.removed}
                <div class="warn small">Removed from the library: it can't play until you add it again in Library → Files (or a file that sounds the same; Files lists any it finds).</div>
              {/if}
              {#if open}
                <div class="audiobox">
                  {#if sharedHere.length || others.length}
                    <div class="small warn">
                      Shared: changes here also apply to {#each sharedHere as x, i (x.id)}{i ? ", " : ""}“{x.name}”{/each}{#if sharedHere.length && others.length}, and {/if}{#each others as o, i (o.id)}{i ? ", " : ""}<button class="plain link small" onclick={() => (app.inspectTrack = o.id)}>{o.title}</button>{/each}.
                    </div>
                  {/if}
                  {@render audioDetails(vrec)}
                </div>
              {/if}
            {:else}
              <div class="audio warn small">Its audio was removed from the library: add it again in Library → Files (or find it on Spotify again).</div>
            {/if}
            <div class="grid">
              <span class="lbl">Styles</span>
              <StyleChips value={v.styles ?? []} bpm={pb.target} onchange={(keys) => patchVersion(v, { styles: keys })} />
              <span class="lbl" title="Shown to dancers instead of the style names, e.g. the name of a line dance">Shown as</span>
              <input type="text" value={v.displayName ?? ""} placeholder={app.shownFor(track, v, null).map((s) => s.name).join(" / ") || "style names"}
                onchange={(e) => patchVersion(v, { displayName: e.currentTarget.value.trim() || null })} />
              <span class="lbl">Tags</span>
              <StyleChips kind="tag" value={v.tags ?? []} onchange={(keys) => patchVersion(v, { tags: keys })} />
              <span class="lbl">Tempo</span>
              <span class="row">
                <input type="number" step="0.5" placeholder={pb.base ? fmtBpm(pb.base) : "orig."} value={v.targetBpm ?? ""}
                  onchange={(e) => patchVersion(v, { targetBpm: e.currentTarget.value ? +e.currentTarget.value : null })} />
                {#if v.targetBpm && pb.base}<span class="faint num small">{((pb.rate - 1) * 100).toFixed(1)}%</span>{/if}
                {#if app.clubStyleOf(track, v, null)?.sweet}
                  <button class="plain small" title="Set the tempo to the nearest edge of the style's sweet range" onclick={() => fitToStyle(v)}>Fit</button>
                {/if}
              </span>
              <span class="lbl">Start / end</span>
              <span class="row">
                <TimeField value={v.cueInMs} placeholder="auto" onchange={(x) => patchVersion(v, { cueInMs: x })} />
                <TimeField value={v.cueOutMs} placeholder="auto" onchange={(x) => patchVersion(v, { cueOutMs: x })} />
                {#if previewPos(v) != null}
                  <button class="plain small" title="Set start to the deck position" onclick={() => patchVersion(v, { cueInMs: Math.round(previewPos(v)!) })}>⇤</button>
                  <button class="plain small" title="Set end to the deck position" onclick={() => patchVersion(v, { cueOutMs: Math.round(previewPos(v)!) })}>⇥</button>
                {/if}
              </span>
              <span class="lbl">Fade at</span>
              <span class="row">
                <TimeField value={v.fadeOutMs} placeholder="none" onchange={(x) => patchVersion(v, { fadeOutMs: x })} />
                {#if previewPos(v) != null}
                  <button class="plain small" title="Fade out from the deck position" onclick={() => patchVersion(v, { fadeOutMs: Math.round(previewPos(v)!) })}>here</button>
                {/if}
              </span>
              <span class="lbl">Gain</span>
              <span class="row"><input type="number" step="0.5" placeholder="recording" value={v.gainDb ?? ""}
                onchange={(e) => patchVersion(v, { gainDb: e.currentTarget.value ? +e.currentTarget.value : null })} /> dB</span>
            </div>
            {#if vrec && app.stemsReady.has(vrec.id)}
              <div class="row small">
                <span class="lbl">Stems</span>
                {#each ["vocals", "drums", "bass", "other"] as stem}
                  {@const on = v.stems?.[stem] !== false}
                  <label title="Start this version with the {stem} stem {on ? 'on' : 'muted'}">
                    <input type="checkbox" checked={on} onchange={(e) => {
                      const next: Record<string, boolean> = { ...(v.stems ?? {}) };
                      if (e.currentTarget.checked) delete next[stem]; else next[stem] = false;
                      patchVersion(v, { stems: Object.keys(next).length ? next : null });
                    }} /> {stem}
                  </label>
                {/each}
              </div>
            {/if}
            <textarea rows="1" placeholder="Comment (e.g. slowed for teaching)" value={v.comment ?? ""}
              onchange={(e) => patchVersion(v, { comment: e.currentTarget.value })}></textarea>
            <div class="row small">
              <button class="plain small" title="To suggest this version in a club's file: the song's title with this recording's fingerprint, length and sources"
                onclick={async () => copy(await app.run(api.clubSongText(v.id)), "Song entry")}><Icon name="copy" size={11} /> Club-file entry</button>
              {#if track.versions.length > 1}
                <button class="plain small" title="Make this version a song of its own, still playing its recording (undoes Combine)"
                  onclick={() => app.run(api.separateVersions([v.id]))}>Separate into Its Own Song</button>
              {/if}
            </div>
          </div>
        {/each}
      </section>

    {/if}
  {/if}
</div>

{#if clubMenu}<ContextMenu x={clubMenu.x} y={clubMenu.y} items={clubMenu.items} onclose={() => (clubMenu = null)} />{/if}

<style>
  .insp { overflow-y: auto; overflow-x: hidden; height: 100%; padding: 8px; display: flex; flex-direction: column; gap: 8px; }
  .insp > * { min-width: 0; }
  .box { display: flex; flex-direction: column; gap: 6px; padding: 10px; background: var(--bg-elev-2); border: 1px solid var(--border); border-radius: var(--radius); }
  .setonly { flex-direction: row; align-items: center; justify-content: space-between; font-size: var(--fs-sm); color: var(--text-dim); }
  .entrybox { border-color: color-mix(in srgb, var(--main) 40%, var(--border)); }
  .section-title.sub { margin-top: 6px; }
  .col { display: flex; flex-direction: column; gap: 4px; }
  .section-title.row { display: flex; justify-content: space-between; align-items: center; }
  .grid { display: grid; grid-template-columns: 74px minmax(0, 1fr); gap: 5px 8px; align-items: center; }
  .grid select { min-width: 0; width: 100%; }
  .lbl { color: var(--text-dim); font-size: var(--fs-sm); }
  .row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .big { font-weight: 600; font-size: var(--fs-md); height: 30px; }
  .mult { display: inline-flex; }
  .mult button { padding: 0 5px; height: 22px; }
  .small { font-size: var(--fs-sm); }
  .ok { color: var(--green); }
  .clubchips { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .clubchips .chip { height: 20px; cursor: pointer; }
  .clubchips .chip.off { opacity: 0.4; filter: grayscale(0.7); }
  .clubchips .add { height: 20px; min-width: 24px; padding: 0 6px; }
  .version { display: flex; flex-direction: column; gap: 6px; padding: 8px; border-radius: 6px; background: var(--bg-elev); border-left: 3px solid var(--c); }
  .version.hl { outline: 1px solid var(--blue); }
  .vhead { display: flex; gap: 4px; align-items: center; }
  .vname { flex: 1; font-weight: 600; }
  .star { color: var(--text-faint); font-size: 14px; }
  .audio { display: flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--fs-sm); }
  .atoggle { padding: 0 2px; height: 20px; color: var(--text-dim); flex: none; }
  .atitle { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 auto; }
  .apick { min-width: 0; flex: 1 1 auto; height: 22px; }
  .audio .num { flex: none; }
  .chip { flex: none; font-size: var(--fs-xs); padding: 0 5px; border: 1px solid var(--border-strong); border-radius: 8px; color: var(--text-dim); }
  .audiobox { display: flex; flex-direction: column; gap: 6px; padding: 8px; border-radius: 6px; background: var(--bg-elev-2); border: 1px solid var(--border); }
  .star.on { color: var(--yellow); opacity: 1; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .tagbtn { cursor: pointer; height: 20px; }
  .tagbtn.off { opacity: 0.4; filter: grayscale(0.7); }
  .source { display: flex; gap: 6px; align-items: flex-start; }
  .source :global(svg) { margin-top: 2px; flex: none; color: var(--text-dim); }
  .src-body { flex: 1; min-width: 0; }
  .path { font-size: var(--fs-xs); color: var(--text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
  .warn { color: var(--yellow); }
  .link { padding: 0; height: auto; color: var(--blue); display: inline; }
  .danger { color: var(--red); }
  textarea { width: 100%; }
</style>

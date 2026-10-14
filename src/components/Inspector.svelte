<script lang="ts">
  // Info tab: the selected track (recording), its versions, and — when a set
  // entry is selected — that entry's per-setlist overrides.
  import { api } from "../lib/api";
  import { app, audition, confirmAction } from "../lib/state.svelte";
  import { fmtBpm, fmtTime, foldBpm, playback, rateToFit, resolveSettings, styleCenter } from "../lib/tempo";
  import type { Id, Overrides, Track, Version } from "../lib/types";
  import Icon from "./Icon.svelte";
  import StyleChips from "./StyleChips.svelte";
  import TimeField from "./TimeField.svelte";

  const entry = $derived(app.current?.entries.find((e) => e.id === app.inspectEntry));
  const track = $derived.by((): Track | undefined => {
    if (entry?.versionId) return app.versionMap.get(entry.versionId)?.track;
    return app.inspectTrack ? app.trackMap.get(app.inspectTrack) : undefined;
  });
  const entryVersion = $derived(entry?.versionId ? app.versionMap.get(entry.versionId)?.version : undefined);

  let taps = $state<number[]>([]);
  let linkInput = $state("");

  function patchTrack(patch: Parameters<typeof api.updateTrack>[1]) {
    if (track) app.run(api.updateTrack(track.id, patch));
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

  function tap() {
    const now = performance.now();
    const recent = [...taps.filter((t) => now - t < 2500), now].slice(-9);
    taps = recent;
    if (recent.length >= 4) {
      const iv = (recent[recent.length - 1] - recent[0]) / (recent.length - 1);
      const bpm = Math.round((60000 / iv) * 10) / 10;
      patchTrack({ bpm, bpmSource: "tap" });
    }
  }

  function scaleBpm(f: number) {
    if (track?.bpm) patchTrack({ bpm: Math.round(track.bpm * f * 10) / 10, bpmSource: "user" });
  }

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
    const s = app.clubStyleOf(v, null, track?.bpm ?? null);
    if (!track?.bpm || !s) return;
    const c = styleCenter(s);
    if (!c) return;
    const base = foldBpm(track.bpm, c);
    const rate = rateToFit(base, s);
    patchVersion(v, { targetBpm: rate === 1 ? null : Math.round(base * rate * 10) / 10 });
  }

  /** Uses the preview deck's position, if it has this version loaded. */
  function previewPos(v: Version): number | null {
    const d = app.decks.find((x) => x.versionId === v.id && x.status !== "empty");
    return d ? d.positionMs : null;
  }

  const preferred = $derived(track?.preferredSource ?? "");
</script>

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
          {#if isPause}
            <span class="lbl">Name</span>
            <input type="text" value={entry.label ?? ""} placeholder="Pause (e.g. Break, Lesson)"
              onchange={(e) => api.updateEntry(entry.id, { label: e.currentTarget.value.trim() || null })} />
          {:else}
          <span class="lbl">{entryVersion ? "Show as" : "Slot"}</span>
          {#if entryVersion && (entryVersion.styles?.length ?? 0) > 0}
            <StyleChips value={shownKeys} choices={entryVersion.styles} bpm={r?.target ?? null}
              onchange={(keys) => api.updateEntry(entry.id, { styles: keys.length === entryVersion.styles?.length || !keys.length ? null : keys })} />
          {:else}
            <StyleChips value={shownKeys} bpm={r?.target ?? null}
              onchange={(keys) => api.updateEntry(entry.id, { styles: keys.length ? keys : null })} />
          {/if}
          <span class="lbl" title="Text shown instead of the style names (setlist and projector)">Label</span>
          <input type="text" value={entry.label ?? ""}
            placeholder={app.shownFor(entryVersion, entry, r?.target ?? null).map((s) => s.name).join(" / ") || "e.g. Mixer"}
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
            {#each app.profile?.tags ?? [] as t}
              {@const on = (entry.tags ?? []).includes(t.id)}
              <button class="badge tagbtn" class:off={!on} style:--c={t.color}
                onclick={() => api.updateEntry(entry.id, { tags: on ? (entry.tags ?? []).filter((x) => x !== t.id) : [...(entry.tags ?? []), t.id] })}>
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
        <div class="grid">
          <span class="lbl">BPM</span>
          <span class="row">
            <input type="number" step="0.1" value={track.bpm ?? ""} onchange={(e) => patchTrack({ bpm: e.currentTarget.value ? +e.currentTarget.value : null })} />
            <button class="small" onclick={tap} title="Tap along to the beat (4+ taps)">Tap</button>
            <span class="mult">
              <button class="plain small" onclick={() => scaleBpm(0.5)} title="Half">½</button>
              <button class="plain small" onclick={() => scaleBpm(2 / 3)} title="×⅔ (triple meter)">⅔</button>
              <button class="plain small" onclick={() => scaleBpm(1.5)} title="×1.5 (triple meter)">1½</button>
              <button class="plain small" onclick={() => scaleBpm(2)} title="Double">2×</button>
            </span>
          </span>
          <span></span>
          <span class="row">
            {#if track.bpmSource !== "detected"}
              <button class="plain small" title="Use the BPM found by analysis"
                onclick={() => app.run(api.resetBpm(track.id, "detected"))}>
                ↺ Analysed{track.detectedBpm != null ? ` (${fmtBpm(track.detectedBpm)})` : ""}
              </button>
            {/if}
            {#if track.bpmSource !== "tag" && (track.tagBpm != null || track.files.length)}
              <button class="plain small" title="Use the BPM stored in the file's tags"
                onclick={() => app.run(api.resetBpm(track.id, "tag"))}>
                ↺ File tag{track.tagBpm != null ? ` (${fmtBpm(track.tagBpm)})` : ""}
              </button>
            {/if}
          </span>
          <span></span>
          <span class="faint small">{track.bpmSource ? `from ${{ detected: "analysis", tag: "file tag", tap: "tapping", user: "you" }[track.bpmSource] ?? track.bpmSource}` : track.analyzed ? "not detected" : "analysing…"}{#if track.loudnessDb != null} · {track.loudnessDb.toFixed(1)} LUFS{/if} · {fmtTime(track.durationMs)}</span>
          <span class="lbl" title="Beats per bar. Detected from the audio, on the beat grid of the BPM above (fix the BPM first if it's off)">Meter</span>
          <span class="row">
            <select value={track.meterSource === "user" ? String(track.beatsPerBar ?? "") : "auto"}
              onchange={(e) => { const v = e.currentTarget.value; patchTrack({ beatsPerBar: v === "auto" ? null : +v }); }}>
              <option value="auto">Detected{track.detectedMeter ? `: ${meterName(track.detectedMeter)}` : track.analyzed ? ": unclear" : ""}</option>
              <option value="3">Triple (3/4, 6/8)</option>
              <option value="4">Duple (2/4, 4/4)</option>
            </select>
          </span>
          <span class="lbl">Gain</span>
          <span class="row"><input type="number" step="0.5" value={track.gainDb ?? 0} onchange={(e) => patchTrack({ gainDb: +e.currentTarget.value })} /> dB</span>
        </div>
        <textarea rows="2" placeholder="Notes" value={track.notes ?? ""} onchange={(e) => patchTrack({ notes: e.currentTarget.value })}></textarea>
      </section>

      <section class="box">
        <div class="section-title row">Versions <button class="plain small" onclick={newVersion}><Icon name="plus" size={12} /> New</button></div>
        {#each track.versions as v (v.id)}
          {@const pb = playback(track, v, null, app.clubStyleOf)}
          <div class="version" class:hl={app.selected.includes(v.id) || entry?.versionId === v.id} style:--c={app.styleColor(v.styles?.[0])}>
            <div class="vhead">
              <input type="text" class="vname" value={v.name} onchange={(e) => patchVersion(v, { name: e.currentTarget.value })} />
              <button class="plain icon" title="Preview this version on the free deck" onclick={() => audition(v.id)}><Icon name="headphones" size={13} /></button>
              <button class="plain icon" title="Duplicate" onclick={() => duplicateVersion(v)}><Icon name="copy" size={13} /></button>
              <button class="plain icon danger" title="Delete version" disabled={track.versions.length < 2}
                onclick={() => confirmAction(`Delete version “${v.name}”?`).then((ok) => { if (ok) app.run(api.deleteVersion(v.id)); })}><Icon name="trash" size={13} /></button>
            </div>
            <div class="grid">
              <span class="lbl">Styles</span>
              <StyleChips value={v.styles ?? []} bpm={pb.target} onchange={(keys) => patchVersion(v, { styles: keys })} />
              <span class="lbl" title="Shown to dancers instead of the style names, e.g. the name of a line dance">Shown as</span>
              <input type="text" value={v.displayName ?? ""} placeholder={app.shownFor(v, null, pb.target).map((s) => s.name).join(" / ") || "style names"}
                onchange={(e) => patchVersion(v, { displayName: e.currentTarget.value.trim() || null })} />
              <span class="lbl">Tags</span>
              <StyleChips kind="tag" value={v.tags ?? []} onchange={(keys) => patchVersion(v, { tags: keys })} />
              <span class="lbl">Tempo</span>
              <span class="row">
                <input type="number" step="0.5" placeholder={pb.base ? fmtBpm(pb.base) : "orig."} value={v.targetBpm ?? ""}
                  onchange={(e) => patchVersion(v, { targetBpm: e.currentTarget.value ? +e.currentTarget.value : null })} />
                {#if v.targetBpm && pb.base}<span class="faint num small">{((pb.rate - 1) * 100).toFixed(1)}%</span>{/if}
                {#if app.clubStyleOf(v, null, track.bpm)?.sweet}
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
              <span class="row"><input type="number" step="0.5" placeholder="song" value={v.gainDb ?? ""}
                onchange={(e) => patchVersion(v, { gainDb: e.currentTarget.value ? +e.currentTarget.value : null })} /> dB</span>
            </div>
            {#if app.stemsReady.has(track.id)}
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
          </div>
        {/each}
      </section>

      <section class="box">
        <div class="section-title">Sources</div>
        {#each track.sources as s}
          <div class="source">
            <Icon name={s.kind === "spotify" ? "spotify" : "music"} size={13} />
            {#if s.kind === "file"}
              {@const paths = track.files}
              <div class="src-body">
                {#if paths.length}
                  {#each paths as p}<div class="path" title={p}>{p}</div>{/each}
                {:else}
                  <div class="warn small">File not found on this computer (content id {s.key.slice(0, 10)}…). Add its folder to your library to relink it.</div>
                {/if}
              </div>
            {:else}
              <div class="src-body path" title={s.key}>{s.key}</div>
            {/if}
            {#if track.sources.length > 1}
              <button class="plain icon" title="Remove source" onclick={() => app.run(api.removeSource(track.id, s))}><Icon name="x" size={12} /></button>
            {/if}
          </div>
        {/each}
        {#if !track.sources.some((s) => s.kind === "spotify")}
          <div class="row">
            <input type="text" placeholder="Link a Spotify track (URL or URI)" bind:value={linkInput} style="flex:1" />
            <button class="small" disabled={!linkInput.trim()} onclick={async () => { if (await app.run(api.linkSpotify(track.id, linkInput))) linkInput = ""; }}>
              <Icon name="link" size={12} /> Link
            </button>
          </div>
        {/if}
        {#if track.sources.length > 1}
          <div class="row small">
            Play from
            <div class="segmented">
              <button class:on={preferred !== "spotify"} onclick={() => patchTrack({ preferredSource: null })}>File</button>
              <button class:on={preferred === "spotify"} onclick={() => patchTrack({ preferredSource: "spotify" })}>Spotify</button>
            </div>
          </div>
        {/if}
        {#if track.files.length}
          {@const job = app.stems?.jobs[track.id]}
          <div class="row small">
            {#if app.stemsReady.has(track.id)}
              Stems ready
              <button class="plain small" onclick={() => app.run(api.stemsDelete(track.id))}>Delete stems</button>
            {:else if job && job.state !== "failed"}
              <span class="num">Preparing stems… {job.state === "running" ? `${Math.round(job.percent)}%` : "(queued)"}</span>
            {:else}
              <button class="small" onclick={() => app.run(api.stemsPrepare([track.id]))}>Prepare Stems</button>
              {#if job?.state === "failed"}<span class="warn">{job.message}</span>{/if}
            {/if}
          </div>
        {/if}
        <div class="row small faint">
          {track.analyzed ? "Analysed" : "Not analysed yet"} · played {track.playCount}×
          <button class="plain small" onclick={() => api.analyze([track.id])}><Icon name="reload" size={11} /> Re-analyse</button>
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .insp { overflow-y: auto; height: 100%; padding: 8px; display: flex; flex-direction: column; gap: 8px; }
  .box { display: flex; flex-direction: column; gap: 6px; padding: 10px; background: var(--bg-elev-2); border: 1px solid var(--border); border-radius: var(--radius); }
  .setonly { flex-direction: row; align-items: center; justify-content: space-between; font-size: var(--fs-sm); color: var(--text-dim); }
  .entrybox { border-color: color-mix(in srgb, var(--main) 40%, var(--border)); }
  .section-title.sub { margin-top: 6px; }
  .col { display: flex; flex-direction: column; gap: 4px; }
  .section-title.row { display: flex; justify-content: space-between; align-items: center; }
  .grid { display: grid; grid-template-columns: 74px 1fr; gap: 5px 8px; align-items: center; }
  .lbl { color: var(--text-dim); font-size: var(--fs-sm); }
  .row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .big { font-weight: 600; font-size: var(--fs-md); height: 30px; }
  .mult { display: inline-flex; }
  .mult button { padding: 0 5px; height: 22px; }
  .small { font-size: var(--fs-sm); }
  .version { display: flex; flex-direction: column; gap: 6px; padding: 8px; border-radius: 6px; background: var(--bg-elev); border-left: 3px solid var(--c); }
  .version.hl { outline: 1px solid var(--blue); }
  .vhead { display: flex; gap: 4px; align-items: center; }
  .vname { flex: 1; font-weight: 600; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .tagbtn { cursor: pointer; height: 20px; }
  .tagbtn.off { opacity: 0.4; filter: grayscale(0.7); }
  .source { display: flex; gap: 6px; align-items: flex-start; }
  .source :global(svg) { margin-top: 2px; flex: none; color: var(--text-dim); }
  .src-body { flex: 1; min-width: 0; }
  .path { font-size: var(--fs-xs); color: var(--text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
  .warn { color: var(--yellow); }
  .danger { color: var(--red); }
  textarea { width: 100%; }
</style>

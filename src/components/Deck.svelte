<script lang="ts">
  // One of two identical decks. The crossfader decides which deck the room
  // hears ("on air"); the other is the free deck, cued in headphones.
  import { NEUTRAL, customLabel } from "../lib/styles";
  import { api } from "../lib/api";
  import { artwork } from "../lib/artwork";
  import { addToSet, app, loadDeck } from "../lib/state.svelte";
  import { fmtBpm, fmtPct, fmtTime, foldBpm, playback, tempoFit } from "../lib/tempo";
  import type { Id } from "../lib/types";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import StyleBadge from "./StyleBadge.svelte";
  import Waveform from "./Waveform.svelte";

  interface Props { deck: 0 | 1 }
  let { deck }: Props = $props();

  const name = $derived(deck === 0 ? "A" : "B");
  const color = $derived(deck === 0 ? "#0a84ff" : "#ff9f0a");
  const d = $derived(app.decks[deck]);
  const row = $derived(d.versionId ? app.versionMap.get(d.versionId) : undefined);
  const track = $derived(row?.track ?? (d.trackId ? app.trackMap.get(d.trackId) : undefined));
  const entry = $derived(d.entryId ? app.current?.entries.find((e) => e.id === d.entryId) : undefined);
  const pb = $derived(playback(track, row?.version, entry ?? null, app.clubStyleOf));
  const shown = $derived(row ? app.shownFor(row.version, entry, pb.target) : []);
  const style = $derived(row ? (app.clubStyleOf(row.version, entry, pb.target) ?? undefined) : undefined);
  const baseBpm = $derived(track?.bpm ? (pb.target ? foldBpm(track.bpm, pb.target) : (pb.base ?? track.bpm)) : null);
  const liveBpm = $derived(baseBpm ? baseBpm * d.tempo : null);
  const fit = $derived(tempoFit(liveBpm, style));
  const endMs = $derived.by(() => {
    let end = d.endMs ?? d.durationMs;
    if (d.fadeAtMs != null) end = Math.min(end, d.fadeAtMs + app.fadeSeconds * 1000);
    return end;
  });
  const remaining = $derived(Math.max(0, (endMs - d.positionMs) / (d.tempo || 1)));
  const elapsed = $derived(Math.max(0, (d.positionMs - d.startMs) / (d.tempo || 1)));
  const range = $derived((app.settings?.tempoRangePct ?? 10) / 100);
  const playing = $derived(d.status === "playing");
  const loaded = $derived(d.status !== "empty" && d.status !== "error");
  /** The room hears this deck right now. */
  const audible = $derived(d.onAir && (playing || d.status === "loading"));
  const isProtected = $derived(audible && (app.settings?.protectOnAir ?? true));
  const isNext = $derived(!!d.entryId && d.entryId === app.perform?.nextEntryId);
  const warnSlow = $derived(d.tempo < 1 - (app.profile?.tempo.maxSlowdownPct ?? 8) / 100 - 1e-6);
  const nearEnd = $derived(audible && remaining < 15000 && !d.fading);
  const hotCues = $derived(track?.hotCues ?? []);
  const atCue = $derived(loaded && !playing && Math.abs(d.positionMs - d.cueMs) < 60);
  const STEM_LABELS = [["Vox", "Vocals", "#ff6482"], ["Drm", "Drums", "#ffd60a"], ["Bas", "Bass", "#30d158"], ["Oth", "Other (melody, instruments)", "#64d2ff"]] as const;
  const stemJob = $derived(track ? app.stems?.jobs[track.id] : undefined);
  const canStem = $derived(!!track && track.files.length > 0);

  let art = $state<string | null>(null);
  $effect(() => {
    const id = track?.id;
    art = null;
    if (id) artwork(id).then((a) => { if (track?.id === id) art = a; });
  });

  /** Deck volume range (dB); matches the engine's. */
  const VOL_MIN = -24;
  const VOL_MAX = 12;
  let volDraft = $state<number | null>(null);
  const volShown = $derived(volDraft ?? d.trimDb);
  const level = $derived(app.levels.decks?.[deck] ?? 0);
  function setVolume(db: number) {
    volDraft = db;
    api.deckSet(deck, { trimDb: db });
  }

  let tempoDraft = $state<number | null>(null);
  const tempoShown = $derived(tempoDraft ?? d.tempo);
  let altDown = $state(false);
  let dropping = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function setTempo(v: number) {
    tempoDraft = v;
    api.deckSet(deck, { tempo: v });
  }

  function guard(alt = false): boolean {
    if (isProtected && !(alt || altDown)) {
      app.toast(`Deck ${name} is on air. Hold ⌥ Option / Alt to seek it anyway, or turn off protection in Settings → General.`, "info");
      return false;
    }
    return true;
  }

  function seek(ms: number, alt = false) {
    if (!guard(alt)) return;
    // Past the guard on a protected deck means ⌥ / Alt is held: override.
    app.run(isProtected ? api.deckSeek(deck, ms, true) : api.deckScrub(deck, ms));
  }

  /** Dragging on the waveform; with ⌥ / Alt it also works on the on-air deck. */
  function scrub(ms: number, alt = false) {
    const force = alt || altDown;
    if (isProtected && !force) return;
    api.deckScrub(deck, ms, force).catch(() => {});
  }

  function pad(slot: number, e: MouseEvent) {
    if (e.altKey || e.button === 2) {
      e.preventDefault();
      app.run(api.deckHotcue(deck, slot, "clear"));
      return;
    }
    if (hotCues.some((c) => c.slot === slot) && !guard()) return;
    app.run(api.deckHotcue(deck, slot, "trigger"));
  }

  function versionsFromDrag(e: DragEvent): Id[] {
    try {
      return JSON.parse(e.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
    } catch {
      return [];
    }
  }

  async function drop(e: DragEvent) {
    e.preventDefault();
    dropping = false;
    const entries: Id[] = JSON.parse(e.dataTransfer?.getData("application/x-orpheus-entries") || "[]");
    const [vid] = versionsFromDrag(e);
    if (vid) await loadDeck(deck, vid, entries.length === 1 ? entries[0] : null);
  }

  function openMenu(e: MouseEvent) {
    const items: MenuItem[] = [
      { label: "Set Start of Version Here", disabled: !loaded, action: () => app.run(api.deckSetPoint(deck, "start")) },
      { label: "Set End of Version Here", disabled: !loaded, action: () => app.run(api.deckSetPoint(deck, "end")) },
      { label: "Set Fade-Out Point Here", disabled: !loaded, action: () => app.run(api.deckSetPoint(deck, "fade")) },
      { label: "Clear Start / End / Fade", disabled: !loaded, action: () => app.run(api.deckSetPoint(deck, "clear")) },
      { separator: true },
      { label: "Reset Tempo", disabled: !loaded, action: () => api.deckTempoReset(deck) },
      { label: "Play Next After Current Song", disabled: !d.versionId || !!entry, action: () => d.versionId && addToSet([d.versionId], "next") },
      { separator: true },
      { label: "Eject", disabled: !loaded || audible, action: () => app.run(api.deckAction(deck, "unload")) },
    ];
    menu = { x: e.clientX, y: e.clientY, items };
  }
</script>

<svelte:window onkeydown={(e) => (altDown = e.altKey)} onkeyup={(e) => (altDown = e.altKey)} onblur={() => (altDown = false)} />

<section
  class="deck"
  class:audible
  class:near={nearEnd}
  class:dropping
  style:--deck={color}
  aria-label="Deck {name}"
  ondragover={(e) => { if (!audible) { e.preventDefault(); dropping = true; } }}
  ondragleave={() => (dropping = false)}
  ondrop={drop}
>
  <header>
    <span class="letter">{name}</span>
    {#if audible}
      <span class="air live"><span class="dot"></span> ON AIR</span>
    {:else if d.onAir && loaded}
      <span class="air">IN THE ROOM</span>
    {:else if d.pfl}
      <span class="air phones"><Icon name="headphones" size={11} /> HEADPHONES</span>
    {/if}
    {#if isNext}<span class="badge" style:--c="#30d158">Up next</span>{/if}
    <span class="status">
      {#if d.status === "loading"}Loading…
      {:else if d.fading}Fading out…
      {:else if d.status === "ended"}Ended
      {:else if d.status === "error"}<span class="err">Error</span>
      {:else if d.status === "paused" && loaded}{atCue ? "Cued" : "Paused"}
      {/if}
    </span>
    <span class="spacer"></span>
    <span class="vol" title="Deck volume (double-click: 0 dB). The meter shows this deck's level, whether or not the room hears it.">
      <span class="meter"><span style:width="{Math.min(100, Math.sqrt(level) * 100)}%" class:hot={level > 0.985}></span></span>
      <input type="range" min={VOL_MIN} max={VOL_MAX} step="0.5" value={volShown} aria-label="Deck {name} volume"
        oninput={(e) => setVolume(+e.currentTarget.value)} onchange={() => (volDraft = null)}
        ondblclick={() => { setVolume(0); volDraft = null; }} />
      <span class="db num">{volShown > 0 ? "+" : volShown < 0 ? "−" : ""}{Math.abs(volShown).toFixed(1)} dB</span>
    </span>
    <button class="plain icon" class:on={d.pfl} title="Hear this deck in headphones (PFL)"
      onclick={() => api.deckSet(deck, { pfl: !d.pfl })}><Icon name="headphones" size={15} /></button>
    <button class="plain icon" title="Deck actions" onclick={openMenu}><Icon name="dots" size={15} /></button>
  </header>

  <div class="meta">
    <div class="art" style:--c={shown[0]?.color ?? NEUTRAL}>
      {#if art}
        <img src={art} alt="" />
      {:else}
        <span>{customLabel(row?.version, entry) || shown[0]?.short || (loaded ? "♪" : "")}</span>
      {/if}
    </div>
    <div class="titles">
      {#if track}
        <div class="title" title={track.title}>{track.title}</div>
        <div class="artist">
          {track.artist}
          {#if row && app.versionLabel(row.version, track)}<span class="ver">· {row.version.name}</span>{/if}
        </div>
        <div class="chips">
          <StyleBadge styles={shown} label={customLabel(row?.version, entry)} />
          {#each entry?.tags ?? [] as tag}
            {@const t = app.profile?.tags.find((x) => x.id === tag)}
            <span class="badge" style:--c={t?.color ?? "#999"}>{t?.name ?? tag}</span>
          {/each}
          {#if track.sources.some((s) => s.kind === "spotify") && (track.files.length === 0 || track.preferredSource === "spotify")}
            <span class="badge" style:--c="#1db954">Spotify</span>
          {/if}
        </div>
      {:else if d.status === "loading"}
        <div class="title faint">Loading…</div>
      {:else}
        <div class="title faint">Deck {name} is empty</div>
        <div class="artist faint">Drag a song here{app.settings?.autoLoadNext ? " — the set's next song loads automatically" : ""}</div>
      {/if}
      {#if d.error}<div class="err small">{d.error}</div>{/if}
    </div>
    <div class="times num">
      <div class="remain" class:near={nearEnd}>{loaded ? "−" + fmtTime(remaining) : "–:––"}</div>
      <div class="elapsed dim">{loaded ? fmtTime(elapsed) : ""}</div>
    </div>
  </div>

  <Waveform
    trackId={d.trackId}
    revision={track?.analyzed}
    durationMs={d.durationMs}
    positionMs={d.positionMs}
    startMs={d.startMs}
    endMs={d.endMs}
    fadeAtMs={d.fadeAtMs}
    cueMs={loaded ? d.cueMs : null}
    {hotCues}
    accent={color}
    height={58}
    onseek={loaded ? seek : null}
    onscrub={loaded ? scrub : null}
    seekHint={isProtected ? "On air: ⌥-click to seek" : "Click or drag to scrub"}
  />

  <div class="controls">
    <button class="cue" class:lit={atCue} disabled={!loaded}
      title="CUE: while playing, back to the cue point; while paused, set the cue point here (or hold to preview)"
      onpointerdown={(e) => { if (e.button === 0) api.deckCue(deck, true); }}
      onpointerup={() => api.deckCue(deck, false)}
      onpointerleave={(e) => { if (e.buttons) api.deckCue(deck, false); }}>CUE</button>
    <button class="transport" class:active={playing} disabled={!loaded || d.status === "loading"}
      title={playing ? "Pause" : d.onAir ? "Play (the room hears this deck)" : "Play in headphones"}
      onclick={() => api.deckAction(deck, "toggle")}>
      <Icon name={playing ? "pause" : "play"} size={18} />
    </button>
    <div class="pads" role="group" aria-label="Hot cues">
      {#each [1, 2, 3, 4] as n}
        {@const set = hotCues.some((c) => c.slot === n)}
        <button class="pad" class:set style:--pc={["#ff453a", "#ff9f0a", "#30d158", "#64d2ff"][n - 1]} disabled={!loaded}
          title={set ? `Hot cue ${n}: jump (⌥-click or right-click to clear)` : `Set hot cue ${n} here`}
          onclick={(e) => pad(n, e)} oncontextmenu={(e) => pad(n, e)}>{n}</button>
      {/each}
    </div>
    <span class="spacer"></span>
    {#if audible && playing}
      <button disabled={d.fading} title="Fade out ({app.fadeSeconds}s)" onclick={() => api.deckAction(deck, "fadeOut")}>
        <Icon name="fade" /> Fade
      </button>
    {:else}
      <button class="golive" disabled={!loaded} title="Put this deck on air (the other deck fades out first)"
        onclick={async () => {
          const r = await app.run(api.deckGoLive(deck));
          if (r === "armed") app.toast("A song is playing — press Go Live again within 3 s to fade it out.", "info");
        }}>Go Live</button>
    {/if}
    {#if d.fading}
      <button class="plain" onclick={() => api.deckAction(deck, "cancelFade")}>Cancel</button>
    {/if}
  </div>

  <div class="tempo">
    {#if canStem}
      <div class="stems" role="group" aria-label="Stems">
        {#if d.stems}
          {#each STEM_LABELS as [short, long, c], i}
            <button class="stem" class:off={!d.stems[i]} style:--sc={c}
              title="{long}: {d.stems[i] ? 'on — click to mute' : 'muted — click to turn on'}"
              onclick={() => app.run(api.deckStem(deck, i))}>{short}</button>
          {/each}
        {:else if stemJob?.state === "running" || stemJob?.state === "queued"}
          <span class="stem-prep num" title="Separating vocals, drums, bass and other">
            Stems {stemJob.state === "running" ? `${Math.round(stemJob.percent)}%` : "queued"}
          </span>
        {:else}
          <button class="plain small stem-prep" disabled={!loaded}
            title={stemJob?.state === "failed" ? `Failed: ${stemJob.message}` : "Separate this song into vocals, drums, bass and other (takes a minute or two)"}
            onclick={() => app.run(api.deckStem(deck, 0))}>{stemJob?.state === "failed" ? "Stems failed — retry" : "Prepare stems"}</button>
        {/if}
      </div>
    {/if}
    <div class="bpm num" class:outside={fit === "outside"} class:sweet={fit === "sweet"}
      title={style?.sweet ? `${style.name}: ${style.sweet[0]}–${style.sweet[1]} BPM` : ""}>
      <strong>{fmtBpm(liveBpm)}</strong> <span class="dim">BPM</span>
      {#if baseBpm && Math.abs(d.tempo - 1) > 0.0005}<span class="dim">· orig {fmtBpm(baseBpm)}</span>{/if}
    </div>
    <input type="range" min={1 - range} max={1 + range} step={0.001} value={tempoShown}
      oninput={(e) => setTempo(+e.currentTarget.value)} onchange={() => (tempoDraft = null)}
      ondblclick={() => api.deckTempoReset(deck)} disabled={!loaded} aria-label="Tempo" />
    <button class="plain pct num" class:warn={warnSlow} disabled={!loaded} onclick={() => api.deckTempoReset(deck)}
      title={warnSlow ? "Slowed more than the profile's recommended maximum — may sound warbly" : "Reset to the version's tempo"}>
      {fmtPct(tempoShown)}
    </button>
  </div>
</section>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .deck { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; background: var(--bg-elev); border: 1px solid var(--border);
    border-top: 2px solid color-mix(in srgb, var(--deck) 55%, var(--border)); border-radius: var(--radius); min-width: 0; }
  .deck.audible { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--red) 35%, transparent); }
  .deck.near { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--orange) 60%, transparent); }
  .deck.dropping { outline: 2px dashed var(--blue); outline-offset: -3px; }
  header { display: flex; align-items: center; gap: 8px; height: 20px; }
  .vol { display: flex; align-items: center; gap: 6px; }
  .vol input { width: 90px; }
  .vol .meter { width: 46px; height: 6px; background: var(--bg); border-radius: 3px; overflow: hidden; border: 1px solid var(--border); }
  .vol .meter span { display: block; height: 100%; background: var(--green); transition: width 60ms linear; }
  .vol .meter span.hot { background: var(--red); }
  .db { width: 52px; text-align: right; font-size: var(--fs-xs); color: var(--text-dim); }
  .letter { font: 800 15px var(--font-display); color: var(--deck); width: 14px; }
  .air { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-xs); font-weight: 700; letter-spacing: 0.08em; color: var(--text-dim); }
  .air.live { color: var(--red); }
  .air.phones { color: var(--teal); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--red); box-shadow: 0 0 8px var(--red); animation: pulse 1.6s infinite; }
  @keyframes pulse { 50% { opacity: 0.5; } }
  .status { font-size: var(--fs-sm); color: var(--text-dim); }
  .spacer { flex: 1; }
  button.on { color: var(--teal); background: color-mix(in srgb, var(--teal) 16%, transparent); }
  .meta { display: flex; gap: 10px; align-items: center; min-height: 56px; }
  .art { width: 52px; height: 52px; flex: none; border-radius: 6px; overflow: hidden; display: grid; place-items: center;
    background: color-mix(in srgb, var(--c) 25%, var(--bg-elev-2)); color: color-mix(in srgb, var(--c) 80%, white); font-weight: 800; font-size: 14px; }
  .art img { width: 100%; height: 100%; object-fit: cover; }
  .titles { flex: 1; min-width: 0; }
  .title { font: 600 19px/1.2 var(--font-display); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .artist { color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
  .ver { color: var(--text-faint); }
  .chips { display: flex; gap: 4px; margin-top: 4px; flex-wrap: nowrap; overflow: hidden; }
  .times { text-align: right; }
  .remain { font: 600 24px/1 var(--font-display); }
  .remain.near { color: var(--orange); }
  .elapsed { margin-top: 3px; }
  .controls { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .cue { width: 46px; height: 30px; font-weight: 700; font-size: var(--fs-sm); letter-spacing: 0.05em; }
  .cue.lit { color: var(--orange); border-color: color-mix(in srgb, var(--orange) 60%, transparent); }
  .transport { width: 46px; height: 30px; }
  .transport.active { color: var(--green); border-color: color-mix(in srgb, var(--green) 60%, transparent); }
  .pads { display: flex; gap: 3px; margin: 0 4px; }
  .pad { width: 28px; height: 26px; padding: 0; font-size: var(--fs-sm); font-weight: 600; color: var(--text-faint); }
  .pad.set { background: color-mix(in srgb, var(--pc) 30%, var(--bg-elev-3)); border-color: color-mix(in srgb, var(--pc) 70%, transparent); color: white; }
  .golive { font-weight: 600; }
  .tempo { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .stems { display: flex; gap: 2px; flex: none; margin-right: 4px; }
  .stem { width: 34px; height: 22px; padding: 0; font-size: 10px; font-weight: 700; letter-spacing: 0.03em;
    background: color-mix(in srgb, var(--sc) 28%, var(--bg-elev-3)); border-color: color-mix(in srgb, var(--sc) 60%, transparent); color: white; }
  .stem.off { background: var(--bg-elev-2); border-color: var(--border); color: var(--text-faint); text-decoration: line-through; }
  .stem-prep { font-size: var(--fs-xs); color: var(--text-dim); white-space: nowrap; }
  .tempo input { flex: 1; min-width: 40px; }
  .bpm { font-size: var(--fs); white-space: nowrap; }
  .bpm strong { font-size: var(--fs-md); }
  .bpm.sweet strong { color: var(--green); }
  .bpm.outside strong { color: var(--yellow); }
  .pct { min-width: 52px; padding: 0 4px; font-size: var(--fs-sm); flex: none; }
  .pct.warn { color: var(--yellow); }
  .err { color: var(--red); }
  .small { font-size: var(--fs-sm); }
</style>

<script lang="ts">
  import { api } from "../lib/api";
  import { addToSet, app } from "../lib/state.svelte";
  import { fmtBpm, fmtPct, fmtTime, foldBpm, playback, resolveSettings, styleOf, tempoFit } from "../lib/tempo";
  import Icon from "./Icon.svelte";
  import StyleBadge from "./StyleBadge.svelte";
  import Waveform from "./Waveform.svelte";

  interface Props { deck: 0 | 1 }
  let { deck }: Props = $props();

  const d = $derived(app.decks[deck]);
  const isMain = $derived(deck === 0);
  const row = $derived(d.versionId ? app.versionMap.get(d.versionId) : undefined);
  const track = $derived(row?.track ?? (d.trackId ? app.trackMap.get(d.trackId) : undefined));
  const entry = $derived(d.entryId ? app.current?.entries.find((e) => e.id === d.entryId) : undefined);
  const styleKey = $derived(styleOf(row?.version, entry));
  const style = $derived(app.style(styleKey));
  const pb = $derived(playback(track, row?.version, entry ?? null, app.profile));
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
  const warnSlow = $derived(d.tempo < 1 - (app.profile?.tempo.maxSlowdownPct ?? 8) / 100 - 1e-6);
  const loaded = $derived(d.status !== "empty");
  const nearEnd = $derived(isMain && playing && remaining < 15000 && !d.fading);

  let tempoDraft = $state<number | null>(null);
  const tempoShown = $derived(tempoDraft ?? d.tempo);

  function setTempo(v: number) {
    tempoDraft = v;
    api.deckSet(deck, { tempo: v });
  }

  function resetTempo() {
    const target = row ? resolveSettings(row.version, entry).targetBpm : null;
    const t = target && track?.bpm ? target / foldBpm(track.bpm, target) : 1;
    tempoDraft = null;
    api.deckSet(deck, { tempo: t });
  }

  function seek(ms: number) {
    api.deckSeek(deck, ms);
  }

  function mainSeek(ms: number) {
    // Seeking the on-air deck is guarded: the room hears it.
    if (!playing || altDown) seek(ms);
    else app.toast("Hold ⌥ Option / Alt and click to seek the main deck while it's playing.", "info");
  }

  let altDown = $state(false);
</script>

<svelte:window onkeydown={(e) => (altDown = e.altKey)} onkeyup={(e) => (altDown = e.altKey)} />

<section class="deck" class:main={isMain} class:playing class:near={nearEnd} style:--deck={isMain ? "var(--main)" : "var(--preview)"}>
  <header>
    <span class="label">
      {#if isMain}
        <span class="dot" class:live={playing}></span> ON AIR
      {:else}
        <Icon name="headphones" size={13} /> PREVIEW
      {/if}
    </span>
    <span class="status">
      {#if d.status === "loading"}Loading…
      {:else if d.fading}Fading out…
      {:else if d.status === "ended"}Ended
      {:else if d.status === "error"}<span class="err">Error</span>
      {:else if d.status === "paused" && loaded}Paused
      {/if}
    </span>
    <span class="spacer"></span>
    {#if isMain}
      <button class="plain icon" class:on={d.pfl} title="Hear the main deck in headphones (PFL)"
        onclick={() => api.deckSet(0, { pfl: !d.pfl })}><Icon name="headphones" size={15} /></button>
    {:else if !app.outputs?.cue}
      <span class="warn-chip" title="Choose a headphone output in Settings → Audio">No headphone output</span>
    {/if}
  </header>

  <div class="meta">
    <div class="art" style:--c={app.styleColor(styleKey)}>
      {#if track?.artworkUrl}
        <img src={track.artworkUrl} alt="" />
      {:else}
        <span>{style?.short ?? (loaded ? "♪" : "")}</span>
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
          <StyleBadge style={styleKey} />
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
        <div class="title faint">{isMain ? "Nothing playing" : "Nothing in preview"}</div>
        <div class="artist faint">{isMain ? "Press Play Next to start the set" : "Double-click a song to audition it in your headphones"}</div>
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
    accent={isMain ? "#ff9f0a" : "#0a84ff"}
    height={isMain ? 64 : 52}
    onseek={loaded ? (isMain ? mainSeek : seek) : null}
    seekHint={isMain ? "⌥-click to seek while playing" : "Click to seek"}
  />

  <div class="controls">
    <button class="transport" class:active={playing} disabled={!loaded || d.status === "loading"}
      title={playing ? "Pause" : "Play"} onclick={() => api.deckAction(deck, "toggle")}>
      <Icon name={playing ? "pause" : "play"} size={18} />
    </button>
    {#if isMain}
      <button disabled={!playing || d.fading} title="Fade out ({app.fadeSeconds}s)" onclick={() => api.deckAction(0, "fadeOut")}>
        <Icon name="fade" /> Fade
      </button>
      {#if d.fading}
        <button class="plain" onclick={() => api.deckAction(0, "cancelFade")}>Cancel fade</button>
      {/if}
    {:else}
      <button class="icon" disabled={!loaded} title="Back to start" onclick={() => api.deckAction(1, "stop")}>
        <Icon name="back" size={13} />
      </button>
      <button disabled={!d.versionId} title="Play this song next" onclick={() => d.versionId && addToSet([d.versionId], "next")}>
        <Icon name="plus" size={13} /> Next
      </button>
      <button disabled={!d.versionId} title="Add to the end of the set" onclick={() => d.versionId && addToSet([d.versionId], "end")}>
        <Icon name="plus" size={13} /> End
      </button>
    {/if}

    <div class="tempo">
      <div class="bpm num" class:outside={fit === "outside"} class:sweet={fit === "sweet"}
        title={style?.sweet ? `${style.name}: ${style.sweet[0]}–${style.sweet[1]} BPM` : ""}>
        <strong>{fmtBpm(liveBpm)}</strong> <span class="dim">BPM</span>
        {#if baseBpm && Math.abs(d.tempo - 1) > 0.0005}<span class="dim">· orig {fmtBpm(baseBpm)}</span>{/if}
      </div>
      <input type="range" min={1 - range} max={1 + range} step={0.001} value={tempoShown}
        oninput={(e) => setTempo(+e.currentTarget.value)} onchange={() => (tempoDraft = null)}
        ondblclick={resetTempo} disabled={!loaded} aria-label="Tempo" />
      <button class="plain pct num" class:warn={warnSlow} disabled={!loaded} onclick={resetTempo}
        title={warnSlow ? "Slowed more than the profile's recommended maximum — may sound warbly" : "Reset to the version's tempo"}>
        {fmtPct(tempoShown)}
      </button>
    </div>
  </div>
</section>

<style>
  .deck { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .deck.main { border-top: 2px solid color-mix(in srgb, var(--deck) 50%, var(--border)); }
  .deck:not(.main) { border-top: 2px solid color-mix(in srgb, var(--deck) 50%, var(--border)); }
  .deck.near { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--orange) 50%, transparent); }
  header { display: flex; align-items: center; gap: 10px; height: 20px; }
  .label { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-xs); font-weight: 700; letter-spacing: 0.08em; color: var(--deck); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-faint); }
  .dot.live { background: var(--red); box-shadow: 0 0 8px var(--red); animation: pulse 1.6s infinite; }
  @keyframes pulse { 50% { opacity: 0.5; } }
  .status { font-size: var(--fs-sm); color: var(--text-dim); }
  .spacer { flex: 1; }
  button.on { color: var(--preview); background: color-mix(in srgb, var(--preview) 18%, transparent); }
  .warn-chip { font-size: var(--fs-xs); color: var(--yellow); }
  .meta { display: flex; gap: 12px; align-items: center; min-height: 64px; }
  .art { width: 60px; height: 60px; flex: none; border-radius: 6px; overflow: hidden; display: grid; place-items: center;
    background: color-mix(in srgb, var(--c) 25%, var(--bg-elev-2)); color: color-mix(in srgb, var(--c) 80%, white); font-weight: 800; font-size: 15px; }
  .art img { width: 100%; height: 100%; object-fit: cover; }
  .titles { flex: 1; min-width: 0; }
  .title { font: 600 var(--fs-lg)/1.2 var(--font-display); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .main .title { font-size: 22px; }
  .artist { color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
  .ver { color: var(--text-faint); }
  .chips { display: flex; gap: 4px; margin-top: 5px; flex-wrap: wrap; }
  .times { text-align: right; }
  .remain { font: 600 26px/1 var(--font-display); }
  .remain.near { color: var(--orange); }
  .elapsed { margin-top: 4px; }
  .controls { display: flex; align-items: center; gap: 6px; margin-top: auto; min-width: 0; }
  .transport { width: 44px; height: 32px; }
  .transport.active { color: var(--deck); border-color: color-mix(in srgb, var(--deck) 60%, transparent); }
  .tempo { margin-left: auto; display: flex; align-items: center; gap: 6px; min-width: 0; flex: 1; justify-content: flex-end; }
  .tempo input { width: 140px; min-width: 50px; flex: 0 1 140px; }
  .bpm { font-size: var(--fs); white-space: nowrap; }
  .bpm strong { font-size: var(--fs-md); }
  .bpm.sweet strong { color: var(--green); }
  .bpm.outside strong { color: var(--yellow); }
  .pct { min-width: 52px; padding: 0 4px; font-size: var(--fs-sm); flex: none; }
  .pct.warn { color: var(--yellow); }
  .err { color: var(--red); }
  .small { font-size: var(--fs-sm); }
</style>

<script lang="ts">
  // Centre column: the manual "Play Next" control, the pause between songs,
  // and what's coming up. Songs never overlap.
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { MOD, fmtBpm, fmtTime, playback } from "../lib/tempo";
  import Icon from "./Icon.svelte";
  import StyleBadge from "./StyleBadge.svelte";
  import { customLabel } from "../lib/styles";

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 200);
    return () => clearInterval(t);
  });

  const p = $derived(app.perform);
  /** Something is audible in the room. */
  const playing = $derived(app.decks.some((d) => d.onAir && (d.status === "playing" || d.status === "loading")));
  const nextDeck = $derived(app.decks.find((d) => d.entryId && d.entryId === p?.nextEntryId));
  const xf = $derived(p?.crossfader ?? 0.5);
  const xfOffCenter = $derived(xf !== 0.5);
  /** Flashes the crossfader when an action was refused because of it. */
  let xfBlocked = $state(false);
  $effect(() => {
    if (!app.crossfaderBlocked) return;
    // Restart the animation on every refusal.
    xfBlocked = false;
    const raf = requestAnimationFrame(() => (xfBlocked = true));
    return () => cancelAnimationFrame(raf);
  });
  const next = $derived(app.entryRow(p?.nextEntryId));
  const nextPb = $derived(next?.row ? playback(next.row.track, next.row.version, next.entry, app.clubStyleOf) : null);
  const nextShown = $derived(next ? app.shownFor(next.row?.version, next.entry, nextPb?.target ?? null) : []);
  const inGap = $derived(!!p?.gapStartedAt && !playing);
  const gapMs = $derived(inGap && p?.gapStartedAt ? now - p.gapStartedAt : 0);
  /** Expected pause: the general pause, or with announcements, both pauses and the speech. */
  const gapTarget = $derived.by(() => {
    if (p?.autoAdvanceAt && p.gapStartedAt) return Math.max(1000, p.autoAdvanceAt - p.gapStartedAt);
    return (p?.gapSeconds ?? 8) * 1000;
  });
  const autoIn = $derived(p?.autoAdvanceAt && inGap ? Math.max(0, p.autoAdvanceAt - now) : null);

  const label = $derived.by(() => {
    if (p?.pendingNext) return "Starting after fade…";
    if (playing) return p?.armed ? "Press again to fade & play" : "Fade & Play Next";
    return "Play Next";
  });

  function setXf(x: number) {
    app.run(api.setLevels({ crossfader: x }));
  }

  async function playNext() {
    const r = await app.run(api.playNext());
    if (r === "armed") app.toast("A song is playing — press Play Next again within 3 s to fade it out and continue.", "info");
  }
</script>

<section class="transport">
  <button
    class="next"
    class:armed={p?.armed}
    class:pending={p?.pendingNext}
    class:live={playing}
    disabled={!next || (!next.row && !playing)}
    onclick={playNext}
    title="Play the next song on the free deck ({MOD}Enter). A playing song is faded out first."
  >
    <Icon name="next" size={20} />
    <span>{label}</span>
  </button>
  {#if p?.pendingNext}
    <button class="plain small" onclick={() => api.cancelPending()}>Cancel</button>
  {/if}

  {#if inGap || playing}
  <div class="gap" class:active={inGap}>
    {#if inGap}
      <!-- One line: the pause, and what auto mode does next. -->
      <div class="gap-time num">
        <span class="big" class:over={gapMs > gapTarget}>{fmtTime(gapMs)}</span>
        {#if p?.autoPhase === "announcing"}<span class="auto line" title="Announcing the next dance">announcing…</span>
        {:else if p?.autoPhase === "held"}<span class="auto held line" title="Auto mode waits here: press Play Next">waits for Play Next</span>
        {:else if autoIn != null}<span class="auto line">next song in {Math.ceil(autoIn / 1000)}s</span>
        {:else}<span class="dim line">pause · target {Math.round(gapTarget / 1000)}s</span>{/if}
      </div>
      <div class="bar"><div style:width="{Math.min(100, (gapMs / gapTarget) * 100)}%"></div></div>
    {:else if playing}
      {@const note = p?.autoAdvance ? (p.autoPhase === "held" ? "Auto mode stops before the next song." : "Auto mode starts the next song after the pause.") : "Next song starts only when you press Play Next."}
      <div class="dim small line" title={note}>{note}</div>
    {/if}
  </div>
  {/if}

  <div class="upnext">
    <div class="section-title">Up next</div>
    {#if next && next.entry.kind === "pause"}
      <div class="un-title"><Icon name="pause" size={12} /> {next.entry.label || "Pause"}</div>
      <div class="un-artist dim small num">
        {Math.round(next.entry.pauseBefore ?? p?.gapSeconds ?? 0)} s{#if next.entry.announcement}{" · "}“{next.entry.announcement}”{/if}{#if next.entry.hold}{" · "}stops here{/if}
      </div>
    {:else if next}
      {#if next.row}
        <div class="un-title" title={next.row.track.title}>
          <StyleBadge styles={nextShown} label={customLabel(next.row?.version, next.entry)} />
          <span class="line">{next.row.track.title}</span>
        </div>
        <div class="un-artist dim small num">
          {next.row.track.artist}{#if nextPb?.target}{" · "}{fmtBpm(nextPb.target)} BPM{/if}{#if nextDeck}<span class="ondeck">{" · "}deck {nextDeck.deck === 0 ? "A" : "B"}</span>{/if}
        </div>
      {:else}
        <div class="un-style"><StyleBadge styles={nextShown} label={customLabel(undefined, next.entry)} big /></div>
        <div class="slot">Pick a song for this slot</div>
      {/if}
    {:else}
      <div class="dim small">{app.current ? "End of setlist" : "No setlist open — add songs to start a live set"}</div>
    {/if}
  </div>

  <div class="xfader" class:blocked={xfBlocked} onanimationend={() => (xfBlocked = false)}>
    <button class="plain xl" class:lit={xf < 0.5} title="Crossfader all the way to deck A" onclick={() => setXf(0)}>A</button>
    <span class="track">
      <input type="range" min="0" max="1" step="0.01" value={xf} aria-label="Crossfader"
        title="Crossfader: which deck the room hears. Snaps to the center. Play Next, Go Live and auto mode need it centered or on the side of the deck they start."
        oninput={(e) => setXf(+e.currentTarget.value)} />
      <button class="plain notch" class:on={xf === 0.5} title="Center the crossfader (the room hears both decks)" aria-label="Center the crossfader"
        onclick={() => setXf(0.5)}></button>
    </span>
    <button class="plain xl" class:lit={xf > 0.5} title="Crossfader all the way to deck B" onclick={() => setXf(1)}>B</button>
  </div>
  {#if xfOffCenter && p?.autoAdvance}
    <button class="plain xf-hint small" title="Auto mode only starts songs when the crossfader is centered" onclick={() => setXf(0.5)}>Auto mode: center the fader</button>
  {/if}

  <div class="row">
    <button class="small" disabled={!next} onclick={() => api.skipNext()} title="Skip the up-next song (stays in the set, greyed out)">
      <Icon name="skip" size={13} /> Skip
    </button>
    {#if app.tts?.state === "ready"}
      <button class="small" disabled={!next} onclick={() => app.run(api.announceNext())} title="Announce the next dance now" aria-label="Announce the next dance">
        <Icon name="speaker" size={13} />
      </button>
    {/if}
    <button class="small" class:on={p?.autoAdvance} onclick={() => api.setAutoAdvance(!p?.autoAdvance)}
      title={app.settings?.announce.enabled ? "Auto mode: after a song, pause, announce the next dance, pause, and start it" : "Auto mode: start the next song after the pause"}>
      <Icon name="auto" size={13} /> Auto
    </button>
  </div>
</section>

<style>
  .transport { display: flex; flex-direction: column; align-items: stretch; gap: 8px; padding: 10px 12px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .next { flex: none; height: 52px; font-size: var(--fs-md); font-weight: 700; gap: 10px; border-radius: 10px;
    background: linear-gradient(#2fc25a, #239a47); border: 1px solid #36d466; color: white; text-shadow: 0 1px 1px rgba(0,0,0,.3); }
  .next:hover:not(:disabled) { background: linear-gradient(#36cf63, #27a64e); }
  .next.live, .next.live:hover:not(:disabled) { background: linear-gradient(#3a3d44, #2e3036); border-color: var(--border-strong); color: var(--text); }
  .next.armed, .next.armed:hover:not(:disabled) { background: linear-gradient(#ff9f0a, #d98200); border-color: #ffb340; color: #1a1000; animation: blink 0.6s infinite alternate; }
  .next.pending, .next.pending:hover:not(:disabled) { background: var(--bg-elev-3); color: var(--text-dim); }
  @keyframes blink { to { filter: brightness(1.25); } }
  .small { font-size: var(--fs-sm); }
  /* The first to give up space (the column's height is fixed): never the up-next song. */
  .gap { flex: 0 1 auto; min-height: 0; overflow: hidden; display: flex; flex-direction: column; gap: 4px; justify-content: center; }
  .line { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .gap-time { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .gap-time .big { flex: none; }
  .big { font: 600 24px/1 var(--font-display); color: var(--green); }
  .big.over { color: var(--yellow); }
  .bar { height: 4px; background: var(--bg-elev-3); border-radius: 2px; overflow: hidden; }
  .bar div { height: 100%; background: var(--green); transition: width 0.2s linear; }
  .auto { color: var(--teal); font-size: var(--fs-sm); }
  .auto.held { color: var(--yellow); }
  .upnext { flex: 1 0 auto; min-height: 0; overflow: hidden; display: flex; flex-direction: column; gap: 2px; padding-top: 6px; border-top: 1px solid var(--border); min-width: 0; }
  .upnext > * { flex: none; }
  .un-style { margin: 2px 0; }
  .un-title { font-weight: 600; display: flex; align-items: center; gap: 6px; min-width: 0; }
  .un-title > :global(*) { flex: none; }
  .un-title .line { flex: 1; }
  .un-artist { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .slot { color: var(--yellow); font-size: var(--fs-sm); }
  .row { flex: none; display: flex; gap: 6px; }
  .row button { flex: 1; }
  button.on { color: var(--teal); border-color: color-mix(in srgb, var(--teal) 50%, transparent); }
  .ondeck { color: var(--text-faint); }
  .xfader { flex: none; display: flex; align-items: center; gap: 6px; }
  .track { flex: 1; min-width: 0; position: relative; display: flex; padding-bottom: 7px; }
  .track input { flex: 1; min-width: 0; }
  .notch { position: absolute; left: 50%; bottom: -3px; transform: translateX(-50%); width: 18px; height: 10px; padding: 0; min-height: 0;
    border: none; background: none; }
  .notch::after { content: ""; position: absolute; left: 50%; top: 2px; transform: translateX(-50%);
    border: 4px solid transparent; border-bottom: 5px solid var(--text-faint); border-top: none; }
  .notch:hover::after, .notch.on::after { border-bottom-color: var(--text); }
  .xl { font-weight: 800; font-size: var(--fs-sm); color: var(--text-faint); width: 18px; min-width: 18px; padding: 2px 0; text-align: center; }
  .xl.lit { color: var(--text); }
  .xf-hint { align-self: flex-start; padding: 0; }
  .xfader.blocked { border-radius: 6px; animation: xf-error 0.2s 6 alternate; }
  @keyframes xf-error { to { background: color-mix(in srgb, var(--red) 45%, transparent); } }
  .xf-hint { color: var(--yellow); margin-top: -4px; }
</style>

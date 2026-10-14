<script lang="ts">
  // Centre column: the manual "Play Next" control, the pause between songs,
  // and what's coming up. Songs never overlap.
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { MOD, fmtBpm, fmtTime, playback, styleOf } from "../lib/tempo";
  import Icon from "./Icon.svelte";
  import StyleBadge from "./StyleBadge.svelte";

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 200);
    return () => clearInterval(t);
  });

  const p = $derived(app.perform);
  const main = $derived(app.decks[0]);
  const playing = $derived(main.status === "playing" || main.status === "loading");
  const next = $derived(app.entryRow(p?.nextEntryId));
  const nextStyle = $derived(next ? styleOf(next.row?.version, next.entry) : null);
  const nextPb = $derived(next?.row ? playback(next.row.track, next.row.version, next.entry, app.profile) : null);
  const inGap = $derived(!!p?.gapStartedAt && !playing);
  const gapMs = $derived(inGap && p?.gapStartedAt ? now - p.gapStartedAt : 0);
  const gapTarget = $derived((p?.gapSeconds ?? 8) * 1000);
  const autoIn = $derived(p?.autoAdvanceAt && inGap ? Math.max(0, p.autoAdvanceAt - now) : null);

  const label = $derived.by(() => {
    if (p?.pendingNext) return "Starting after fade…";
    if (playing) return p?.armed ? "Press again to fade & play" : "Fade & Play Next";
    return "Play Next";
  });

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
    title="Play the next song ({MOD}Enter). Songs never overlap: a playing song is faded out first."
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
      <div class="gap-time num">
        <span class="big" class:over={gapMs > gapTarget}>{fmtTime(gapMs)}</span>
        <span class="dim">pause · target {Math.round(gapTarget / 1000)}s</span>
      </div>
      <div class="bar"><div style:width="{Math.min(100, (gapMs / gapTarget) * 100)}%"></div></div>
      {#if autoIn != null}<div class="auto num">Auto-play in {Math.ceil(autoIn / 1000)}s</div>{/if}
    {:else if playing}
      <div class="dim small">Next song starts only when you press Play Next.</div>
    {/if}
  </div>
  {/if}

  <div class="upnext">
    <div class="section-title">Up next</div>
    {#if next}
      <div class="un-style"><StyleBadge style={nextStyle} big /></div>
      {#if next.row}
        <div class="un-title" title={next.row.track.title}>{next.row.track.title}</div>
        <div class="un-artist dim">{next.row.track.artist}</div>
        {#if nextPb?.target}<div class="dim small num">{fmtBpm(nextPb.target)} BPM</div>{/if}
      {:else}
        <div class="slot">Pick a song for this slot</div>
      {/if}
    {:else}
      <div class="dim small">{app.current ? "End of setlist" : "No setlist open — add songs to start a live set"}</div>
    {/if}
  </div>

  <div class="row">
    <button class="small" disabled={!next} onclick={() => api.skipNext()} title="Skip the up-next song (stays in the set, greyed out)">
      <Icon name="skip" size={13} /> Skip
    </button>
    <button class="small" class:on={p?.autoAdvance} onclick={() => api.setAutoAdvance(!p?.autoAdvance)}
      title="Automatically start the next song after the pause">
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
  .gap { flex: none; min-height: 40px; display: flex; flex-direction: column; gap: 4px; justify-content: center; }
  .gap-time { display: flex; align-items: baseline; gap: 8px; }
  .big { font: 600 24px/1 var(--font-display); color: var(--green); }
  .big.over { color: var(--yellow); }
  .bar { height: 4px; background: var(--bg-elev-3); border-radius: 2px; overflow: hidden; }
  .bar div { height: 100%; background: var(--green); transition: width 0.2s linear; }
  .auto { color: var(--teal); font-size: var(--fs-sm); }
  .upnext { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; gap: 2px; padding-top: 6px; border-top: 1px solid var(--border); min-width: 0; }
  .upnext > * { flex: none; }
  .un-style { margin: 2px 0; }
  .un-title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .un-artist { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .slot { color: var(--yellow); font-size: var(--fs-sm); }
  .row { flex: none; display: flex; gap: 6px; }
  .row button { flex: 1; }
  button.on { color: var(--teal); border-color: color-mix(in srgb, var(--teal) 50%, transparent); }
</style>

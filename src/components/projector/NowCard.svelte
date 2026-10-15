<script lang="ts">
  // The dominant "what are we dancing" card: the current song, or the next
  // dance during the pause between songs.
  import { fit } from "../../lib/fit";
  import { fade, fly } from "svelte/transition";
  import { artwork } from "../../lib/artwork";
  import { projStyle } from "../../lib/styles";
  import type { ProjEntry, ProjectorPayload } from "../../lib/types";

  let { payload, progress, compact = false }: { payload: ProjectorPayload; progress: number | null; compact?: boolean } =
    $props();

  const s = $derived(payload.settings);
  const gap = $derived(payload.inGap && !payload.playing);
  const revealNext = $derived(gap && s.revealNextInGap && !!payload.next);
  const entry = $derived<ProjEntry | null>(revealNext ? payload.next : payload.current);
  const label = $derived(revealNext ? "Next dance" : gap ? "Just played" : "Now dancing");
  const st = $derived(entry ? projStyle(payload, entry) : null);
  const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  let art = $state<string | null>(null);
  $effect(() => {
    const id = !revealNext && s.showArtwork && inTauri ? entry?.recordingId : null;
    art = null;
    if (id) artwork(id).then((a) => { if (entry?.recordingId === id) art = a; });
  });
  const key = $derived(`${revealNext ? "next" : gap ? "past" : "now"}:${entry?.id ?? ""}`);

  function fmtBpm(b: number | null) {
    return b == null ? "" : `${Math.round(b)} BPM`;
  }
</script>

<div class="card" class:compact>
  {#key key}
    <div class="inner" class:dim={gap && !revealNext} class:with-art={!!art} in:fly={{ y: 24, duration: 420, delay: 120 }} out:fade={{ duration: 180 }}>
      {#if art}<img class="art" src={art} alt="" in:fade={{ duration: 300 }} />{/if}
      {#if entry}<div class="text">
        <div class="label" style:--c={st?.color}>
          {#if !gap}<span class="live-dot"></span>{/if}
          {label}
        </div>
        {#if st?.name}<div class="style" style:--c={st.color} use:fit={{ lines: 2 }}>{st.name}</div>{/if}
        {#if entry.tags.length}
          <div class="tags">
            {#each entry.tags as t (t.id)}
              <span class="tag" style:--c={t.color}>{t.name}</span>
            {/each}
          </div>
        {/if}
        {#if revealNext}
          <div class="hint">Find a partner!</div>
        {:else}
          {#if entry.title}<div class="song" use:fit={{ lines: 2 }}>{entry.title}</div>{/if}
          {#if s.showArtist && entry.artist}<div class="artist" use:fit>{entry.artist}</div>{/if}
          {#if s.showBpm && entry.bpm}<div class="bpm">{fmtBpm(entry.bpm)}</div>{/if}
        {/if}
      </div>{/if}
    </div>
  {/key}
  {#if !gap && progress != null && entry}
    <div class="progress" style:--c={st?.color}><div class="bar" style:width="{progress * 100}%"></div></div>
  {/if}
</div>

<style>
  .card {
    position: relative;
    display: grid;
    /* Too tall for the card: overflow downward only, never above the label. */
    align-content: safe center;
    min-height: 0;
    /* Sizes below follow the card's width (cqi), so long style names fit. */
    container-type: inline-size;
    /* The bottom padding keeps the text clear of the progress bar. */
    padding: calc(2.5 * var(--u)) calc(3 * var(--u)) calc(3.4 * var(--u));
    border-radius: calc(1.6 * var(--u));
    background: var(--p-bg-2);
    border: 1px solid var(--p-line);
    overflow: hidden;
  }
  .inner {
    grid-area: 1 / 1;
  }
  .inner.with-art {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: calc(3 * var(--u));
  }
  .inner .text {
    min-width: 0;
  }
  .art {
    grid-column: 2;
    grid-row: 1;
    /* At most a third of the card, so the text keeps room. */
    width: min(calc(22 * var(--u)), 32cqi);
    height: min(calc(22 * var(--u)), 32cqi);
    object-fit: cover;
    border-radius: calc(1 * var(--u));
    box-shadow: 0 calc(0.6 * var(--u)) calc(3 * var(--u)) rgba(0, 0, 0, 0.45);
  }
  .compact .art {
    width: min(calc(12 * var(--u)), 24cqi);
    height: min(calc(12 * var(--u)), 24cqi);
  }
  .inner.dim {
    opacity: 0.55;
  }
  .label {
    display: flex;
    align-items: center;
    gap: calc(0.8 * var(--u));
    font-size: calc(1.9 * var(--u));
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--p-text-2);
  }
  .live-dot {
    width: calc(1.1 * var(--u));
    height: calc(1.1 * var(--u));
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 calc(1.2 * var(--u)) var(--c);
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }
  .style {
    margin-top: calc(0.6 * var(--u));
    /* Large; a long name shrinks to fit two lines (use:fit). */
    font-size: calc(9 * var(--u));
    overflow: hidden;
    line-height: 1;
    font-weight: 800;
    letter-spacing: -0.035em;
    color: var(--ink-c);
    text-wrap: balance;
  }
  .compact .style {
    font-size: calc(6.4 * var(--u));
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: calc(0.8 * var(--u));
    margin-top: calc(1.6 * var(--u));
  }
  .tag {
    padding: calc(0.4 * var(--u)) calc(1.2 * var(--u));
    border-radius: 999px;
    font-size: calc(2.2 * var(--u));
    font-weight: 750;
    color: #0b0b0d;
    background: var(--c);
  }
  .song {
    margin-top: calc(2.2 * var(--u));
    font-size: calc(4 * var(--u));
    font-weight: 700;
    line-height: 1.12;
    letter-spacing: -0.015em;
    text-wrap: balance;
  }
  .compact .song {
    font-size: calc(3.2 * var(--u));
  }
  .artist {
    margin-top: calc(0.5 * var(--u));
    font-size: calc(2.8 * var(--u));
    font-weight: 500;
    color: var(--p-text-2);
  }
  .compact .artist {
    font-size: calc(2.4 * var(--u));
  }
  .bpm {
    margin-top: calc(0.8 * var(--u));
    font-size: calc(2.2 * var(--u));
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--p-text-3);
  }
  .hint {
    margin-top: calc(2 * var(--u));
    font-size: calc(3.4 * var(--u));
    font-weight: 600;
    color: var(--p-text);
  }
  .progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: calc(0.6 * var(--u));
    background: color-mix(in srgb, var(--c) 18%, transparent);
  }
  .bar {
    height: 100%;
    background: var(--c);
    transition: width 0.5s linear;
  }
</style>

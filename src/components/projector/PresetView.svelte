<script lang="ts">
  // Preset mode: the whole setlist at once, in columns (up to 3 × 18 songs fit
  // on screen; longer sets scroll sideways to keep the current song in view).
  // A slim banner above shows what is dancing now, or the next dance during
  // the pause.
  import { fade } from "svelte/transition";
  import { artwork } from "../../lib/artwork";
  import { projStyle } from "../../lib/styles";
  import type { ProjEntry, ProjectorPayload } from "../../lib/types";

  let { payload, progress }: { payload: ProjectorPayload; progress: number | null } = $props();

  /** Columns visible at once, and the most rows per column. */
  const COLUMNS = 3;
  const MAX_ROWS = 18;
  /** Fewer rows than this per column look sparse; rows grow instead. */
  const MIN_ROWS = 8;

  const s = $derived(payload.settings);
  const gap = $derived(payload.inGap && !payload.playing);
  const currentId = $derived(gap ? null : (payload.current?.id ?? null));
  const nextId = $derived(payload.next?.id ?? null);
  const revealNext = $derived(gap && s.revealNextInGap && !!payload.next);
  const banner = $derived<ProjEntry | null>(revealNext ? payload.next : gap ? null : payload.current);
  const bannerStyle = $derived(banner ? projStyle(payload, banner) : null);

  const rows = $derived(Math.min(MAX_ROWS, Math.max(MIN_ROWS, Math.ceil(payload.entries.length / COLUMNS))));
  const columns = $derived.by(() => {
    const out: { e: ProjEntry; n: number | null }[][] = [];
    let n = 0;
    payload.entries.forEach((e, i) => {
      if (i % rows === 0) out.push([]);
      out[out.length - 1].push({ e, n: e.status === "skipped" ? null : ++n });
    });
    return out;
  });

  // Keep the current song's column on screen (as the middle one when scrolling).
  let scroller: HTMLDivElement | undefined = $state();
  let firstShow = true;
  $effect(() => {
    const id = currentId ?? nextId;
    const el = scroller;
    void columns.length;
    if (!el || columns.length <= COLUMNS) return;
    const col = Math.max(0, payload.entries.findIndex((e) => e.id === id)) / rows;
    const colWidth = el.scrollWidth / columns.length;
    const left = Math.max(0, (Math.floor(col) - 1) * colWidth);
    el.scrollTo({ left, behavior: firstShow ? "instant" : "smooth" });
    firstShow = false;
  });

  const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  let art = $state<string | null>(null);
  $effect(() => {
    const id = !revealNext && s.showArtwork && inTauri ? banner?.trackId : null;
    art = null;
    if (id) artwork(id).then((a) => { if (banner?.trackId === id) art = a; });
  });
</script>

<div class="preset" style:--rows={rows}>
  {#if banner && bannerStyle}
    {#key banner.id + (revealNext ? ":next" : "")}
      <section class="banner" style:--c={bannerStyle.color} in:fade={{ duration: 300 }}>
        {#if art}<img class="art" src={art} alt="" />{/if}
        <div class="label">{#if !revealNext}<span class="live-dot"></span>{/if}{revealNext ? "Next dance" : "Now dancing"}</div>
        <div class="b-style">{bannerStyle.name}</div>
        {#if revealNext}
          <div class="b-song dim">Find a partner!</div>
        {:else if banner.title}
          <div class="b-song">{banner.title}{#if s.showArtist && banner.artist}<span class="dim">{" · "}{banner.artist}</span>{/if}</div>
        {/if}
        {#each banner.tags as t (t.id)}<span class="tag" style:--t={t.color}>{t.name}</span>{/each}
        {#if !gap && progress != null}<div class="progress"><div class="bar" style:width="{progress * 100}%"></div></div>{/if}
      </section>
    {/key}
  {/if}
  <div class="columns" bind:this={scroller}>
    {#each columns as col, ci (ci)}
      <ol class="col">
        {#each col as { e, n } (e.id)}
          {@const st = projStyle(payload, e)}
          <li
            style:--c={st.color}
            class:played={e.status === "played"}
            class:skipped={e.status === "skipped"}
            class:current={e.id === currentId}
            class:next={gap && e.id === nextId}
          >
            <span class="n">{n ?? ""}</span>
            <span class="stripe"></span>
            <span class="txt">
              <span class="style">{st.name || "—"}</span>
              {#if e.title}<span class="song">{e.title}{#if s.showArtist && e.artist}{" · "}{e.artist}{/if}</span>{/if}
            </span>
            {#each e.tags as t (t.id)}<span class="tag" style:--t={t.color}>{t.name}</span>{/each}
          </li>
        {/each}
      </ol>
    {/each}
  </div>
</div>

<style>
  .preset {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: calc(1.6 * var(--u));
    min-height: 0;
  }
  .banner {
    flex: none;
    position: relative;
    display: flex;
    align-items: center;
    gap: calc(2 * var(--u));
    padding: calc(1.2 * var(--u)) calc(2.4 * var(--u));
    border-radius: calc(1.4 * var(--u));
    background: color-mix(in srgb, var(--c) 14%, var(--p-bg-2));
    border: 1px solid color-mix(in srgb, var(--c) 45%, var(--p-line));
    overflow: hidden;
    white-space: nowrap;
  }
  .art {
    width: calc(6.5 * var(--u));
    height: calc(6.5 * var(--u));
    object-fit: cover;
    border-radius: calc(0.6 * var(--u));
    flex: none;
  }
  .label {
    display: flex;
    align-items: center;
    gap: calc(0.7 * var(--u));
    font-size: calc(1.5 * var(--u));
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--p-text-2);
    flex: none;
  }
  .live-dot {
    width: calc(0.9 * var(--u));
    height: calc(0.9 * var(--u));
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 calc(1 * var(--u)) var(--c);
  }
  .b-style {
    font-size: calc(4 * var(--u));
    font-weight: 800;
    letter-spacing: -0.02em;
    color: var(--ink-c);
    flex: none;
  }
  .b-song {
    font-size: calc(2.4 * var(--u));
    font-weight: 600;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dim {
    color: var(--p-text-2);
    font-weight: 500;
  }
  .progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: calc(0.45 * var(--u));
    background: color-mix(in srgb, var(--c) 20%, transparent);
  }
  .bar {
    height: 100%;
    background: var(--c);
    transition: width 0.25s linear;
  }
  .columns {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: calc((100% - 2 * 1.6 * var(--u)) / 3);
    column-gap: calc(1.6 * var(--u));
    overflow-x: auto;
    scrollbar-width: none;
  }
  .columns::-webkit-scrollbar {
    display: none;
  }
  .col {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-rows: repeat(var(--rows), minmax(0, 1fr));
    min-height: 0;
  }
  li {
    /* Text scales with the row height (rows grow when the set is short). */
    --row: calc((100vh - 18 * var(--u)) / var(--rows));
    display: grid;
    grid-template-columns: calc(2.4 * var(--u)) calc(0.35 * var(--u)) minmax(0, 1fr) auto;
    align-items: center;
    column-gap: calc(0.9 * var(--u));
    padding: 0 calc(0.8 * var(--u));
    border-radius: calc(0.7 * var(--u));
    font-size: min(calc(1.9 * var(--u)), calc(var(--row) * 0.42));
    min-height: 0;
    transition:
      background 400ms,
      opacity 400ms;
  }
  .n {
    text-align: right;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--p-text-3);
    font-size: 0.85em;
  }
  .stripe {
    align-self: stretch;
    margin: 18% 0;
    border-radius: 999px;
    background: var(--c);
  }
  .txt {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .style {
    font-weight: 750;
    color: var(--ink-c);
  }
  .song {
    margin-left: 0.6em;
    color: var(--p-text-2);
    font-size: 0.82em;
  }
  .tag {
    flex: none;
    padding: 0.1em 0.6em;
    border-radius: 999px;
    font-size: 0.7em;
    font-weight: 750;
    color: #0b0b0d;
    background: var(--t);
    white-space: nowrap;
  }
  li.played {
    opacity: 0.38;
  }
  li.skipped {
    opacity: 0.25;
  }
  li.skipped .txt {
    text-decoration: line-through;
  }
  li.current {
    opacity: 1;
    background: color-mix(in srgb, var(--c) 24%, transparent);
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--c) 75%, transparent);
  }
  li.current .n,
  li.current .song {
    color: var(--p-text);
  }
  li.next {
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--c) 55%, transparent);
  }
</style>

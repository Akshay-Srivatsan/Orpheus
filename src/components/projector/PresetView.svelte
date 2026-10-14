<script lang="ts">
  // Preset mode: the whole setlist, with the current position highlighted
  // and kept in view.
  import { projStyle } from "../../lib/styles";
  import type { ProjectorPayload } from "../../lib/types";
  import NowCard from "./NowCard.svelte";

  let { payload, progress }: { payload: ProjectorPayload; progress: number | null } = $props();

  let list: HTMLOListElement | undefined = $state();
  const gap = $derived(payload.inGap && !payload.playing);
  const currentId = $derived(gap ? null : (payload.current?.id ?? null));
  const nextId = $derived(payload.next?.id ?? null);
  const anchorId = $derived(currentId ?? nextId);
  const hasCurrent = $derived(!!(payload.current || payload.next));

  // Keep the anchor row about a quarter of the way down, so a couple of past
  // songs stay visible above it and the upcoming ones below.
  let positioned = false;
  $effect(() => {
    const id = anchorId;
    void payload.entries.length;
    const el = list;
    if (!el) return;
    const raf = requestAnimationFrame(() => {
      const row = id ? el.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`) : null;
      const top = row ? row.offsetTop - el.clientHeight * 0.22 : 0;
      // Jump on first show; glide afterwards.
      el.scrollTo({ top: Math.max(0, top), behavior: positioned ? "smooth" : "instant" });
      positioned = true;
    });
    return () => cancelAnimationFrame(raf);
  });

  // Number rows by position among non-skipped entries.
  const numbers = $derived.by(() => {
    let n = 0;
    return payload.entries.map((e) => (e.status === "skipped" ? null : ++n));
  });
</script>

<div class="preset" class:full={!hasCurrent}>
  {#if hasCurrent}
    <NowCard {payload} {progress} compact />
  {/if}
  <div class="setlist">
    <ol bind:this={list}>
      {#each payload.entries as e, i (e.id)}
        {@const st = projStyle(payload, e)}
        <li
          data-id={e.id}
          style:--c={st.color}
          class:played={e.status === "played"}
          class:skipped={e.status === "skipped"}
          class:current={e.id === currentId}
          class:next={gap && e.id === nextId}
        >
          <span class="n">{numbers[i] ?? ""}</span>
          <span class="stripe"></span>
          <span class="style">{st.name || "—"}</span>
          <span class="song">
            <span class="song-text">
              {#if e.title}<span class="title">{e.title}</span>{/if}
              {#if payload.settings.showArtist && e.artist}<span class="artist"> · {e.artist}</span>{/if}
            </span>
            {#each e.tags as t (t.id)}<span class="tag" style:--t={t.color}>{t.name}</span>{/each}
          </span>
        </li>
      {/each}
    </ol>
  </div>
</div>

<style>
  .preset {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.55fr);
    gap: calc(2.4 * var(--u));
    min-height: 0;
  }
  .preset.full {
    grid-template-columns: minmax(0, 1fr);
  }
  .setlist {
    position: relative;
    min-height: 0;
    border-radius: calc(1.6 * var(--u));
    background: var(--p-bg-2);
    border: 1px solid var(--p-line);
    overflow: hidden;
  }
  /* Fade the list out at the top and bottom edges. */
  .setlist::before,
  .setlist::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    height: calc(3 * var(--u));
    z-index: 1;
    pointer-events: none;
  }
  .setlist::before {
    top: 0;
    background: linear-gradient(var(--p-bg-2), transparent);
  }
  .setlist::after {
    bottom: 0;
    background: linear-gradient(transparent, var(--p-bg-2));
  }
  ol {
    list-style: none;
    margin: 0;
    padding: calc(2 * var(--u)) calc(1.4 * var(--u));
    height: 100%;
    overflow-y: auto;
    scrollbar-width: none;
  }
  ol::-webkit-scrollbar {
    display: none;
  }
  li {
    display: grid;
    grid-template-columns: calc(2.6 * var(--u)) calc(0.45 * var(--u)) calc(19 * var(--u)) minmax(0, 1fr);
    align-items: center;
    column-gap: calc(1.2 * var(--u));
    padding: calc(0.75 * var(--u)) calc(1 * var(--u));
    border-radius: calc(0.9 * var(--u));
    font-size: calc(2 * var(--u));
    transition:
      background 400ms,
      opacity 400ms;
  }
  .n {
    text-align: right;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--p-text-3);
  }
  .stripe {
    align-self: stretch;
    border-radius: 999px;
    background: var(--c);
  }
  .style {
    font-weight: 750;
    color: var(--ink-c);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .song {
    display: flex;
    align-items: center;
    gap: calc(0.6 * var(--u));
    min-width: 0;
  }
  .song-text {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title {
    font-weight: 600;
    color: var(--p-text);
  }
  .artist {
    color: var(--p-text-2);
  }
  .tag {
    flex: none;
    padding: calc(0.15 * var(--u)) calc(0.8 * var(--u));
    border-radius: 999px;
    font-size: calc(1.5 * var(--u));
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
  li.skipped .style,
  li.skipped .song {
    text-decoration: line-through;
  }
  li.current {
    opacity: 1;
    background: color-mix(in srgb, var(--c) 22%, transparent);
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--c) 70%, transparent);
  }
  li.current .n {
    color: var(--ink-c);
  }
  li.next {
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--c) 55%, transparent);
  }
</style>

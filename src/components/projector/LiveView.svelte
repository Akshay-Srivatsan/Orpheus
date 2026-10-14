<script lang="ts">
  // Live mode: the current dance dominates; recent songs alongside.
  import { flip } from "svelte/animate";
  import { fade } from "svelte/transition";
  import type { ProjectorPayload } from "../../lib/types";
  import NowCard from "./NowCard.svelte";

  let { payload, progress }: { payload: ProjectorPayload; progress: number | null } = $props();

  const s = $derived(payload.settings);
  const gap = $derived(payload.inGap && !payload.playing);
  // "Up next" while a song plays (during the pause the main card shows it).
  const upNext = $derived(s.showNext && !gap && payload.next ? payload.next : null);
  const history = $derived.by(() => {
    // During the pause the main card shows the last song (unless revealing the next), so skip it here.
    const showingLast = gap && !(s.revealNextInGap && payload.next);
    const h = showingLast && payload.current ? payload.history.filter((e) => e.id !== payload.current!.id) : payload.history;
    return h.slice(0, s.historyCount);
  });
</script>

<div class="live" class:no-side={!upNext && history.length === 0}>
  <NowCard {payload} {progress} />
  {#if upNext || history.length}
    <aside>
      {#if upNext}
        <section class="next" style:--c={upNext.color} transition:fade={{ duration: 250 }}>
          <div class="h">Up next</div>
          <div class="next-style">{upNext.styleName}</div>
          {#if upNext.tags.length}
            <div class="next-tags">{upNext.tags.map((t) => t.name).join(" · ")}</div>
          {/if}
        </section>
      {/if}
      {#if history.length}
        <section class="recent">
          <div class="h">Recently played</div>
          <ol>
            {#each history as e (e.id)}
              <li style:--c={e.color} animate:flip={{ duration: 350 }} in:fade={{ duration: 300 }}>
                <span class="stripe"></span>
                <span class="txt">
                  <span class="r-style">{e.styleName}</span>
                  <span class="r-title">{e.title}</span>
                </span>
              </li>
            {/each}
          </ol>
        </section>
      {/if}
    </aside>
  {/if}
</div>

<style>
  .live {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 1.75fr) minmax(0, 1fr);
    gap: calc(2.4 * var(--u));
    min-height: 0;
  }
  .live.no-side {
    grid-template-columns: minmax(0, 1fr);
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: calc(2.4 * var(--u));
    min-height: 0;
  }
  section {
    border-radius: calc(1.6 * var(--u));
    background: var(--p-bg-2);
    border: 1px solid var(--p-line);
    padding: calc(2 * var(--u)) calc(2.4 * var(--u));
  }
  .h {
    font-size: calc(1.7 * var(--u));
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--p-text-2);
  }
  .next {
    position: relative;
    overflow: hidden;
  }
  .next::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: calc(0.6 * var(--u));
    background: var(--c);
  }
  .next-style {
    margin-top: calc(0.6 * var(--u));
    font-size: calc(4.2 * var(--u));
    font-weight: 800;
    line-height: 1.05;
    letter-spacing: -0.02em;
    color: var(--ink-c);
  }
  .next-tags {
    margin-top: calc(0.6 * var(--u));
    font-size: calc(2 * var(--u));
    font-weight: 650;
    color: var(--p-text);
  }
  .recent {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  ol {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    /* Fade out whatever doesn't fit instead of cutting a row in half. */
    -webkit-mask-image: linear-gradient(#000 calc(100% - 3 * var(--u)), transparent);
    mask-image: linear-gradient(#000 calc(100% - 3 * var(--u)), transparent);
    list-style: none;
    margin: calc(1.2 * var(--u)) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: calc(1.2 * var(--u));
  }
  li {
    display: flex;
    gap: calc(1.2 * var(--u));
    align-items: stretch;
  }
  .stripe {
    flex: none;
    width: calc(0.5 * var(--u));
    border-radius: 999px;
    background: var(--c);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .r-style {
    font-size: calc(2.3 * var(--u));
    font-weight: 750;
    color: var(--ink-c);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .r-title {
    font-size: calc(2.1 * var(--u));
    font-weight: 500;
    color: var(--p-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>

<script lang="ts">
  // Live mode: the current dance dominates; recent songs alongside, and
  // optionally what's coming up (queued songs and style slots).
  import { fit } from "../../lib/fit";
  import { flip } from "svelte/animate";
  import { fade } from "svelte/transition";
  import { projStyle } from "../../lib/styles";
  import type { ProjectorPayload } from "../../lib/types";
  import NowCard from "./NowCard.svelte";

  let { payload, progress }: { payload: ProjectorPayload; progress: number | null } = $props();

  const s = $derived(payload.settings);
  const gap = $derived(payload.inGap && !payload.playing);
  // "Up next" while a song plays (during the pause the main card shows it).
  const revealing = $derived(gap && s.revealNextInGap && !!payload.next);
  /** Queued songs / slots after the one the main card shows. */
  const upcoming = $derived.by(() => {
    const n = s.upcomingCount ?? 0;
    if (!n) return [];
    const anchor = revealing ? payload.next?.id : (payload.current?.id ?? null);
    const i = anchor ? payload.entries.findIndex((e) => e.id === anchor) : -1;
    return payload.entries.slice(i + 1).filter((e) => e.status === "pending").slice(0, n);
  });
  const upNext = $derived(!upcoming.length && s.showNext && !gap && payload.next ? payload.next : null);
  const history = $derived.by(() => {
    // During the pause the main card shows the last song (unless revealing the next), so skip it here.
    const showingLast = gap && !(s.revealNextInGap && payload.next);
    const h = showingLast && payload.current ? payload.history.filter((e) => e.id !== payload.current!.id) : payload.history;
    return h.slice(0, s.historyCount);
  });
</script>

<div class="live" class:no-side={!upNext && !upcoming.length && history.length === 0}>
  <NowCard {payload} {progress} />
  {#if upNext || upcoming.length || history.length}
    <aside>
      {#if upNext}
        {@const st = projStyle(payload, upNext)}
        <section class="next" style:--c={st.color} transition:fade={{ duration: 250 }}>
          <div class="h">Up next</div>
          <div class="next-style" use:fit={{ lines: 2 }}>{st.name}</div>
          {#if upNext.tags.length}
            <div class="next-tags">{upNext.tags.map((t) => t.name).join(" · ")}</div>
          {/if}
        </section>
      {/if}
      {#if upcoming.length}
        <section class="recent coming">
          <div class="h">Coming up</div>
          <ol>
            {#each upcoming as e (e.id)}
              {@const st = projStyle(payload, e)}
              <li style:--c={st.color} animate:flip={{ duration: 350 }} in:fade={{ duration: 300 }}>
                <span class="stripe"></span>
                <span class="txt" use:fit={{ min: 0.45 }}>
                  <span class="r-style">{st.name || "—"}</span>
                  {#if e.title}<span class="r-title">{e.title}</span>{/if}
                </span>
              </li>
            {/each}
          </ol>
        </section>
      {/if}
      {#if history.length}
        <section class="recent">
          <div class="h">Recently played</div>
          <ol>
            {#each history as e (e.id)}
              {@const st = projStyle(payload, e)}
              <li style:--c={st.color} animate:flip={{ duration: 350 }} in:fade={{ duration: 300 }}>
                <span class="stripe"></span>
                <span class="txt">
                  <span class="r-style" use:fit={{ min: 0.45 }}>{st.name}</span>
                  <span class="r-title" use:fit={{ min: 0.45 }}>{e.title}</span>
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
  /* An inset stripe in the style's colour (an edge stripe would be clipped by the corners). */
  .next::before {
    content: "";
    position: absolute;
    left: calc(0.9 * var(--u));
    top: calc(1.6 * var(--u));
    bottom: calc(1.6 * var(--u));
    width: calc(0.45 * var(--u));
    border-radius: 999px;
    background: var(--c);
  }
  .next-style {
    overflow: hidden;
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
  .coming {
    flex: none;
  }
  /* One line per upcoming entry: style, then the song (smaller). */
  .coming .txt {
    flex-direction: row;
    align-items: baseline;
    gap: calc(1 * var(--u));
  }
  .coming .r-style {
    flex: 0 0 auto;
  }
  .coming .txt {
    overflow: hidden;
  }
  /* Natural width: the whole line shrinks to fit (use:fit) instead of cutting the title. */
  .coming .r-title {
    flex: 0 0 auto;
    font-size: calc(1.7 * var(--u));
    color: var(--p-text-2);
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

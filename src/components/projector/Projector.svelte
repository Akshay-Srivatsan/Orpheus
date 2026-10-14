<script lang="ts">
  // Public projector display: what dancers see on the big screen.
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import type { DeckState, ProjectorPayload } from "../../lib/types";
  import LiveView from "./LiveView.svelte";
  import PresetView from "./PresetView.svelte";
  import Clock from "./Clock.svelte";

  const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  // `?noanim` (browser mock only) disables transitions, for screenshots.
  const animMs = !inTauri && new URLSearchParams(location.search).has("noanim") ? 0 : 400;

  let payload = $state<ProjectorPayload | null>(null);
  let decks = $state<DeckState[] | null>(null);
  let mockDeckState = $state<DeckState | null>(null);
  // The progress bar follows the deck the room is hearing.
  const deck = $derived(decks ? (decks[payload?.liveDeck ?? 0] ?? null) : mockDeckState);
  let cursorHidden = $state(false);

  const settings = $derived(payload?.settings);
  const idle = $derived(!payload || (!payload.current && !payload.next && payload.entries.length === 0));
  const progress = $derived.by(() => {
    if (!deck || !payload?.current || deck.entryId !== payload.current.id) return null;
    if (deck.status === "empty" || deck.status === "loading" || deck.status === "error") return null;
    const start = deck.startMs ?? 0;
    const end = deck.endMs ?? deck.durationMs;
    if (!(end > start)) return null;
    return Math.min(1, Math.max(0, (deck.positionMs - start) / (end - start)));
  });

  onMount(() => {
    const unsubs: (() => void)[] = [];
    let cancelled = false;
    if (inTauri) {
      (async () => {
        const { api, on } = await import("../../lib/api");
        const subs = await Promise.all([
          on("projector", (p) => (payload = p)),
          on("deck-state", (d) => (decks = d)),
        ]);
        if (cancelled) subs.forEach((u) => u());
        else unsubs.push(...subs);
        try {
          payload = await api.projectorState();
          decks = await api.deckStates();
        } catch (e) {
          console.error("projector: initial state failed", e);
        }
      })();
    } else {
      (async () => {
        const m = await import("../../lib/projectorMock");
        const params = new URLSearchParams(location.search);
        payload = m.mockPayload(params);
        if (!params.has("gap") && !params.has("idle")) {
          mockDeckState = m.mockDeck();
          // Animate the progress bar in the mock.
          const t = setInterval(() => {
            const d = mockDeckState;
            if (d) mockDeckState = { ...d, positionMs: (d.positionMs + 500) % (d.endMs ?? d.durationMs) };
          }, 500);
          unsubs.push(() => clearInterval(t));
        }
      })();
    }

    // Hide the cursor after 2 s without movement.
    let timer: ReturnType<typeof setTimeout> | undefined;
    const wake = () => {
      cursorHidden = false;
      clearTimeout(timer);
      timer = setTimeout(() => (cursorHidden = true), 2000);
    };
    wake();
    window.addEventListener("mousemove", wake);
    window.addEventListener("keydown", onKey);
    unsubs.push(() => {
      window.removeEventListener("mousemove", wake);
      window.removeEventListener("keydown", onKey);
      clearTimeout(timer);
    });
    return () => {
      cancelled = true;
      unsubs.forEach((u) => u());
    };
  });

  async function setFullscreen(on: boolean | "toggle") {
    if (inTauri) {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const w = getCurrentWindow();
      const target = on === "toggle" ? !(await w.isFullscreen()) : on;
      await w.setFullscreen(target);
    } else {
      const target = on === "toggle" ? !document.fullscreenElement : on;
      if (target && !document.fullscreenElement) await document.documentElement.requestFullscreen().catch(() => {});
      if (!target && document.fullscreenElement) await document.exitFullscreen().catch(() => {});
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "f" || e.key === "F") setFullscreen("toggle");
    else if (e.key === "Escape") setFullscreen(false);
  }
</script>

<div
  class="proj"
  class:light={settings?.theme === "light"}
  class:no-cursor={cursorHidden}
  style:--s={settings?.fontScale ?? 1}
  ondblclick={() => setFullscreen("toggle")}
  role="presentation"
>
  <header>
    <div class="title">{payload?.title ?? "Orpheus"}</div>
    {#if settings?.showClock ?? true}
      <Clock />
    {/if}
  </header>

  <main>
    {#if idle}
      <div class="idle">
        <div class="idle-title">{payload?.title ?? "Orpheus"}</div>
        <div class="idle-sub">The dancing will begin shortly</div>
      </div>
    {:else if payload && payload.mode === "preset"}
      <PresetView {payload} {progress} />
    {:else if payload}
      <LiveView {payload} {progress} />
    {/if}
  </main>

  {#if payload?.message}
    <div class="message" role="status" transition:fly={{ y: 60, duration: animMs }}>{payload.message}</div>
  {/if}
</div>

<style>
  .proj {
    /* One unit ≈ 1% of the screen width (bounded by height), times the user's font scale. */
    --u: calc(min(1vw, 1.78vh) * var(--s, 1));
    --p-bg: #07080a;
    --p-bg-2: #121318;
    --p-bg-3: #1b1d23;
    --p-text: #ffffff;
    --p-text-2: #d9dbe1;
    --p-text-3: #9a9da6;
    --p-line: #2a2c33;
    /* Style colours are tuned for dark backgrounds; darken them on light ones. */
    --ink: 100%;
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(120% 80% at 20% 0%, #15161d 0%, transparent 60%),
      var(--p-bg);
    color: var(--p-text);
    font-family: var(--font-display);
    overflow: hidden;
  }
  .proj.light {
    --p-bg: #f5f5f7;
    --p-bg-2: #ffffff;
    --p-bg-3: #e8e8ed;
    --p-text: #0b0b0d;
    --p-text-2: #2a2b30;
    --p-text-3: #5d5f66;
    --p-line: #d2d2d7;
    --ink: 62%;
    background: var(--p-bg);
  }
  .proj :global(*) {
    --ink-c: color-mix(in srgb, var(--c) var(--ink), black);
  }
  .no-cursor,
  .no-cursor :global(*) {
    cursor: none !important;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: calc(2 * var(--u));
    padding: calc(1.6 * var(--u)) calc(3 * var(--u)) calc(0.8 * var(--u));
  }
  .title {
    font-size: calc(2.2 * var(--u));
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--p-text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  main {
    flex: 1;
    min-height: 0;
    display: flex;
    padding: 0 calc(3 * var(--u)) calc(2.4 * var(--u));
  }
  .idle {
    margin: auto;
    text-align: center;
  }
  .idle-title {
    font-size: calc(7 * var(--u));
    font-weight: 800;
    letter-spacing: -0.03em;
    background: linear-gradient(90deg, #a78bfa, #f472b6 50%, #fbbf24);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .idle-sub {
    margin-top: calc(1.5 * var(--u));
    font-size: calc(2.8 * var(--u));
    color: var(--p-text-2);
  }
  .message {
    flex: none;
    padding: calc(1.6 * var(--u)) calc(3 * var(--u));
    background: #ffd60a;
    color: #000;
    font-size: calc(3.4 * var(--u));
    font-weight: 750;
    text-align: center;
    letter-spacing: -0.01em;
  }
</style>

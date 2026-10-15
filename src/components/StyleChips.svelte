<script lang="ts">
  // Edits a list of your dance styles (or your tags): removable chips plus an
  // "add" menu. With `choices`, only those can be toggled on and off (e.g.
  // which of a song's styles a setlist shows).
  import { app } from "../lib/state.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  interface Props {
    value: string[];
    onchange: (keys: string[]) => void;
    /** Restrict to toggling these styles (in this order). */
    choices?: string[];
    /** Planned tempo, for showing how each style maps at the current club. */
    bpm?: number | null;
    kind?: "style" | "tag";
  }
  let { value, onchange, choices, bpm = null, kind = "style" }: Props = $props();

  const all = $derived(kind === "tag" ? app.myTags : app.myStyles);
  const addable = $derived(all.filter((s) => !value.includes(s.key)));
  const name = (k: string) => (kind === "tag" ? app.tagName(k) : app.styleName(k));
  const color = (k: string) => (kind === "tag" ? app.tagColor(k) : app.styleColor(k));

  // The app's own menu, not a native <select>: WebKitGTK truncates a narrow
  // select's "+" to an ellipsis.
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  function openAdd(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = addable.map((s) => ({ label: s.name, action: () => onchange([...value, s.key]) }));
    menu = { x: r.left, y: r.bottom + 2, items };
  }

  function title(key: string): string {
    if (kind === "tag") return app.tagName(key);
    void bpm;
    return app.styleName(key);
  }
</script>

<span class="chips">
  {#if choices}
    {#each choices as k}
      {@const on = value.includes(k)}
      <button class="badge chip" class:off={!on} style:--c={color(k)} title={title(k)}
        onclick={() => onchange(on ? value.filter((x) => x !== k) : choices.filter((x) => x === k || value.includes(x)))}>
        {name(k)}
      </button>
    {/each}
  {:else}
    {#each value as k}
      <span class="badge chip" style:--c={color(k)} title={title(k)}>
        {name(k)}
        <button class="plain x" title="Remove {name(k)}" onclick={() => onchange(value.filter((x) => x !== k))}>×</button>
      </span>
    {/each}
    {#if addable.length}
      <button class="add small" title={kind === "tag" ? "Add a tag" : "Add a dance style"} onclick={openAdd}>
        {value.length ? "+" : kind === "tag" ? "Add tag…" : "Add style…"}
      </button>
    {:else if !all.length}
      <button class="plain small" onclick={() => (app.settingsOpen = "styles")}>{kind === "tag" ? "Create tags…" : "Create dance styles…"}</button>
    {/if}
  {/if}
</span>
{#if menu}<ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

<style>
  .chips { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .chip { height: 20px; gap: 2px; }
  button.chip { cursor: pointer; }
  .chip.off { opacity: 0.4; filter: grayscale(0.7); }
  .x { padding: 0 2px; height: 16px; min-width: 0; font-size: 13px; line-height: 1; color: inherit; }
  .add { height: 20px; min-width: 24px; padding: 0 6px; }
  .small { font-size: var(--fs-sm); }
</style>

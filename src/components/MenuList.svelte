<script lang="ts">
  // One panel of a context menu; submenus open beside their item (see ContextMenu).
  import type { MenuItem } from "./ContextMenu.svelte";
  import Self from "./MenuList.svelte";

  type Action = Extract<MenuItem, { action: () => unknown }>;
  type Sub = Extract<MenuItem, { items: MenuItem[] }>;

  interface Props {
    items: MenuItem[];
    /** Where a top-level menu opens. */
    x?: number;
    y?: number;
    /** A submenu's item: it opens beside it. */
    anchor?: DOMRect;
    onclose: () => void;
    /** The mouse reached this panel (its parent keeps it open). */
    onenter?: () => void;
  }
  let { items, x = 0, y = 0, anchor, onclose, onenter }: Props = $props();

  /** Moving across other items toward an open submenu shouldn't close it straight away (ms). */
  const SWITCH_DELAY_MS = 200;
  /** Gap between the window edge and a menu (px). */
  const EDGE = 6;

  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });
  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (anchor) {
      const right = anchor.right - 2;
      const left = right + r.width > window.innerWidth - EDGE ? Math.max(EDGE, anchor.left - r.width + 2) : right;
      pos = { left, top: Math.max(EDGE, Math.min(anchor.top - 4, window.innerHeight - r.height - EDGE)) };
    } else {
      pos = { left: Math.min(x, window.innerWidth - r.width - EDGE), top: Math.min(y, window.innerHeight - r.height - EDGE) };
    }
  });

  const isSep = (it: MenuItem) => "separator" in it && !!it.separator;
  const isSub = (it: MenuItem): it is Sub => "items" in it;
  /** Without empty submenus, and separators only between items. */
  const clean = $derived.by(() => {
    const kept = items.filter((it) => !isSub(it) || it.items.some((c) => !isSep(c)));
    return kept.filter((it, i, a) => !isSep(it) || (i > 0 && i < a.length - 1 && !isSep(a[i - 1])));
  });

  let open = $state<{ index: number; rect: DOMRect } | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  function hover(i: number, target: HTMLElement) {
    clearTimeout(timer);
    const it = clean[i];
    const next = isSub(it) && !it.disabled ? { index: i, rect: target.getBoundingClientRect() } : null;
    if (open?.index === next?.index) return;
    if (open) timer = setTimeout(() => (open = next), SWITCH_DELAY_MS);
    else open = next;
  }
  $effect(() => () => clearTimeout(timer));
</script>

<div class="menu" bind:this={el} style:left="{pos.left}px" style:top="{pos.top}px" role="menu" tabindex="-1"
  onmouseenter={() => onenter?.()}>
  {#each clean as it, i}
    {#if isSep(it)}
      <div class="sep"></div>
    {:else if isSub(it)}
      <button role="menuitem" aria-haspopup="menu" aria-expanded={open?.index === i} class:open={open?.index === i} disabled={it.disabled}
        onmouseenter={(e) => hover(i, e.currentTarget)}
        onclick={(e) => { clearTimeout(timer); open = open?.index === i ? null : { index: i, rect: e.currentTarget.getBoundingClientRect() }; }}>
        <span class="check"></span><span class="label">{it.label}</span><span class="more">▸</span>
      </button>
    {:else}
      {@const item = it as Action}
      <button role="menuitem" class:danger={item.danger} disabled={item.disabled}
        onmouseenter={(e) => hover(i, e.currentTarget)}
        onclick={() => { onclose(); item.action(); }}>
        <span class="check">{item.checked ? "✓" : ""}</span><span class="label">{item.label}</span>
      </button>
    {/if}
  {/each}
</div>
{#if open}
  {@const sub = clean[open.index] as Sub}
  <Self items={sub.items} anchor={open.rect} {onclose} onenter={() => clearTimeout(timer)} />
{/if}

<style>
  .menu { position: fixed; z-index: 100; min-width: 200px; max-height: 70vh; overflow-y: auto; padding: 4px; border-radius: 8px;
    background: rgba(40, 41, 46, 0.97); border: 1px solid var(--border-strong); box-shadow: var(--shadow); backdrop-filter: blur(20px); outline: none; }
  button { display: flex; align-items: center; width: 100%; justify-content: flex-start; height: 24px; border: none; background: none; border-radius: 4px; padding: 0 8px 0 4px; }
  button:hover:not(:disabled), button.open { background: var(--blue); color: white; }
  .danger { color: var(--red); }
  .check { width: 16px; flex: none; display: inline-block; text-align: center; }
  .label { flex: 1; text-align: left; white-space: nowrap; }
  .more { margin-left: 16px; font-size: 10px; opacity: 0.7; }
  .sep { height: 1px; background: var(--border-strong); margin: 4px 6px; }
</style>

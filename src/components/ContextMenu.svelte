<script module lang="ts">
  export type MenuItem =
    | { label: string; action: () => void; danger?: boolean; disabled?: boolean; checked?: boolean; separator?: false }
    | { separator: true };
</script>

<script lang="ts">
  interface Props { x: number; y: number; items: MenuItem[]; onclose: () => void }
  let { x, y, items, onclose }: Props = $props();
  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    pos = {
      left: Math.min(x, window.innerWidth - r.width - 6),
      top: Math.min(y, window.innerHeight - r.height - 6),
    };
  });

  const clean = $derived(items.filter((it, i, a) => !("separator" in it && it.separator) || (i > 0 && i < a.length - 1 && !("separator" in a[i - 1] && a[i - 1].separator))));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />
<div class="backdrop" role="presentation" onmousedown={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>
<div class="menu" bind:this={el} style:left="{pos.left}px" style:top="{pos.top}px" role="menu">
  {#each clean as it}
    {#if "separator" in it && it.separator}
      <div class="sep"></div>
    {:else if !("separator" in it && it.separator)}
      {@const item = it as Extract<MenuItem, { label: string }>}
      <button role="menuitem" class:danger={item.danger} disabled={item.disabled}
        onclick={() => { onclose(); item.action(); }}>
        <span class="check">{item.checked ? "✓" : ""}</span>{item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 99; }
  .menu { position: fixed; z-index: 100; min-width: 200px; max-height: 70vh; overflow-y: auto; padding: 4px; border-radius: 8px;
    background: rgba(40, 41, 46, 0.97); border: 1px solid var(--border-strong); box-shadow: var(--shadow); backdrop-filter: blur(20px); }
  button { display: flex; width: 100%; justify-content: flex-start; height: 24px; border: none; background: none; border-radius: 4px; padding: 0 8px 0 4px; }
  button:hover:not(:disabled) { background: var(--blue); color: white; }
  .danger { color: var(--red); }
  .check { width: 16px; display: inline-block; text-align: center; }
  .sep { height: 1px; background: var(--border-strong); margin: 4px 6px; }
</style>

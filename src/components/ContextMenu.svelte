<script module lang="ts">
  /** A command, a submenu (`items`, opens beside it), or a separator. */
  export type MenuItem =
    | { label: string; action: () => unknown; danger?: boolean; disabled?: boolean; checked?: boolean; separator?: false }
    | { label: string; items: MenuItem[]; disabled?: boolean; separator?: false }
    | { separator: true };
</script>

<script lang="ts">
  import MenuList from "./MenuList.svelte";

  interface Props { x: number; y: number; items: MenuItem[]; onclose: () => void }
  let { x, y, items, onclose }: Props = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />
<div class="backdrop" role="presentation" onmousedown={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>
<MenuList {items} {x} {y} {onclose} />

<style>
  .backdrop { position: fixed; inset: 0; z-index: 99; }
</style>

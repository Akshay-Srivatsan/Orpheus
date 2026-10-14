<script lang="ts">
  import { app } from "../lib/state.svelte";
  interface Props { style: string | null | undefined; short?: boolean; big?: boolean }
  let { style, short = false, big = false }: Props = $props();
  const s = $derived(app.style(style));
</script>

{#if style}
  <span class="badge" class:big style:--c={app.styleColor(style)} title={app.styleName(style)}>
    {short ? (s?.short ?? app.styleName(style)) : app.styleName(style)}
  </span>
{/if}

<style>
  .big { height: 24px; padding: 0 10px; font-size: var(--fs); border-radius: 12px; }
</style>

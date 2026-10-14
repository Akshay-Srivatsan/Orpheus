<script lang="ts">
  // Dance-style badges: one per style, or a single one with an entry's custom label.
  import { NEUTRAL } from "../lib/styles";
  import type { Shown } from "../lib/styles";
  interface Props { styles: Shown[]; label?: string | null; short?: boolean; big?: boolean }
  let { styles, label = null, short = false, big = false }: Props = $props();
</script>

{#if label}
  <span class="badge" class:big style:--c={styles[0]?.color ?? NEUTRAL} title={styles.map((s) => s.name).join(" / ")}>{label}</span>
{:else}
  {#each styles as s (s.key)}
    <span class="badge" class:big class:unmapped={s.unmapped} style:--c={s.color} title={s.unmapped ? `${s.name} (not a style at this club)` : s.name}>
      {short ? s.short : s.name}
    </span>
  {/each}
{/if}

<style>
  .big { height: 24px; padding: 0 10px; font-size: var(--fs); border-radius: 12px; }
  .unmapped { font-style: italic; opacity: 0.7; }
</style>

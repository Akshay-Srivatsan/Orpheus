<script lang="ts">
  import { onMount } from "svelte";

  let now = $state(new Date());
  onMount(() => {
    const t = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(t);
  });
  const text = $derived(now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }));
</script>

<div class="clock">{text}</div>

<style>
  .clock {
    font-size: calc(2.2 * var(--u));
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--p-text-2);
    white-space: nowrap;
  }
</style>

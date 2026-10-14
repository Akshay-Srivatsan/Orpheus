<script lang="ts">
  import { app } from "../lib/state.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}" role="status">
      <span>{t.text}</span>
      <button class="plain icon" onclick={() => (app.toasts = app.toasts.filter((x) => x.id !== t.id))}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; left: 50%; bottom: 20px; transform: translateX(-50%); z-index: 120; display: flex; flex-direction: column; gap: 8px; align-items: center; pointer-events: none; }
  .toast { pointer-events: auto; display: flex; align-items: center; gap: 10px; max-width: 640px; padding: 8px 8px 8px 14px; border-radius: 10px;
    background: rgba(44, 45, 50, 0.97); border: 1px solid var(--border-strong); box-shadow: var(--shadow); }
  .toast.error { border-color: color-mix(in srgb, var(--red) 60%, transparent); }
  .toast.info { border-color: color-mix(in srgb, var(--blue) 50%, transparent); }
</style>

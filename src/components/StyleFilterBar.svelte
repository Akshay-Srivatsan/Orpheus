<script lang="ts">
  // Dance-style filter for whatever the library is showing. Styles are labels
  // on songs, not collections: any view (all songs, a crate, a setlist...) can
  // be narrowed to one or more styles. Dropping songs on a style labels them.
  import { api } from "../lib/api";
  import { NO_STYLE, app } from "../lib/state.svelte";
  import type { Id, Row } from "../lib/types";

  interface Props { rows: Row[] }
  let { rows }: Props = $props();

  let drop = $state<string | null>(null);

  const counts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const r of rows) {
      const k = r.version.style ?? NO_STYLE;
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return m;
  });
  const sel = $derived(new Set(app.styleFilter));
  /** Profile styles plus any unknown style keys found in this view. */
  const chips = $derived.by(() => {
    const known = (app.profile?.styles ?? []).map((s) => s.key);
    const extra = [...counts.keys()].filter((k) => k !== NO_STYLE && !known.includes(k));
    // Hide styles with no songs here unless selected, to keep the bar short.
    return [...known, ...extra].filter((k) => counts.get(k) || sel.has(k));
  });
  const hidden = $derived((app.profile?.styles ?? []).filter((s) => !chips.includes(s.key)));
  const st = $derived(app.style(app.filterStyle));

  function toggle(e: MouseEvent, key: string) {
    if (e.metaKey || e.ctrlKey || e.shiftKey) {
      app.styleFilter = sel.has(key) ? app.styleFilter.filter((k) => k !== key) : [...app.styleFilter, key];
    } else {
      app.styleFilter = sel.has(key) && sel.size === 1 ? [] : [key];
    }
    app.selected = [];
  }

  function dragged(e: DragEvent): Id[] {
    try {
      return JSON.parse(e.dataTransfer?.getData("application/x-orpheus-versions") || "[]");
    } catch {
      return [];
    }
  }

  async function dropOn(e: DragEvent, key: string) {
    e.preventDefault();
    drop = null;
    const ids = dragged(e);
    if (!ids.length) return;
    if (key === NO_STYLE) await app.run(api.styleRemove(ids));
    else await app.run(api.styleAdd(key, ids));
  }
</script>

<div class="bar" role="toolbar" aria-label="Filter by dance style">
  <button class="chip all" class:on={!app.styleFilter.length} onclick={() => (app.styleFilter = [])}>All styles</button>
  {#each chips as k (k)}
    {@const s = app.style(k)}
    <button
      class="chip"
      class:on={sel.has(k)}
      class:drop={drop === k}
      style:--c={app.styleColor(k)}
      title="{app.styleName(k)}{s?.sweet ? ` · ${s.sweet[0]}–${s.sweet[1]} BPM` : ''} — click to filter, ⌘/Shift-click to add; drop songs here to label them"
      onclick={(e) => toggle(e, k)}
      ondragover={(e) => { e.preventDefault(); drop = k; }}
      ondragleave={() => (drop = null)}
      ondrop={(e) => dropOn(e, k)}
    >
      <span class="dot"></span>{s?.short || app.styleName(k)}<span class="n">{counts.get(k) ?? 0}</span>
    </button>
  {/each}
  {#if counts.get(NO_STYLE) || sel.has(NO_STYLE)}
    <button class="chip" class:on={sel.has(NO_STYLE)} class:drop={drop === NO_STYLE}
      title="Songs without a dance style — drop songs here to remove their style"
      onclick={(e) => toggle(e, NO_STYLE)}
      ondragover={(e) => { e.preventDefault(); drop = NO_STYLE; }}
      ondragleave={() => (drop = null)}
      ondrop={(e) => dropOn(e, NO_STYLE)}>No style<span class="n">{counts.get(NO_STYLE)}</span></button>
  {/if}
  {#if hidden.length}
    <select class="more" value="" title="Filter by a style with no songs here yet (e.g. to label some)"
      onchange={(e) => { if (e.currentTarget.value) app.styleFilter = [e.currentTarget.value]; e.currentTarget.value = ""; }}>
      <option value="">More…</option>
      {#each hidden as s}<option value={s.key}>{s.name}</option>{/each}
    </select>
  {/if}
</div>
{#if st}
  <div class="info">
    <span class="sname" style:color={st.color}>{st.name}</span>
    {#if st.sweet}<span class="num">{st.sweet[0]}–{st.sweet[1]} BPM{#if st.extended} <span class="faint">(ok {st.extended[0]}–{st.extended[1]})</span>{/if}</span>{/if}
    {#if st.sweet || st.extended}
      <label class="faint" title="Also list songs without a style whose tempo fits {st.name}">
        <input type="checkbox" bind:checked={app.suggestByTempo} /> include unlabelled songs that fit its tempo
      </label>
    {/if}
  </div>
{/if}

<style>
  .bar { display: flex; flex-wrap: wrap; gap: 4px; padding: 6px 10px; border-bottom: 1px solid var(--border); max-height: 62px; overflow-y: auto; }
  .chip { height: 22px; padding: 0 8px; gap: 5px; border-radius: 11px; font-size: var(--fs-sm); background: var(--bg-elev-2); border-color: var(--border); color: var(--text-dim); }
  .chip .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--c, #8e8e93); }
  .chip .n { color: var(--text-faint); font-size: var(--fs-xs); font-variant-numeric: tabular-nums; }
  .chip.on { background: color-mix(in srgb, var(--c, var(--accent)) 28%, var(--bg-elev-3)); border-color: color-mix(in srgb, var(--c, var(--accent)) 70%, transparent); color: var(--text); }
  .chip.on .n { color: var(--text-dim); }
  .chip.all.on { --c: var(--accent); }
  .chip.drop { outline: 2px solid var(--blue); outline-offset: 1px; }
  .more { flex: none; width: auto; height: 22px; padding: 0 6px; font-size: var(--fs-sm); line-height: 20px; border-radius: 11px; color: var(--text-dim); }
  .info { display: flex; align-items: center; gap: 12px; padding: 4px 12px; font-size: var(--fs-sm); border-bottom: 1px solid var(--border); background: var(--bg-elev); }
  .sname { font-weight: 600; }
  label { font-size: var(--fs-sm); }
</style>

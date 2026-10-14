<script lang="ts">
  // Settings → Dance Styles: your own styles (what songs are tagged with) and,
  // per club, the ordered rules that map them onto the club's styles.
  import { api } from "../lib/api";
  import { app, confirmAction } from "../lib/state.svelte";
  import { clubStyle, NEUTRAL } from "../lib/styles";
  import type { StyleLibrary, StyleMappingRule } from "../lib/types";
  import Icon from "./Icon.svelte";

  const lib = $derived(app.styleLib);
  const usage = $derived(lib?.usage ?? {});
  const clubs = $derived(app.profiles.filter((p) => p.styles.length > 0));
  let clubId = $state(app.hasClub ? (app.profile?.id ?? "") : "");
  const club = $derived(clubs.find((p) => p.id === clubId) ?? clubs[0]);
  const rules = $derived((club && lib?.mappings[club.id]) ?? []);
  let newName = $state("");
  const undefinedKeys = $derived(Object.keys(usage).filter((k) => !lib?.styles.some((s) => s.key === k)));

  /** Applies a change to styles.json and saves it. */
  function update(change: (l: StyleLibrary) => void) {
    if (!lib) return;
    const { usage: u, ...rest } = $state.snapshot(lib);
    const next = structuredClone(rest) as StyleLibrary;
    change(next);
    app.styleLib = { ...next, usage: u };
    app.run(api.saveStyles(next));
  }

  function slug(name: string): string {
    const base = name.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "style";
    let key = base;
    for (let i = 2; lib?.styles.some((s) => s.key === key); i++) key = `${base}-${i}`;
    return key;
  }

  function addStyle(name: string, from?: { short: string; color: string; key?: string }) {
    const key = from?.key && !lib?.styles.some((s) => s.key === from.key) ? from.key : slug(name);
    update((l) => l.styles.push({ key, name, short: from?.short ?? "", color: from?.color ?? NEUTRAL }));
  }

  async function removeStyle(key: string) {
    const n = usage[key] ?? 0;
    const name = app.styleName(key);
    if (!(await confirmAction(n ? `Delete “${name}”? It is removed from ${n} song version(s); the songs stay.` : `Delete “${name}”?`))) return;
    await app.run(api.rekeyStyle(key, null));
  }

  async function mergeStyle(key: string, into: string) {
    if (!into) return;
    const ok = await confirmAction(`Merge “${app.styleName(key)}” into “${app.styleName(into)}”? Songs tagged ${app.styleName(key)} become ${app.styleName(into)}.`, "Merge");
    if (ok) await app.run(api.rekeyStyle(key, into));
  }

  function setRules(change: (r: StyleMappingRule[]) => void) {
    if (!club) return;
    const id = club.id;
    update((l) => {
      const r = l.mappings[id] ?? [];
      change(r);
      if (r.length) l.mappings[id] = r;
      else delete l.mappings[id];
    });
  }

  function num(v: string): number | null {
    return v.trim() === "" ? null : +v;
  }

  /** Club styles not among yours (by key or name), offered for import. */
  const importable = $derived(
    (club?.styles ?? []).filter((c) => !lib?.styles.some((s) => s.key === c.key || s.name.toLowerCase() === c.name.toLowerCase())),
  );
</script>

<h2>Dance Styles</h2>
<p class="dim">
  Your dance styles are what you tag songs with. A song can have several. Clubs can name and split styles differently, so each
  club profile has its own styles. The rules below say how yours appear at a club. The setlist and projector always show the
  club's names; under <b>No club</b>, your own styles are shown.
</p>

<h3>Your styles</h3>
<table>
  <thead><tr><th></th><th>Name</th><th>Short</th><th>Songs</th><th>Merge into</th><th></th></tr></thead>
  <tbody>
    {#each lib?.styles ?? [] as st, i (st.key)}
      <tr>
        <td><input type="color" value={st.color} onchange={(e) => update((l) => (l.styles[i].color = e.currentTarget.value))} /></td>
        <td><input type="text" value={st.name} title="Key: {st.key}" onchange={(e) => update((l) => (l.styles[i].name = e.currentTarget.value.trim() || st.name))} /></td>
        <td><input type="text" class="w5" value={st.short} placeholder={st.name.slice(0, 4)} onchange={(e) => update((l) => (l.styles[i].short = e.currentTarget.value.trim()))} /></td>
        <td class="num faint">{usage[st.key] ?? 0}</td>
        <td>
          <select value="" onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ""; mergeStyle(st.key, v); }}>
            <option value="">—</option>
            {#each (lib?.styles ?? []).filter((o) => o.key !== st.key) as o}<option value={o.key}>{o.name}</option>{/each}
          </select>
        </td>
        <td>
          <button class="plain icon" disabled={i === 0} title="Move up" onclick={() => update((l) => l.styles.splice(i - 1, 0, ...l.styles.splice(i, 1)))}>↑</button>
          <button class="plain icon danger" title="Delete style" onclick={() => removeStyle(st.key)}><Icon name="trash" size={12} /></button>
        </td>
      </tr>
    {:else}
      <tr><td colspan="6" class="faint">No styles yet.</td></tr>
    {/each}
  </tbody>
</table>
{#if undefinedKeys.length}
  <p class="small warn">
    Songs are tagged with styles that aren't in your list:
    {#each undefinedKeys as k, i}{i ? ", " : ""}<button class="plain link" onclick={() => addStyle(app.styleName(k), { key: k, short: "", color: NEUTRAL })}>{app.styleName(k)} ({usage[k]})</button>{/each}.
    Click one to add it.
  </p>
{/if}
<form class="row add" onsubmit={(e) => { e.preventDefault(); if (newName.trim()) { addStyle(newName.trim()); newName = ""; } }}>
  <input type="text" placeholder="New style name" bind:value={newName} />
  <button type="submit" disabled={!newName.trim()}><Icon name="plus" size={12} /> Add Style</button>
  {#if club && importable.length}
    <button type="button" onclick={() => importable.forEach((c) => addStyle(c.name, c))} title={importable.map((c) => c.name).join(", ")}>
      Add {club.name}'s {importable.length} other style{importable.length === 1 ? "" : "s"}
    </button>
  {/if}
</form>

<h3>At a club</h3>
{#if !club}
  <p class="dim">No club profile has its own styles. Add some in Club Profiles.</p>
{:else}
  <div class="row">
    <select bind:value={clubId}>{#each clubs as p}<option value={p.id}>{p.name}</option>{/each}</select>
    <span class="faint small">The first matching rule wins. A BPM range is matched in its own counting (half and double time count).</span>
  </div>
  <table class="rules">
    <thead><tr><th>Your style</th><th>Planned BPM</th><th></th><th>Counts at {club.name} as</th><th></th></tr></thead>
    <tbody>
      {#each rules as r, i (i)}
        <tr>
          <td>
            <select value={r.style} onchange={(e) => setRules((rs) => (rs[i].style = e.currentTarget.value))}>
              {#each lib?.styles ?? [] as s}<option value={s.key}>{s.name}</option>{/each}
              {#if !lib?.styles.some((s) => s.key === r.style)}<option value={r.style}>{app.styleName(r.style)}</option>{/if}
            </select>
          </td>
          <td class="nowrap">
            <input type="number" class="w4" placeholder="any" value={r.minBpm ?? ""} onchange={(e) => setRules((rs) => (rs[i].minBpm = num(e.currentTarget.value)))} />–<input
              type="number" class="w4" placeholder="any" value={r.maxBpm ?? ""} onchange={(e) => setRules((rs) => (rs[i].maxBpm = num(e.currentTarget.value)))} />
          </td>
          <td>→</td>
          <td>
            <select value={r.to} onchange={(e) => setRules((rs) => (rs[i].to = e.currentTarget.value))}>
              {#each club.styles as c}<option value={c.key}>{c.name}{c.sweet ? ` (${c.sweet[0]}–${c.sweet[1]})` : ""}</option>{/each}
              <option value="">Not danced here</option>
            </select>
          </td>
          <td class="nowrap">
            <button class="plain icon" disabled={i === 0} title="Move up" onclick={() => setRules((rs) => rs.splice(i - 1, 0, ...rs.splice(i, 1)))}>↑</button>
            <button class="plain icon" disabled={i === rules.length - 1} title="Move down" onclick={() => setRules((rs) => rs.splice(i + 1, 0, ...rs.splice(i, 1)))}>↓</button>
            <button class="plain icon danger" title="Delete rule" onclick={() => setRules((rs) => rs.splice(i, 1))}><Icon name="x" size={12} /></button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <button class="small" disabled={!lib?.styles.length}
    onclick={() => setRules((rs) => rs.push({ style: lib!.styles[0].key, minBpm: null, maxBpm: null, to: club.styles[0]?.key ?? "" }))}>
    <Icon name="plus" size={12} /> Add Rule
  </button>

  <h3 class="small">Without a rule</h3>
  <div class="defaults small">
    {#each lib?.styles ?? [] as s}
      {@const own = rules.filter((r) => r.style === s.key)}
      {@const always = own.some((r) => r.minBpm == null && r.maxBpm == null)}
      {@const d = clubStyle({ styles: lib?.styles ?? [], mappings: {} }, club, s.key, null)}
      <span class:faint={own.length > 0}>
        <span style:color={s.color}>{s.name}</span> →
        {#if always}<i>always set by a rule</i>{:else if d}{d.name}{:else}<i>not a {club.name} style</i>{/if}{#if own.length && !always}&nbsp;(if no rule matches){/if}
      </span>
    {/each}
  </div>
{/if}

<style>
  h2 { margin: 0 0 8px; font: 600 var(--fs-lg) var(--font-display); }
  h3 { margin: 18px 0 6px; font-size: var(--fs-md); }
  p { margin: 0 0 14px; max-width: 720px; }
  table { border-collapse: collapse; font-size: var(--fs-sm); margin-bottom: 6px; }
  th { text-align: left; color: var(--text-faint); font-weight: 600; padding: 4px; border-bottom: 1px solid var(--border); }
  td { padding: 3px 4px; border-bottom: 1px solid rgba(255, 255, 255, 0.04); }
  td input[type="text"] { width: 180px; }
  .w4 { width: 60px; }
  .w5 { width: 70px !important; }
  input[type="color"] { width: 26px; height: 22px; padding: 0; border: none; background: none; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; margin-bottom: 6px; }
  .add input { width: 200px; }
  .nowrap { white-space: nowrap; }
  .small { font-size: var(--fs-sm); }
  .danger { color: var(--red); }
  .warn { color: var(--yellow); }
  .link { padding: 0; height: auto; color: var(--blue); display: inline; }
  .defaults { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 3px 16px; }
</style>

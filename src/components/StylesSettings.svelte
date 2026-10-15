<script lang="ts">
  // Settings → Dance Styles: your own styles (what songs are tagged with) and,
  // per club, rules for what the club calls them ("what I call X, the club may
  // call Y"; the same name needs no rule).
  import { api } from "../lib/api";
  import { app, confirmAction } from "../lib/state.svelte";
  import { NEUTRAL, sameName, styleDef } from "../lib/styles";
  import type { Club, StyleLibrary } from "../lib/types";
  import Icon from "./Icon.svelte";

  const lib = $derived(app.styleLib);
  const usage = $derived(lib?.usage ?? {});
  let clubId = $state(app.clubRef?.id ?? app.clubs[0]?.id ?? "");
  /** The chosen club's latest guidelines (for its styles). */
  let club = $state<Club | null>(null);
  $effect(() => {
    const c = app.clubs.find((x) => x.id === clubId) ?? app.clubs[0];
    const v = c?.versions[0];
    club = null;
    if (c && v) api.club(c.id, v.id).then((r) => { if (c.id === (app.clubs.find((x) => x.id === clubId) ?? app.clubs[0])?.id) club = r.club; });
  });
  const clubKey = $derived((app.clubs.find((x) => x.id === clubId) ?? app.clubs[0])?.id ?? "");
  const clubStyles = $derived(Object.entries(club?.style ?? {}));
  const rules = $derived(lib?.clubRules?.[clubKey] ?? []);
  /** Your styles and the club's with the same name: paired without a rule. */
  const sameNamed = $derived(
    (lib?.styles ?? []).flatMap((m) => clubStyles.filter(([id, c]) => sameName(styleDef(id, c).name, m.name)).map(([id]) => ({ mine: m.key, club: id }))),
  );
  let ruleMine = $state("");
  let ruleClub = $state("");
  const clubStyleName = (id: string) => (club?.style[id] ? styleDef(id, club.style[id]).name : id);
  let newName = $state("");
  const undefinedKeys = $derived(Object.keys(usage).filter((k) => !lib?.styles.some((s) => s.key === k)));

  /** Applies a change to styles.json and saves it. */
  function update(change: (l: StyleLibrary) => void) {
    if (!lib) return;
    const { usage: u, tagUsage: tu, ...rest } = $state.snapshot(lib);
    const next = structuredClone(rest) as StyleLibrary;
    next.tags ??= [];
    change(next);
    app.styleLib = { ...next, usage: u, tagUsage: tu };
    app.run(api.saveStyles(next));
  }

  function slug(name: string, taken: { key: string }[] = lib?.styles ?? []): string {
    const base = name.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "item";
    let key = base;
    for (let i = 2; taken.some((s) => s.key === key); i++) key = `${base}-${i}`;
    return key;
  }

  // ---------------------------------------------------------------- your tags
  let newTag = $state("");
  const tags = $derived(lib?.tags ?? []);
  const tagUsage = $derived(lib?.tagUsage ?? {});
  const undefinedTags = $derived(Object.keys(tagUsage).filter((k) => !tags.some((t) => t.key === k)));

  function addTag(name: string, key?: string) {
    const k = key && !tags.some((t) => t.key === key) ? key : slug(name, tags);
    update((l) => l.tags!.push({ key: k, name, color: NEUTRAL }));
  }

  async function removeTag(key: string) {
    const n = tagUsage[key] ?? 0;
    const name = app.tagName(key);
    if (!(await confirmAction(n ? `Delete the tag “${name}”? It is removed from ${n} song version(s).` : `Delete the tag “${name}”?`))) return;
    await app.run(api.rekeyTag(key, null));
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

  /** Adds "what I call `mine`, the club may call `theirs`" (or removes it). */
  function setRule(mine: string, theirs: string, on: boolean) {
    const id = clubKey;
    if (!id || !mine || !theirs) return;
    update((l) => {
      const all = (l.clubRules ??= {});
      const list = (all[id] ?? []).filter((r) => !(r.mine === mine && r.club === theirs));
      if (on) list.push({ mine, club: theirs });
      if (list.length) all[id] = list;
      else delete all[id];
    });
  }

  /** Club styles not among yours (by name), offered for import. */
  const importable = $derived(
    clubStyles.map(([id, c]) => styleDef(id, c)).filter((c) => !lib?.styles.some((s) => s.key === c.key || s.name.toLowerCase() === c.name.toLowerCase())),
  );
</script>

<h2>Dance Styles &amp; Tags</h2>
<p class="dim">
  Your dance styles are what you tag songs with; a song can have several. In a club's set, songs show the <i>club's</i>
  styles they fit (by meter, tempo and length, or because the club suggests the song). When a song fits several, your
  styles on it pick which show by default: what the club calls them (see At a Club below), or all of them. That never
  makes a song a style it doesn't fit; choose any style for a song in the set with right-click → Show as. Without a club,
  your own styles are shown.
</p>

<h3>Your styles</h3>
<table>
  <thead><tr><th></th><th>Name</th><th>Short</th><th>Songs</th><th>Merge into</th><th></th></tr></thead>
  <tbody>
    {#each lib?.styles ?? [] as st, i (st.key)}
      <tr>
        <td><input type="color" value={st.color} onchange={(e) => update((l) => (l.styles[i].color = e.currentTarget.value))} /></td>
        <td><input type="text" value={st.name} title="Key: {st.key}" onchange={(e) => update((l) => (l.styles[i].name = e.currentTarget.value.trim() || st.name))} /></td>
        <td><input type="text" class="w5" value={st.short} placeholder={st.name} title="Short name for tight spaces (empty = the full name)" onchange={(e) => update((l) => (l.styles[i].short = e.currentTarget.value.trim()))} /></td>
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

<h3>Your tags</h3>
<p class="dim small">Labels for songs that aren't dance styles, e.g. “Classic”, “Teaching”, “Requests”. Smart crates can use them.</p>
<table>
  <thead><tr><th></th><th>Name</th><th>Songs</th><th></th></tr></thead>
  <tbody>
    {#each tags as t, i (t.key)}
      <tr>
        <td><input type="color" value={t.color} onchange={(e) => update((l) => (l.tags![i].color = e.currentTarget.value))} /></td>
        <td><input type="text" value={t.name} onchange={(e) => update((l) => (l.tags![i].name = e.currentTarget.value.trim() || t.name))} /></td>
        <td class="num faint">{tagUsage[t.key] ?? 0}</td>
        <td><button class="plain icon danger" title="Delete tag" onclick={() => removeTag(t.key)}><Icon name="trash" size={12} /></button></td>
      </tr>
    {:else}
      <tr><td colspan="4" class="faint">No tags yet.</td></tr>
    {/each}
  </tbody>
</table>
{#if undefinedTags.length}
  <p class="small warn">
    Songs carry tags that aren't in your list:
    {#each undefinedTags as k, i}{i ? ", " : ""}<button class="plain link" onclick={() => addTag(app.tagName(k), k)}>{app.tagName(k)} ({tagUsage[k]})</button>{/each}.
    Click one to add it.
  </p>
{/if}
<form class="row add" onsubmit={(e) => { e.preventDefault(); if (newTag.trim()) { addTag(newTag.trim()); newTag = ""; } }}>
  <input type="text" placeholder="New tag name" bind:value={newTag} />
  <button type="submit" disabled={!newTag.trim()}><Icon name="plus" size={12} /> Add Tag</button>
</form>

<h3>At a club</h3>
{#if !app.clubs.length}
  <p class="dim">No clubs yet: add one in <button class="plain link" onclick={() => (app.settingsOpen = "clubs")}>Clubs</button>.</p>
{:else}
  <div class="row">
    <select bind:value={clubId}>{#each app.clubs as c}<option value={c.id}>{c.name}</option>{/each}</select>
  </div>
  {#if club && clubStyles.length && lib?.styles.length}
    <p class="dim small">
      When a song fits several of {club.name}'s styles, it shows the ones your styles on it are called there: by your
      rules below, or by the same name.
    </p>
    <table>
      <tbody>
        {#each rules as r (r.mine + "→" + r.club)}
          {@const gone = !lib.styles.some((m) => m.key === r.mine) || !(r.club in (club?.style ?? {}))}
          <tr>
            <td>What I call <b style:color={lib.styles.find((m) => m.key === r.mine)?.color}>{app.styleName(r.mine)}</b>,</td>
            <td>{club.name} may call <b>{clubStyleName(r.club)}</b>{#if gone}<span class="warn"> · not a style any more</span>{/if}</td>
            <td><button class="plain icon danger" title="Delete rule" onclick={() => setRule(r.mine, r.club, false)}><Icon name="trash" size={12} /></button></td>
          </tr>
        {/each}
        {#each sameNamed.filter((p) => !rules.some((r) => r.mine === p.mine && r.club === p.club)) as p (p.mine + "=" + p.club)}
          <tr class="faint">
            <td>What I call <b>{app.styleName(p.mine)}</b>,</td>
            <td>{club.name} calls <b>{clubStyleName(p.club)}</b> · same name</td>
            <td></td>
          </tr>
        {/each}
        {#if !rules.length && !sameNamed.length}
          <tr><td colspan="3" class="faint">No rules yet: songs that fit several of {club.name}'s styles show all of them.</td></tr>
        {/if}
      </tbody>
    </table>
    <form class="row add" onsubmit={(e) => { e.preventDefault(); setRule(ruleMine, ruleClub, true); ruleClub = ""; }}>
      What I call
      <select bind:value={ruleMine}>
        <option value="">—</option>
        {#each lib.styles as m (m.key)}<option value={m.key}>{m.name}</option>{/each}
      </select>
      {club.name} may call
      <select bind:value={ruleClub}>
        <option value="">—</option>
        {#each clubStyles as [id, c] (id)}<option value={id}>{styleDef(id, c).name}</option>{/each}
      </select>
      <button type="submit" disabled={!ruleMine || !ruleClub || rules.some((r) => r.mine === ruleMine && r.club === ruleClub)}><Icon name="plus" size={12} /> Add Rule</button>
    </form>
  {:else if club && !clubStyles.length}
    <p class="dim">{club.name} has no styles yet.</p>
  {/if}
{/if}

<style>
  h2 { margin: 0 0 8px; font: 600 var(--fs-lg) var(--font-display); }
  h3 { margin: 18px 0 6px; font-size: var(--fs-md); }
  p { margin: 0 0 14px; max-width: 720px; }
  table { border-collapse: collapse; font-size: var(--fs-sm); margin-bottom: 6px; }
  th { text-align: left; color: var(--text-faint); font-weight: 600; padding: 4px; border-bottom: 1px solid var(--border); }
  td { padding: 3px 4px; border-bottom: 1px solid rgba(255, 255, 255, 0.04); }
  td input[type="text"] { width: 180px; }
  .w5 { width: 70px !important; }
  input[type="color"] { width: 26px; height: 22px; padding: 0; border: none; background: none; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; margin-bottom: 6px; }
  .add input { width: 200px; }
  .faint { color: var(--text-faint); }
  .small { font-size: var(--fs-sm); }
  .danger { color: var(--red); }
  .warn { color: var(--yellow); }
  .link { padding: 0; height: auto; color: var(--blue); display: inline; }
</style>

<script lang="ts">
  // Create / edit a smart crate: a crate whose songs are whatever in the
  // library matches its rules (e.g. "in Waltzes and tempo fits Rotary Waltz").
  import { api } from "../lib/api";
  import { SMART_FIELDS, defaultCondition, evaluate, fieldDef, type SmartCondition, type SmartField, type SmartRules } from "../lib/smart";
  import { app } from "../lib/state.svelte";
  import { fmtTime } from "../lib/tempo";
  import Icon from "./Icon.svelte";

  const editing = $derived(app.smartEditor && app.smartEditor !== "new" ? app.crates.find((c) => c.id === app.smartEditor) : undefined);
  // Seeded once from the crate being edited (or a sensible default).
  const init = (() => {
    const c = app.smartEditor && app.smartEditor !== "new" ? app.crates.find((x) => x.id === app.smartEditor) : undefined;
    const style = app.filterStyle;
    return {
      name: c?.name ?? (style ? `${app.styleName(style)} (smart)` : "New smart crate"),
      rules: c?.smart
        ? (structuredClone($state.snapshot(c.smart)) as SmartRules)
        : ({ match: "all", conditions: [style ? { field: "style", op: "is", value: style } : defaultCondition("style")] } as SmartRules),
    };
  })();
  let name = $state(init.name);
  let rules = $state<SmartRules>(init.rules);

  const preview = $derived(
    evaluate($state.snapshot(rules) as SmartRules, app.rows, { ...app.smartCtx, crates: app.crates.filter((c) => c.id !== editing?.id) }),
  );
  const tags = $derived(app.myTags);
  const otherCrates = $derived(app.crates.filter((c) => c.id !== editing?.id));

  function setField(c: SmartCondition, f: SmartField) {
    Object.assign(c, defaultCondition(f));
    if (f === "crate") c.value = otherCrates[0]?.id ?? null;
    if (f === "tag") c.value = tags[0]?.key ?? null;
    if (f === "bpm" || f === "style") c.value = app.filterStyle ?? app.myStyles[0]?.key ?? null;
  }

  function setOp(c: SmartCondition, op: string) {
    const input = fieldDef(c.field).ops.find((o) => o.op === op)?.input;
    c.op = op;
    if (input === "range" && !Array.isArray(c.value)) c.value = [100, 120];
    if ((input === "number" || input === "days") && typeof c.value !== "number") c.value = c.field === "duration" ? 210 : 30;
    if (input === "style" && typeof c.value !== "string") c.value = app.myStyles[0]?.key ?? null;
    if (input === "source" && c.value !== "file" && c.value !== "spotify") c.value = "file";
  }

  async function save() {
    const r = $state.snapshot(rules) as SmartRules;
    if (editing) {
      if (name.trim() && name !== editing.name) await app.run(api.renameCrate(editing.id, name.trim()));
      await app.run(api.setCrateSmart(editing.id, r));
    } else {
      const id = await app.run(api.createCrate(name.trim() || "Smart crate", r));
      if (id) {
        app.view = { kind: "crate", id };
        app.styleFilter = [];
      }
    }
    app.smartEditor = false;
  }
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (app.smartEditor = false)}>
  <form class="sheet" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <h2><Icon name="auto" size={18} /> {editing ? "Edit Smart Crate" : "New Smart Crate"}</h2>
    <div class="row">
      <span class="l">Name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" bind:value={name} autofocus style="flex:1" />
    </div>
    <div class="row">
      <span class="l"></span>
      Songs matching
      <select bind:value={rules.match}><option value="all">all</option><option value="any">any</option></select>
      of these rules:
    </div>
    <div class="conds">
      {#each rules.conditions as c, i (i)}
        {@const op = fieldDef(c.field).ops.find((o) => o.op === c.op)}
        <div class="cond">
          <select value={c.field} onchange={(e) => setField(c, e.currentTarget.value as SmartField)}>
            {#each SMART_FIELDS as f}<option value={f.field}>{f.label}</option>{/each}
          </select>
          <select value={c.op} onchange={(e) => setOp(c, e.currentTarget.value)}>
            {#each fieldDef(c.field).ops as o}<option value={o.op}>{o.label}</option>{/each}
          </select>
          {#if op?.input === "style"}
            <select bind:value={c.value}>
              {#each app.myStyles as s}<option value={s.key}>{s.name}</option>{/each}
            </select>
          {:else if op?.input === "number" || op?.input === "days"}
            <input type="number" min="0" value={c.value as number} oninput={(e) => (c.value = +e.currentTarget.value)} />
            {#if c.field === "duration" && typeof c.value === "number"}<span class="faint num">{fmtTime(c.value * 1000)}</span>{/if}
          {:else if op?.input === "range" && Array.isArray(c.value)}
            <input type="number" value={c.value[0]} oninput={(e) => (c.value = [+e.currentTarget.value, (c.value as [number, number])[1]])} />–
            <input type="number" value={c.value[1]} oninput={(e) => (c.value = [(c.value as [number, number])[0], +e.currentTarget.value])} />
          {:else if op?.input === "text"}
            <input type="text" value={(c.value as string) ?? ""} oninput={(e) => (c.value = e.currentTarget.value)} style="flex:1" />
          {:else if op?.input === "source"}
            <select bind:value={c.value}><option value="file">Local file</option><option value="spotify">Spotify</option></select>
          {:else if op?.input === "crate"}
            <select bind:value={c.value}>
              {#each otherCrates as x}<option value={x.id}>{x.name}</option>{/each}
            </select>
          {:else if op?.input === "tag"}
            <select bind:value={c.value}>
              {#each tags as t}<option value={t.key}>{t.name}</option>{/each}
            </select>
          {/if}
          <span class="spacer"></span>
          <button type="button" class="plain icon" title="Remove rule" disabled={rules.conditions.length < 2}
            onclick={() => rules.conditions.splice(i, 1)}><Icon name="minus" size={13} /></button>
          <button type="button" class="plain icon" title="Add rule" onclick={() => rules.conditions.splice(i + 1, 0, defaultCondition("bpm"))}><Icon name="plus" size={13} /></button>
        </div>
      {/each}
    </div>
    <div class="preview">
      <span class="faint">{preview.length} song{preview.length === 1 ? "" : "s"} match now</span>
      {#each preview.slice(0, 5) as r (r.version.id)}<span class="pv">{r.track.title}</span>{/each}
      {#if preview.length > 5}<span class="faint">…</span>{/if}
    </div>
    <div class="buttons">
      <button type="button" onclick={() => (app.smartEditor = false)}>Cancel</button>
      <button type="submit" class="primary">{editing ? "Save" : "Create"}</button>
    </div>
  </form>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 80; background: rgba(0,0,0,0.45); display: flex; justify-content: center; align-items: flex-start; }
  .sheet { margin-top: 60px; width: 720px; max-height: 80vh; overflow-y: auto; padding: 20px 22px; background: var(--bg-elev); border: 1px solid var(--border-strong); border-radius: 12px; box-shadow: var(--shadow); display: flex; flex-direction: column; gap: 10px; }
  h2 { margin: 0 0 4px; font: 600 var(--fs-lg) var(--font-display); display: flex; align-items: center; gap: 8px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .l { width: 50px; color: var(--text-dim); text-align: right; }
  .conds { display: flex; flex-direction: column; gap: 6px; padding: 8px; background: var(--bg-elev-2); border-radius: 8px; border: 1px solid var(--border); }
  .cond { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .spacer { flex: 1; }
  .preview { display: flex; flex-wrap: wrap; gap: 6px; font-size: var(--fs-sm); align-items: center; min-height: 20px; }
  .pv { background: var(--bg-elev-3); border-radius: 4px; padding: 1px 6px; max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
</style>

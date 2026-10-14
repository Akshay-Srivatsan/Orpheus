<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import type { SetlistMode } from "../lib/types";

  const today = new Date().toISOString().slice(0, 10);
  let name = $state("");
  let mode = $state<SetlistMode>("preset");
  let profileId = $state(app.settings?.activeProfileId ?? "dancebreak");
  let date = $state(today);
  let lesson = $state("");
  const profile = $derived(app.profiles.find((p) => p.id === profileId));

  async function create() {
    const set = await app.run(api.createSetlist({
      name: name.trim() || `${profile?.name ?? "Social"} — ${date}`, mode, profileId, lessonStyle: lesson || null, eventDate: date || null,
    }));
    if (!set) return;
    await app.run(api.performOpen(set.id));
    app.newSetlistOpen = false;
    app.rightTab = "set";
  }
</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (app.newSetlistOpen = false)}>
  <form class="sheet" onsubmit={(e) => { e.preventDefault(); create(); }}>
    <h2>New Setlist</h2>
    <div class="form">
      <span class="l">Name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" bind:value={name} placeholder="{profile?.name ?? 'Social'} — {date}" autofocus />
      <span class="l">Date</span>
      <input type="date" bind:value={date} />
      <span class="l">Club</span>
      <select bind:value={profileId}>{#each app.profiles as p}<option value={p.id}>{p.name}</option>{/each}</select>
      <span class="l">Lesson</span>
      <select bind:value={lesson}>
        <option value="">No lesson</option>
        {#each profile?.styles ?? [] as s}<option value={s.key}>{s.name}</option>{/each}
      </select>
      <span class="l">Style</span>
      <div class="col">
        <label><input type="radio" bind:group={mode} value="preset" /> <b>Preset</b> — plan the songs ahead; the projector shows the whole setlist</label>
        <label><input type="radio" bind:group={mode} value="live" /> <b>Live</b> — pick as you go; the projector shows now playing and recent songs</label>
        <span class="faint small">You can mix both: add style slots now and choose songs during the event.</span>
      </div>
    </div>
    <div class="buttons">
      <button type="button" onclick={() => (app.newSetlistOpen = false)}>Cancel</button>
      <button type="submit" class="primary">Create</button>
    </div>
  </form>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 80; background: rgba(0,0,0,0.45); display: flex; justify-content: center; align-items: flex-start; }
  .sheet { margin-top: 60px; width: 560px; padding: 20px 22px; background: var(--bg-elev); border: 1px solid var(--border-strong); border-radius: 12px; box-shadow: var(--shadow); }
  h2 { margin: 0 0 14px; font: 600 var(--fs-lg) var(--font-display); }
  .form { display: grid; grid-template-columns: 70px 1fr; gap: 10px 12px; align-items: center; }
  .l { color: var(--text-dim); text-align: right; }
  .col { display: flex; flex-direction: column; gap: 6px; }
  .col label { align-items: flex-start; }
  .small { font-size: var(--fs-sm); }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
</style>

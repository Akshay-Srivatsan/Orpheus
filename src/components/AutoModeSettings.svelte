<script lang="ts">
  // Settings → Auto Mode: starting songs automatically, and spoken
  // announcements of the next dance (Kokoro text-to-speech). All off by default.
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { app, confirmAction } from "../lib/state.svelte";
  import type { AnnounceSettings, Pronunciation, Settings, StyleLibrary, VoiceInfo } from "../lib/types";
  import Icon from "./Icon.svelte";

  const s = $derived(app.settings!);
  const a = $derived(s.announce);
  const tts = $derived(app.tts);
  /** The voice in use (an old Piper voice name falls back to Kokoro's default). */
  const voice = $derived(tts?.voice || a.voice);
  let voices = $state<VoiceInfo[] | null>(null);
  let voicesError = $state<string | null>(null);
  let language = $state("");
  let sample = $state("");

  onMount(() => {
    api.ttsVoices()
      .then((v) => {
        voices = v;
        language = v.find((x) => x.key === voice)?.language ?? "";
      })
      .catch((e) => (voicesError = String(e)));
  });

  const languages = $derived([...new Set((voices ?? []).map((v) => v.language))]);
  const shownVoices = $derived((voices ?? []).filter((v) => !language || v.language === language));
  const installing = $derived(tts?.state === "installing" ? tts : null);

  function setAnnounce(patch: Partial<AnnounceSettings>) {
    app.saveSettings({ announce: { ...$state.snapshot(a), ...patch } } as Partial<Settings>);
  }

  function setPattern(i: number, text: string) {
    const patterns = [...a.patterns];
    patterns[i] = text;
    setAnnounce({ patterns });
  }

  // ---------------------------------------------------------------- pronunciations (in styles.json)
  const prons = $derived(app.styleLib?.pronunciations ?? []);
  let newWord = $state("");
  let shown = $state<Record<number, string>>({});

  function saveProns(next: Pronunciation[]) {
    const lib = app.styleLib;
    if (!lib) return;
    const { usage, tagUsage, ...rest } = $state.snapshot(lib);
    const saved: StyleLibrary = { ...structuredClone(rest), pronunciations: next };
    app.styleLib = { ...saved, usage, tagUsage };
    app.run(api.saveStyles(saved));
  }

  function setPron(i: number, patch: Partial<Pronunciation>) {
    shown = {};
    saveProns(prons.map((p, j) => (j === i ? { ...p, ...patch } : p)));
  }

  async function showPhonemes(i: number) {
    const p = await app.run(api.ttsPhonemes(prons[i].word));
    if (p != null) shown = { ...shown, [i]: p };
  }

  function trySay() {
    const text = sample.trim() || (a.patterns.find((p) => p.trim()) ?? "The next dance is a {dance}.").replace("{dance}", "Rotary Waltz");
    app.run(api.ttsSay(text));
  }

  function mb(bytes: number) {
    return `${(bytes / 1e6).toFixed(0)} MB`;
  }
</script>

<h2>Auto Mode</h2>
<p class="dim">
  Auto mode starts the next song by itself after the pause between songs, and can announce the next dance first:
  the song ends, a pause, the announcement, another pause, then the next song. Mark songs in a setlist with
  <b>Auto Mode Stops Before This Song</b> (right-click) to start those yourself with Play Next. Everything here is off unless you turn it on.
</p>

<div class="form">
  <span class="l">Next song</span>
  <span class="col">
    <label><input type="radio" name="adv" checked={!(s.autoAdvance ?? app.club?.set.auto_advance)} onchange={() => app.saveSettings({ autoAdvance: false })} /> Manual: start each song with Play Next</label>
    <label><input type="radio" name="adv" checked={!!(s.autoAdvance ?? app.club?.set.auto_advance)} onchange={() => app.saveSettings({ autoAdvance: true })} /> Automatic (auto mode)</label>
    <span class="faint small">The <b>Auto</b> button under Play Next switches it for the current session.</span>
  </span>

  <span class="l">Announcements</span>
  <span class="col">
    <label><input type="checkbox" checked={a.enabled} onchange={(e) => setAnnounce({ enabled: e.currentTarget.checked })} /> Announce the next dance before starting it</label>
    {#if !a.enabled}<span class="faint small">Without announcements, auto mode waits for the “pause between songs” (Settings → General).</span>{/if}
  </span>

  <span class="l">Timing</span>
  <span class="col">
    <span>Pause <input type="number" min="0" step="0.5" value={a.pauseBeforeSecs} onchange={(e) => setAnnounce({ pauseBeforeSecs: +e.currentTarget.value })} /> s,
      announcement, pause <input type="number" min="0" step="0.5" value={a.pauseAfterSecs} onchange={(e) => setAnnounce({ pauseAfterSecs: +e.currentTarget.value })} /> s, next song</span>
  </span>

  <span class="l">What to say</span>
  <span class="col">
    <span class="faint small">One is picked at random each time (not the same twice in a row). <code>{"{dance}"}</code> becomes the next dance, as the projector names it.</span>
    {#each a.patterns as p, i (i)}
      <span class="row">
        <input type="text" class="pattern" value={p} onchange={(e) => setPattern(i, e.currentTarget.value)} />
        <button class="plain icon danger" title="Remove" onclick={() => setAnnounce({ patterns: a.patterns.filter((_, j) => j !== i) })}><Icon name="x" size={12} /></button>
      </span>
    {/each}
    <button class="small" onclick={() => setAnnounce({ patterns: [...a.patterns, "Next up: {dance}."] })}><Icon name="plus" size={12} /> Add Sentence</button>
  </span>

  <span class="l">Voice</span>
  <span class="col">
    {#if voices}
      <span class="row">
        <select bind:value={language}>
          <option value="">All languages</option>
          {#each languages as l}<option value={l}>{l}</option>{/each}
        </select>
        <select value={voice} onchange={(e) => setAnnounce({ voice: e.currentTarget.value })}>
          {#if !shownVoices.some((v) => v.key === voice)}<option value={voice}>{voice}</option>{/if}
          {#each shownVoices as v}
            <option value={v.key}>{v.name} ({v.gender}){tts?.installedVoices.includes(v.key) ? " ✓" : ""}</option>
          {/each}
        </select>
      </span>
    {:else if voicesError}
      <span class="warn small">Couldn't load Kokoro's voice list ({voicesError}). Voice: {voice}</span>
    {:else}
      <span class="faint small">Loading voices…</span>
    {/if}
    <span class="row">
      {#if installing}
        <span class="num small">Downloading {installing.what}… {installing.total ? `${Math.round((installing.done / installing.total) * 100)}%` : mb(installing.done)}</span>
      {:else if tts?.state === "ready"}
        <span class="ok small"><Icon name="check" size={12} /> Ready</span>
        <button class="small" onclick={() => confirmAction("Delete the announcer's model and voices from this computer?").then((ok) => { if (ok) app.run(api.ttsRemove()); })}>Remove…</button>
      {:else}
        <button class="primary small" onclick={() => app.run(api.ttsInstall())}>Download {tts?.installedVoices.length ? "This Voice" : `the Voice (${a.model === "fp32" ? "325" : "160"} MB)`}</button>
        {#if tts?.state === "error"}<span class="err small">{tts.message}</span>{/if}
      {/if}
    </span>
    <span class="row small">Quality
      <select value={a.model} onchange={(e) => setAnnounce({ model: e.currentTarget.value as AnnounceSettings["model"] })}>
        <option value="fp16">Standard (160 MB)</option>
        <option value="fp32">Full precision (325 MB)</option>
      </select>
    </span>
    <span class="faint small">Announcements are spoken by <a href="https://huggingface.co/hexgrad/Kokoro-82M" target="_blank" rel="noreferrer">Kokoro</a>, an open-source text-to-speech model that runs inside Orpheus on this computer. Downloading needs the internet once.</span>
  </span>

  <span class="l">Pronunciation</span>
  <span class="col">
    <span class="faint small">
      How announcements say words they get wrong, e.g. dance names. Write it the way it sounds (“shot-EESH”), or give
      phonemes between slashes (<code>/ʃɑtˈiʃ/</code>); “phonemes” shows what the voice reads now. Saved with your library.
    </span>
    {#if prons.length}
      <table class="prons">
        <thead><tr><th>Word or phrase</th><th>Say as</th><th></th></tr></thead>
        <tbody>
          {#each prons as p, i (i)}
            <tr>
              <td><input type="text" value={p.word} onchange={(e) => setPron(i, { word: e.currentTarget.value.trim() })} /></td>
              <td><input type="text" value={p.say} placeholder="e.g. shot-eesh" onchange={(e) => setPron(i, { say: e.currentTarget.value.trim() })} /></td>
              <td class="nowrap">
                <button class="plain small" disabled={tts?.state !== "ready" || !p.word} title="Say it with this pronunciation"
                  onclick={() => app.run(api.ttsSay(p.word))}><Icon name="speaker" size={12} /></button>
                <button class="plain small" disabled={tts?.state !== "ready" || !p.word} onclick={() => showPhonemes(i)}>phonemes</button>
                <button class="plain icon danger" title="Remove" onclick={() => saveProns(prons.filter((_, j) => j !== i))}><Icon name="x" size={12} /></button>
              </td>
            </tr>
            {#if shown[i]}<tr><td></td><td colspan="2" class="ph"><code>{shown[i]}</code></td></tr>{/if}
          {/each}
        </tbody>
      </table>
    {/if}
    <form class="row" onsubmit={(e) => { e.preventDefault(); if (newWord.trim()) { saveProns([...prons, { word: newWord.trim(), say: "" }]); newWord = ""; } }}>
      <input type="text" placeholder="Word or phrase" bind:value={newWord} />
      <button type="submit" class="small" disabled={!newWord.trim()}><Icon name="plus" size={12} /> Add Pronunciation</button>
    </form>
  </span>

  <span class="l">Sound</span>
  <span class="col">
    <span class="row">Speed <input type="range" min="0.7" max="1.4" step="0.05" value={a.speed || 1}
      onchange={(e) => setAnnounce({ speed: +e.currentTarget.value })} />
      Volume <input type="range" min="0" max="1.5" step="0.05" value={a.volume} onchange={(e) => setAnnounce({ volume: +e.currentTarget.value })} />
      <span class="num faint">{Math.round(a.volume * 100)}%</span></span>
    <span class="row">
      <input type="text" class="pattern" placeholder="Try a sentence…" bind:value={sample} />
      <button class="small" disabled={tts?.state !== "ready"} onclick={trySay}><Icon name="speaker" size={12} /> Say It</button>
    </span>
    <span class="faint small">Announcements play in the room and in your headphones.</span>
  </span>

</div>

<style>
  h2 { margin: 0 0 8px; font: 600 var(--fs-lg) var(--font-display); }
  p { margin: 0 0 14px; max-width: 720px; }
  .form { display: grid; grid-template-columns: 170px 1fr; gap: 14px 14px; align-items: start; }
  .l { color: var(--text-dim); text-align: right; padding-top: 4px; }
  .col { display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  input[type="number"] { width: 60px; }
  .pattern { width: 420px; }
  .small { font-size: var(--fs-sm); }
  .ok { color: var(--green); display: inline-flex; gap: 4px; align-items: center; }
  .err { color: var(--red); }
  .warn { color: var(--yellow); }
  .danger { color: var(--red); }
  .prons { border-collapse: collapse; font-size: var(--fs-sm); }
  .prons th { text-align: left; color: var(--text-faint); font-weight: 600; padding: 2px 4px; }
  .prons td { padding: 2px 4px; }
  .prons input { width: 200px; }
  .nowrap { white-space: nowrap; }
  .ph code { user-select: text; }
  code { font: var(--fs-sm) var(--font-mono); background: var(--bg); padding: 1px 5px; border-radius: 4px; }
</style>

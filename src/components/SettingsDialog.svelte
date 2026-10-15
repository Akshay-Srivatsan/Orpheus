<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { app, confirmAction } from "../lib/state.svelte";
  import type { AudioDevice, MidiControl, MidiInfo, MidiMapping, Settings } from "../lib/types";
  import Icon from "./Icon.svelte";
  import StylesSettings from "./StylesSettings.svelte";
  import AutoModeSettings from "./AutoModeSettings.svelte";
  import ClubsSettings from "./ClubsSettings.svelte";

  const tabs = [
    ["general", "General"], ["library", "Library"], ["audio", "Audio"], ["midi", "Controllers"],
    ["stems", "Stems"], ["spotify", "Spotify"], ["projector", "Projector"], ["auto", "Auto Mode"], ["styles", "Styles & Tags"], ["clubs", "Clubs"],
  ] as const;
  let tab = $state<string>(typeof app.settingsOpen === "string" ? app.settingsOpen : "general");
  const s = $derived(app.settings!);

  let devices = $state<AudioDevice[]>([]);
  let midi = $state<MidiInfo | null>(null);
  let learning = $state<number | null>(null);
  let spotifyCache = $state<number | null>(null);

  $effect(() => {
    if (tab === "audio") api.audioDevices().then((d) => (devices = d));
    if (tab === "midi") api.midiInfo().then((m) => (midi = m));
    if (tab === "library") api.libraryInfo().then((l) => (app.library = l));
    if (tab === "spotify") api.spotifyCacheBytes().then((b) => (spotifyCache = b));
  });

  function set<K extends keyof Settings>(k: K, v: Settings[K]) {
    app.saveSettings({ [k]: v } as Partial<Settings>);
  }
  function setAudio(patch: Partial<Settings["audio"]>) {
    set("audio", { ...$state.snapshot(s.audio), ...patch });
  }
  function setProj(patch: Partial<Settings["projector"]>) {
    set("projector", { ...$state.snapshot(s.projector), ...patch });
  }

  function pairs(channels: number): [number, number][] {
    const out: [number, number][] = [];
    for (let i = 0; i + 1 < Math.max(2, channels); i += 2) out.push([i, i + 1]);
    return out;
  }
  const masterDev = $derived(devices.find((d) => d.id === s.audio.masterDevice) ?? devices.find((d) => d.isDefault));
  const cueDev = $derived(devices.find((d) => d.id === s.audio.cueDevice));

  async function addFolder() {
    const dir = await openDialog({ directory: true, multiple: false, title: "Add music folder" });
    if (typeof dir === "string" && !s.libraryFolders.includes(dir)) set("libraryFolders", [...s.libraryFolders, dir]);
  }
  async function chooseLibraryDir() {
    const dir = await openDialog({ directory: true, multiple: false, title: "Choose library folder (e.g. a git repository)" });
    if (typeof dir === "string") set("libraryDir", dir);
  }
  async function reloadLibrary() {
    const errs = await app.run(api.reloadLibrary());
    app.library = (await app.run(api.libraryInfo())) ?? app.library;
    app.toast(errs?.length ? `Reloaded with ${errs.length} problem(s)` : "Library reloaded", errs?.length ? "error" : "info");
  }

  // ---------------------------------------------------------------- MIDI mapping editor
  const mappings = $derived(midi?.mappings ?? []);
  /** The mapping in use; with automatic selection, the one chosen for the connected controller. */
  const mapping = $derived(mappings.find((m) => m.id === (s.midi.mappingId || midi?.activeMapping)) ?? mappings[0]);
  const hex = (n: number) => "0x" + n.toString(16).toUpperCase().padStart(2, "0");

  function saveMapping(m: MidiMapping) {
    const custom = s.midi.customMappings.filter((x) => x.id !== m.id);
    set("midi", { ...$state.snapshot(s.midi), customMappings: [...custom, { ...m, builtin: false }], mappingId: m.id });
    if (midi) midi = { ...midi, mappings: [...midi.mappings.filter((x) => x.id !== m.id), m] };
  }
  function editable(): MidiMapping {
    const m = $state.snapshot(mapping) as MidiMapping;
    if (!m.builtin) return m;
    return { ...m, id: `${m.id}-custom-${Date.now().toString(36)}`, name: `${m.name} (custom)`, builtin: false };
  }
  async function learn(i: number | null) {
    learning = i ?? -1;
    const msg = await app.run(api.midiLearn());
    learning = null;
    if (!msg) return app.toast("No MIDI message received", "info");
    const m = editable();
    const ctrl: MidiControl = { status: msg.status, data1: msg.data1, kind: msg.kind, action: i == null ? "perform.playNext" : m.controls[i].action };
    if (i == null) m.controls.push(ctrl);
    else m.controls[i] = { ...m.controls[i], status: msg.status, data1: msg.data1 };
    saveMapping(m);
  }
  function updateControl(i: number, patch: Partial<MidiControl>) {
    const m = editable();
    m.controls[i] = { ...m.controls[i], ...patch };
    saveMapping(m);
  }
  function removeControl(i: number) {
    const m = editable();
    m.controls.splice(i, 1);
    saveMapping(m);
  }

</script>

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (app.settingsOpen = false)}>
  <div class="dialog" role="dialog" aria-label="Settings">
    <nav>
      {#each tabs as [id, label]}
        <button class="tab" class:sel={tab === id} onclick={() => (tab = id)}>{label}</button>
      {/each}
    </nav>
    <div class="content">
      <button class="plain icon close" onclick={() => (app.settingsOpen = false)} title="Close (Esc)"><Icon name="x" /></button>

      {#if tab === "general"}
        <h2>General</h2>
        <div class="form">
          <span class="l" title="Club for new setlists (each setlist can choose its own)">Default club</span>
          <span class="col">
            <select value={s.defaultClub} onchange={(e) => set("defaultClub", e.currentTarget.value)}>
              <option value="">No club (your own styles)</option>
              {#each app.clubs as c}<option value={c.id}>{c.name}</option>{/each}
            </select>
            <button class="plain small link" onclick={() => (tab = "clubs")}>Clubs and their guidelines…</button>
          </span>
          <span class="l">Pause between songs</span>
          <span><input type="number" min="0" step="1" placeholder={String(app.club?.set.pause.target ?? 8)} value={s.gapSeconds ?? ""}
            onchange={(e) => set("gapSeconds", e.currentTarget.value ? +e.currentTarget.value : null)} /> seconds <span class="faint">(empty = club default)</span></span>
          <span class="l">Next song</span>
          <span class="col">
            <span>{(s.autoAdvance ?? app.club?.set.auto_advance) ? "Automatic (auto mode)" : "Manual: start each song with Play Next"}</span>
            <button class="plain small link" onclick={() => (tab = "auto")}>Auto mode and announcements…</button>
          </span>
          <span class="l">Play Next while playing</span>
          <span class="col">
            <label><input type="checkbox" checked={s.confirmNextWhilePlaying} onchange={(e) => set("confirmNextWhilePlaying", e.currentTarget.checked)} /> Require a second press</label>
            <span>Fade the current song over <input type="number" min="0" step="0.5" value={s.manualNextFadeMs / 1000}
              onchange={(e) => set("manualNextFadeMs", Math.round(+e.currentTarget.value * 1000))} /> s first</span>
          </span>
          <span class="l">Decks</span>
          <span class="col">
            <label><input type="checkbox" checked={s.autoLoadNext} onchange={(e) => set("autoLoadNext", e.currentTarget.checked)} /> Load the next song in the set onto the free deck automatically</label>
            <label><input type="checkbox" checked={s.protectOnAir} onchange={(e) => set("protectOnAir", e.currentTarget.checked)} /> Protect the on-air deck: ignore jog wheels, CUE and cue jumps while the room hears it</label>
            <label><input type="checkbox" checked={s.headphonesFollowFreeDeck} onchange={(e) => set("headphonesFollowFreeDeck", e.currentTarget.checked)} /> Switch headphones to the free deck when a deck goes on air</label>
          </span>
          <span class="l">Crossfader</span>
          <span class="col">
            <span>Snaps to the center within ± <input type="number" min="0" max="25" step="1" value={Math.round(s.crossfaderSnap * 100)}
              onchange={(e) => set("crossfaderSnap", +e.currentTarget.value / 100)} /> %</span>
            <span class="faint small">Orpheus never moves the crossfader. Play Next, Go Live and auto mode need it centered, or on the side of the deck they start; otherwise they show an error and the controller's lights flash.</span>
          </span>
          <span class="l">Fade-out length</span>
          <span><input type="number" min="0.5" step="0.5" placeholder={String(app.club?.set.fade_out ?? 5)} value={s.fadeOutMs != null ? s.fadeOutMs / 1000 : ""}
            onchange={(e) => set("fadeOutMs", e.currentTarget.value ? Math.round(+e.currentTarget.value * 1000) : null)} /> seconds</span>
          <span class="l">Tempo</span>
          <span class="col">
            <label><input type="checkbox" checked={s.keylock} onchange={(e) => set("keylock", e.currentTarget.checked)} /> Keep pitch when changing tempo (key lock)</label>
            <span title="How tempo changes are computed; applies to songs loaded from now on">Time-stretch engine
              <select value={s.stretchEngine || "signalsmith"} onchange={(e) => set("stretchEngine", e.currentTarget.value as Settings["stretchEngine"])}>
                <option value="signalsmith">Signalsmith Stretch (default, smooth)</option>
                <option value="rubberband">Rubber Band R3 (high quality, more CPU)</option>
                <option value="soundtouch">SoundTouch (crisp rhythm)</option>
                <option value="bungee">Bungee</option>
              </select>
            </span>
            <span class="faint small">If slowed-down songs sound smeared or phasey, try Rubber Band or SoundTouch. Changes apply to the next song loaded.</span>
            <span>Tempo slider range ± <input type="number" min="2" max="50" value={s.tempoRangePct} onchange={(e) => set("tempoRangePct", +e.currentTarget.value)} /> %</span>
          </span>
          <span class="l">Levels</span>
          <span class="col">
            <label><input type="checkbox" checked={s.autoGain} onchange={(e) => set("autoGain", e.currentTarget.checked)} /> Level songs automatically to
              <input type="number" step="1" value={s.targetLoudnessDb} onchange={(e) => set("targetLoudnessDb", +e.currentTarget.value)} /> LUFS</label>
            <label><input type="checkbox" checked={s.trimSilence} onchange={(e) => set("trimSilence", e.currentTarget.checked)} /> Skip silence at the start and end of songs</label>
          </span>
        </div>

      {:else if tab === "library"}
        <h2>Library</h2>
        <p class="dim">Your library (songs, versions, dance styles, setlists, crates, club profiles) is stored as plain JSON files in one folder.
          Put it in a git repository or a synced folder to share it between DJs. Music files themselves are never modified.</p>
        <div class="form">
          <span class="l">Library folder</span>
          <span class="col">
            <code>{app.library?.libraryDir}</code>
            <span class="row">
              <button onclick={chooseLibraryDir}>Choose…</button>
              {#if s.libraryDir}<button class="plain" onclick={() => set("libraryDir", null)}>Use default</button>{/if}
              <button onclick={reloadLibrary} title="Re-read the library folder, e.g. after git pull"><Icon name="reload" size={13} /> Reload</button>
            </span>
          </span>
          <span class="l">Music folders</span>
          <span class="col">
            {#each s.libraryFolders as f}
              <span class="row"><code>{f}</code><button class="plain icon" title="Remove" onclick={() => set("libraryFolders", s.libraryFolders.filter((x) => x !== f))}><Icon name="x" size={12} /></button></span>
            {:else}
              <span class="faint">No folders yet.</span>
            {/each}
            <span class="row">
              <button onclick={addFolder}><Icon name="plus" size={13} /> Add Folder…</button>
              <button onclick={() => api.rescan()} disabled={app.scan?.running}><Icon name="reload" size={13} /> Rescan</button>
              {#if app.scan}<span class="faint small">{app.scan.running ? `${app.scan.phase}…` : `Last scan: ${app.scan.added} new, ${app.scan.relinked} moved, ${app.scan.removed} gone`}</span>{/if}
            </span>
            <span class="faint small">Files are recognised by their audio content, so renaming or reorganising folders keeps everything linked.</span>
          </span>
          <span class="l">This computer</span>
          <span class="col"><code>{app.library?.dataDir}</code><span class="faint small">File locations, waveforms, settings and the Spotify login stay here (not shared).</span></span>
          {#if app.library?.loadErrors.length}
            <span class="l warn">Problems</span>
            <span class="col">{#each app.library.loadErrors as e}<code class="err">{e}</code>{/each}</span>
          {/if}
        </div>

      {:else if tab === "audio"}
        <h2>Audio</h2>
        <p class="dim">The crossfader decides which deck the room hears; the headphone (cue) output plays whichever decks have their headphone button on. With a DJ controller such as the Numark DJ2GO2, choose it for both and use channels 1–2 for the room and 3–4 for headphones.</p>
        <div class="form">
          <span class="l">Room (master)</span>
          <span class="row">
            <select value={s.audio.masterDevice ?? ""} onchange={(e) => setAudio({ masterDevice: e.currentTarget.value || null })}>
              <option value="">System default</option>
              {#each devices as d}<option value={d.id}>{d.name}{d.isDefault ? " (default)" : ""}</option>{/each}
            </select>
            <select value={s.audio.masterChannels.join(",")} onchange={(e) => setAudio({ masterChannels: e.currentTarget.value.split(",").map(Number) as [number, number] })}>
              {#each pairs(masterDev?.channels ?? 2) as p}<option value={p.join(",")}>Channels {p[0] + 1}–{p[1] + 1}</option>{/each}
            </select>
            <button onclick={() => api.audioTest(app.liveDeck)} disabled={app.decks.some((d) => d.onAir && d.status === "playing")}>Test</button>
          </span>
          <span class="l">Headphones (cue)</span>
          <span class="row">
            <select value={s.audio.cueDevice ?? ""} onchange={(e) => setAudio({ cueDevice: e.currentTarget.value || null })}>
              <option value="">None</option>
              {#each devices as d}<option value={d.id}>{d.name}</option>{/each}
            </select>
            {#if s.audio.cueDevice}
              <select value={s.audio.cueChannels.join(",")} onchange={(e) => setAudio({ cueChannels: e.currentTarget.value.split(",").map(Number) as [number, number] })}>
                {#each pairs(cueDev?.channels ?? 2) as p}<option value={p.join(",")}>Channels {p[0] + 1}–{p[1] + 1}</option>{/each}
              </select>
              <button onclick={() => api.audioTest(app.freeDeck)} disabled={app.decks[app.freeDeck].onAir && app.decks[app.freeDeck].status === "playing"}>Test</button>
            {/if}
          </span>
          {#if app.outputs?.cueOnMasterChannels}
            <span></span><span class="warn">⚠ Headphones use the same channels as the room: previews will be heard by the dancers.</span>
          {/if}
          <span class="l">Buffer</span>
          <span><input type="number" min="20" max="500" step="10" value={s.audio.bufferMs} onchange={(e) => setAudio({ bufferMs: +e.currentTarget.value })} /> ms <span class="faint">(raise if you hear dropouts)</span></span>
          <span class="l">Status</span>
          <span class="col small">
            {#if app.outputs?.master}<span>Room: {app.outputs.master.device} · {app.outputs.master.sampleRate} Hz · {app.outputs.master.channels} ch</span>{/if}
            {#if app.outputs?.cue}<span>Headphones: {app.outputs.cue.device} · {app.outputs.cue.sampleRate} Hz</span>{/if}
            {#each app.outputs?.errors ?? [] as e}<span class="err">{e}</span>{/each}
            <span><button class="small" onclick={async () => (app.outputs = (await app.run(api.audioReopen())) ?? app.outputs)}>Reconnect devices</button></span>
          </span>
        </div>

      {:else if tab === "midi"}
        <h2>Controllers</h2>
        <div class="form">
          <span class="l">MIDI</span>
          <label><input type="checkbox" checked={s.midi.enabled} onchange={(e) => set("midi", { ...$state.snapshot(s.midi), enabled: e.currentTarget.checked })} /> Use MIDI controllers</label>
          <span class="l">Connected</span>
          <span class="col small">
            {#each midi?.ports ?? [] as p}<span><span class="led" class:ok={p.connected}></span> {p.name}</span>{:else}<span class="faint">No MIDI devices found. Plug one in; it will connect automatically.</span>{/each}
          </span>
          <span class="l">Mapping</span>
          <span class="row">
            <select value={s.midi.mappingId} onchange={(e) => set("midi", { ...$state.snapshot(s.midi), mappingId: e.currentTarget.value })}>
              <option value="">Automatic{s.midi.mappingId === "" && mapping ? ` (${mapping.name})` : ""}</option>
              {#each mappings as m}<option value={m.id}>{m.name}</option>{/each}
            </select>
            {#if mapping && !mapping.builtin && s.midi.mappingId === mapping.id}
              <button class="danger" onclick={async () => {
                if (!(await confirmAction(`Delete the mapping “${mapping.name}”?`))) return;
                const custom = s.midi.customMappings.filter((x) => x.id !== mapping.id);
                set("midi", { ...$state.snapshot(s.midi), customMappings: custom, mappingId: "" });
                if (midi) midi = { ...midi, mappings: midi.mappings.filter((x) => x.id !== mapping.id) };
              }}>Delete Mapping</button>
            {/if}
          </span>
        </div>
        {#if mapping}
          <p class="dim small">Left = deck A, right = deck B. SYNC = Go Live (on the deck the room hears: fade out). Pads: Cues mode = hot cues (hold PAD MODE to clear), Sampler mode = stems (vocals / drums / bass / other). Tempo faders use soft takeover; the crossfader always follows the hardware. {mapping.builtin ? "Editing a built-in mapping creates a custom copy." : ""}</p>
          <table class="mtable">
            <thead><tr><th>Message</th><th>Kind</th><th>Action</th><th></th></tr></thead>
            <tbody>
              {#each mapping.controls as c, i}
                <tr>
                  <td class="num">{hex(c.status)} {hex(c.data1)}</td>
                  <td>
                    <select value={c.kind} onchange={(e) => updateControl(i, { kind: e.currentTarget.value as MidiControl["kind"] })}>
                      <option value="button">Button</option><option value="hold">Button (press &amp; release)</option><option value="absolute">Fader/knob</option><option value="relative">Encoder/jog</option>
                    </select>
                    <label class="small"><input type="checkbox" checked={!!c.invert} onchange={(e) => updateControl(i, { invert: e.currentTarget.checked })} /> invert</label>
                  </td>
                  <td>
                    <select value={c.action} onchange={(e) => updateControl(i, { action: e.currentTarget.value })}>
                      {#each midi?.actions ?? [] as [a, label]}<option value={a}>{label}</option>{/each}
                    </select>
                  </td>
                  <td class="row">
                    <button class="small" onclick={() => learn(i)} disabled={learning != null}>{learning === i ? "Move a control…" : "Learn"}</button>
                    <button class="plain icon" onclick={() => removeControl(i)} title="Remove"><Icon name="x" size={12} /></button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <div class="row" style="margin-top:8px">
            <button onclick={() => learn(null)} disabled={learning != null}>{learning === -1 ? "Press or move a control on your controller…" : "Learn New Control"}</button>
            {#if learning != null}<button class="plain" onclick={() => api.midiLearnCancel()}>Cancel</button>{/if}
          </div>
        {/if}

      {:else if tab === "stems"}
        {@const st = app.stems}
        {@const jobs = Object.entries(st?.jobs ?? {})}
        <h2>Stems</h2>
        <p class="dim">Orpheus can split songs into vocals, drums, bass and other (HT-Demucs), so you can mute parts of a song,
          e.g. drop the vocals for a teaching track. Separation takes a minute or two per song and runs in the background;
          prepared stems are stored on this computer and work offline. On a controller, pads in Sampler mode toggle stems.
          Only local files can be separated (not Spotify songs).</p>
        <div class="form">
          <span class="l">Separation model</span>
          <span class="col">
            {#if st?.model.state === "ready"}
              <span class="row">Ready
                <button class="plain small" disabled={jobs.some(([, j]) => j.state === "running")}
                  onclick={async () => { if (await confirmAction("Delete the stem separation model (210 MB)? Prepared stems keep working; preparing new ones downloads it again.", "Delete")) app.run(api.stemsRemoveModel()); }}>Remove Model…</button>
              </span>
            {:else if st?.model.state === "downloading"}
              <span class="num">Downloading… {st.model.total ? `${Math.round((st.model.done / st.model.total) * 100)}%` : ""}</span>
            {:else}
              <span class="row">
                <button class="primary" onclick={() => api.stemsDownloadModel()}>Download Model (about 210 MB)</button>
              </span>
              {#if st?.model.state === "error"}<span class="err small">{st.model.message}</span>{/if}
            {/if}
          </span>
          <span class="l">Prepare</span>
          <span class="col">
            <span class="row">
              <button disabled={st?.model.state !== "ready" || !app.current}
                onclick={async () => {
                  const ids = [...new Set((app.current?.entries ?? []).map((e) => (e.versionId ? app.versionMap.get(e.versionId)?.track.id : undefined)).filter(Boolean) as string[])];
                  const n = await app.run(api.stemsPrepare(ids.filter((id) => !app.stemsReady.has(id))));
                  if (n != null) app.toast(`Preparing stems for ${n} song(s) in “${app.current?.name}”`, "info");
                }}>Prepare Stems for the Current Set</button>
            </span>
            {#if jobs.length}
              <span class="small">{jobs.filter(([, j]) => j.state !== "failed").length} song(s) in progress</span>
              {#each jobs.filter(([, j]) => j.state === "failed") as [id, j]}
                <span class="err small">{app.trackMap.get(id)?.title ?? id}: {j.state === "failed" ? j.message : ""}</span>
              {/each}
            {/if}
            <span class="faint small">Separation uses a lot of CPU; Orpheus runs it at low priority, but preparing ahead of an event is best.</span>
          </span>
          <span class="l">Storage</span>
          <span class="col">
            <span class="num">{((st?.cacheBytes ?? 0) / 1e9).toFixed(2)} GB for {app.stemsReady.size} song(s) (about 85 MB per 3-minute song)</span>
            <code>{st?.cacheDir}</code>
            <span><button class="danger" disabled={!app.stemsReady.size}
              onclick={async () => { if (await confirmAction(`Delete the prepared stems of all ${app.stemsReady.size} song(s)?`)) { await app.run(api.stemsDeleteAll()); app.stemsReady = new Set(await api.stemsReady()); } }}>Delete All Stems…</button></span>
          </span>
        </div>

      {:else if tab === "spotify"}
        <h2>Spotify</h2>
        <p class="dim">Play songs from Spotify (Premium required) alongside your local files, with the same tempo control, fades and preview.
          Spotify needs an internet connection while playing.</p>
        <div class="form">
          <span class="l">Account</span>
          <span class="col">
            {#if app.spotify?.connected}
              <span>Connected as <b>{app.spotify.username}</b></span>
              {#if !app.spotify.webApi && !app.spotify.ownWebApp}
                <span class="warn small">Sign in once more to enable Spotify search and your playlists.</span>
                <span><button class="primary" onclick={() => app.run(api.spotifyLogin())}>Sign In Again…</button></span>
              {/if}
              <span><button onclick={() => api.spotifyLogout()}>Disconnect</button></span>
            {:else}
              <span class="row">
                <button class="primary" disabled={app.spotify?.connecting} onclick={() => app.run(api.spotifyLogin())}>{app.spotify?.connecting ? "Waiting for browser…" : "Connect Spotify…"}</button>
                {#if app.spotify?.hasCachedCredentials}<button onclick={() => app.run(api.spotifyConnectCached())}>Reconnect</button>{/if}
              </span>
              <span class="faint small">Opens your browser to sign in.</span>
            {/if}
            {#if app.spotify?.error}<span class="err small">{app.spotify.error}</span>{/if}
          </span>
          <span class="l">Downloaded audio</span>
          <span class="row">
            <span class="num">{spotifyCache == null ? "…" : `${(spotifyCache / 1e6).toFixed(0)} MB`}</span>
            <button disabled={!spotifyCache} onclick={async () => {
              if (!(await confirmAction("Delete Spotify audio cached on this computer? It is downloaded again when needed.", "Delete"))) return;
              await app.run(api.spotifyClearCache());
              spotifyCache = await api.spotifyCacheBytes();
            }}>Clear Cache…</button>
          </span>
          <span class="l">Startup</span>
          <label><input type="checkbox" checked={s.spotify.autoConnect} onchange={(e) => set("spotify", { ...$state.snapshot(s.spotify), autoConnect: e.currentTarget.checked })} /> Reconnect automatically</label>
          <span class="l">Search app</span>
          <span class="col">
            <input type="text" placeholder="Shared (default)" value={s.spotify.clientId}
              onchange={(e) => set("spotify", { ...$state.snapshot(s.spotify), clientId: e.currentTarget.value.trim() })} />
            <span class="faint small">
              Search and your playlists use a client ID shared by many apps, which Spotify rate-limits. If you get
              “Spotify is limiting requests”, enter the client ID of your own app from
              developer.spotify.com (Dashboard → Create app, with redirect URI <code>http://127.0.0.1:8898/login</code>
              and the Web API). Playback keeps using your normal sign-in.
            </span>
            {#if app.spotify?.ownWebApp}
              {#if app.spotify.webApi}
                <span class="small">Search uses your app.</span>
              {:else}
                <span><button class="primary" onclick={() => app.run(api.spotifyWebLogin())}>Sign In for Search…</button></span>
              {/if}
            {/if}
          </span>
        </div>

      {:else if tab === "projector"}
        <h2>Projector</h2>
        <div class="form">
          <span class="l">Title</span>
          <input type="text" placeholder="(setlist name)" value={s.projector.title} onchange={(e) => setProj({ title: e.currentTarget.value })} />
          <span class="l">Layout</span>
          <div class="segmented">
            {#each [["auto", "Follow set mode"], ["live", "Now playing"], ["preset", "Full setlist"]] as [v, l]}
              <button class:on={s.projector.displayMode === v} onclick={() => setProj({ displayMode: v as Settings["projector"]["displayMode"] })}>{l}</button>
            {/each}
          </div>
          <span class="l">Show</span>
          <span class="col">
            <label><input type="checkbox" checked={s.projector.revealNextInGap} onchange={(e) => setProj({ revealNextInGap: e.currentTarget.checked })} /> Next dance during the pause (so dancers can find a partner)</label>
            <label><input type="checkbox" checked={s.projector.showNext} onchange={(e) => setProj({ showNext: e.currentTarget.checked })} /> “Up next” while a song plays</label>
            <label><input type="checkbox" checked={s.projector.showArtist} onchange={(e) => setProj({ showArtist: e.currentTarget.checked })} /> Artist</label>
            <label><input type="checkbox" checked={s.projector.showBpm} onchange={(e) => setProj({ showBpm: e.currentTarget.checked })} /> Tempo (BPM)</label>
            <label><input type="checkbox" checked={s.projector.showClock} onchange={(e) => setProj({ showClock: e.currentTarget.checked })} /> Clock</label>
            <label><input type="checkbox" checked={s.projector.showArtwork} onchange={(e) => setProj({ showArtwork: e.currentTarget.checked })} /> Cover art</label>
          </span>
          <span class="l">Recently played</span>
          <span><input type="number" min="0" max="12" value={s.projector.historyCount} onchange={(e) => setProj({ historyCount: +e.currentTarget.value })} /> songs</span>
          <span class="l">Coming up</span>
          <span><input type="number" min="0" max="8" value={s.projector.upcomingCount ?? 0} onchange={(e) => setProj({ upcomingCount: +e.currentTarget.value })} /> songs or slots <span class="faint">(live layout; 0 = just “Up next”)</span></span>
          <span class="l">Text size</span>
          <span class="row"><input type="range" min="0.6" max="1.6" step="0.05" value={s.projector.fontScale} onchange={(e) => setProj({ fontScale: +e.currentTarget.value })} /> {Math.round(s.projector.fontScale * 100)}%</span>
          <span class="l">Theme</span>
          <div class="segmented">
            <button class:on={s.projector.theme === "dark"} onclick={() => setProj({ theme: "dark" })}>Dark</button>
            <button class:on={s.projector.theme === "light"} onclick={() => setProj({ theme: "light" })}>Light</button>
          </div>
        </div>

      {:else if tab === "clubs"}
        <ClubsSettings />
      {:else if tab === "auto"}
        <AutoModeSettings />
      {:else if tab === "styles"}
        <StylesSettings />
      {/if}
    </div>
  </div>
</div>

<svelte:window onkeydown={(e) => e.key === "Escape" && (app.settingsOpen = false)} />

<style>
  .overlay { position: fixed; inset: 0; z-index: 80; background: rgba(0,0,0,0.5); display: grid; place-items: center; }
  .dialog { width: min(1100px, 94vw); height: min(760px, 90vh); display: grid; grid-template-columns: 180px 1fr; background: var(--bg-elev); border: 1px solid var(--border-strong); border-radius: 12px; box-shadow: var(--shadow); overflow: hidden; }
  nav { background: var(--bg-elev-2); border-right: 1px solid var(--border); padding: 12px 8px; display: flex; flex-direction: column; gap: 2px; }
  .tab { justify-content: flex-start; background: none; border: none; height: 28px; }
  .tab.sel { background: var(--bg-selected); }
  .content { position: relative; overflow-y: auto; padding: 18px 24px 30px; user-select: text; }
  .close { position: absolute; right: 12px; top: 12px; }
  h2 { margin: 0 0 8px; font: 600 var(--fs-lg) var(--font-display); }
  p { margin: 0 0 14px; max-width: 720px; }
  .form { display: grid; grid-template-columns: 170px 1fr; gap: 12px 14px; align-items: start; }
  .l { color: var(--text-dim); text-align: right; padding-top: 4px; }
  .col { display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  code { font: var(--fs-sm) var(--font-mono); background: var(--bg); padding: 3px 6px; border-radius: 4px; word-break: break-all; }
  .err { color: var(--red); }
  .warn { color: var(--yellow); }
  .small { font-size: var(--fs-sm); }
  .led { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--text-faint); }
  .led.ok { background: var(--green); }
  table { border-collapse: collapse; width: 100%; font-size: var(--fs-sm); }
  th { text-align: left; color: var(--text-faint); font-weight: 600; padding: 4px; border-bottom: 1px solid var(--border); }
  td { padding: 3px 4px; border-bottom: 1px solid rgba(255,255,255,0.04); }
  .link { padding: 0; height: auto; color: var(--blue); display: inline; }
  .danger { color: var(--red); }
</style>

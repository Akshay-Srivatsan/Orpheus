<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import { MOD } from "../lib/tempo";
    import type { MonitorInfo } from "../lib/types";
  import Icon from "./Icon.svelte";

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  let announceOpen = $state(false);
  let projOpen = $state(false);
  let custom = $state("");
  let monitors = $state<MonitorInfo[]>([]);
  let midiPorts = $state<{ name: string; connected: boolean }[]>([]);

  $effect(() => {
    const load = () => api.midiInfo().then((m) => (midiPorts = m.ports)).catch(() => {});
    load();
    const t = setInterval(load, 4000);
    return () => clearInterval(t);
  });

  const startedAt = $derived(app.perform?.startedAt ?? null);
  const setMinutes = $derived(app.profile?.setMinutes ?? 120);
  const elapsed = $derived(startedAt ? now - startedAt : 0);
  const over = $derived(startedAt ? elapsed > setMinutes * 60000 : false);
  const audioOk = $derived(!!app.outputs?.master && !(app.outputs?.errors.length));
  const midiConnected = $derived(midiPorts.filter((p) => p.connected));

  async function openProjector(monitor: number | null, fullscreen: boolean) {
    projOpen = false;
    await app.run(api.projectorOpen(monitor, fullscreen));
  }

  async function toggleProjMenu() {
    projOpen = !projOpen;
    if (projOpen) monitors = (await app.run(api.monitors())) ?? [];
  }

  function announce(msg: string | null) {
    api.projectorMessage(msg);
    announceOpen = false;
  }

  function fmtHM(ms: number) {
    const m = Math.floor(ms / 60000);
    return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}`;
  }

  function meterW(v: number) {
    return `${Math.min(100, Math.sqrt(v) * 100)}%`;
  }
</script>

<header class="topbar">
  <div class="brand">
    <span class="logo">◐</span>
    <span class="name">Orpheus</span>
    <span class="profile dim" title="Club profile (rules, styles, tempos)">{app.profile?.name ?? ""}</span>
  </div>

  <div class="clock num">
    <span class="time">{new Date(now).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}</span>
    {#if startedAt}
      <span class="settime" class:over title="Time since the first song of this set (planned {setMinutes} min)">
        Set {fmtHM(elapsed)} of {fmtHM(setMinutes * 60000)}
      </span>
    {/if}
  </div>

  <div class="right">
    {#if app.scan?.running}
      <span class="dim small">{app.scan.phase} {app.scan.total ? `${app.scan.done}/${app.scan.total}` : ""}</span>
    {/if}

    <div class="pop-wrap">
      <button class:on={!!app.perform?.message} onclick={() => (announceOpen = !announceOpen)} title="Show an announcement on the projector">
        <Icon name="mic" size={14} /> Announce
      </button>
      {#if announceOpen}
        <div class="pop">
          {#if app.perform?.message}
            <div class="cur">Showing: “{app.perform.message}”</div>
            <button class="danger" onclick={() => announce(null)}>Clear announcement</button>
            <div class="sep"></div>
          {/if}
          {#each app.profile?.announcements ?? [] as a}
            <button class="plain item" onclick={() => announce(a)}>{a}</button>
          {/each}
          <div class="row">
            <input type="text" placeholder="Custom message" bind:value={custom} onkeydown={(e) => e.key === "Enter" && custom.trim() && announce(custom)} style="flex:1" />
            <button disabled={!custom.trim()} onclick={() => announce(custom)}>Show</button>
          </div>
        </div>
      {/if}
    </div>

    <div class="pop-wrap">
      <button onclick={toggleProjMenu} title="Projector display for the dancers"><Icon name="projector" size={14} /> Projector</button>
      {#if projOpen}
        <div class="pop">
          <div class="section-title">Open full screen on</div>
          {#each monitors as m}
            <button class="plain item" onclick={() => openProjector(m.index, true)}>
              {m.name} <span class="faint">{m.width}×{m.height}{m.primary ? " · this screen" : ""}</span>
            </button>
          {/each}
          <div class="sep"></div>
          <button class="plain item" onclick={() => openProjector(null, false)}>Open in a window</button>
          <button class="plain item" onclick={() => { projOpen = false; api.projectorClose(); }}>Close projector</button>
          <button class="plain item" onclick={() => { projOpen = false; app.settingsOpen = "projector"; }}>Projector settings…</button>
        </div>
      {/if}
    </div>

    <div class="vol" title="Master volume (room)">
      <Icon name="speaker" size={14} />
      <div class="meter"><div style:width={meterW(app.levels.master)} class:hot={app.levels.master > 0.985}></div></div>
      <input type="range" min="0" max="1" step="0.01" value={Math.sqrt(app.levels.masterVolume)}
        oninput={(e) => api.setLevels({ master: (+e.currentTarget.value) ** 2 })} aria-label="Master volume" />
    </div>
    <div class="vol" title="Headphone volume">
      <Icon name="headphones" size={14} />
      <div class="meter"><div style:width={meterW(app.levels.cue)}></div></div>
      <input type="range" min="0" max="1" step="0.01" value={Math.sqrt(app.levels.cueVolume)}
        oninput={(e) => api.setLevels({ cue: (+e.currentTarget.value) ** 2 })} aria-label="Headphone volume" />
    </div>

    <button class="plain status" onclick={() => (app.settingsOpen = "audio")}
      title={audioOk ? `Audio: ${app.outputs?.master?.device}` : (app.outputs?.errors.join("\n") || "No audio output")}>
      <span class="led" class:ok={audioOk} class:bad={!audioOk}></span> Audio
    </button>
    <button class="plain status" onclick={() => (app.settingsOpen = "midi")}
      title={midiConnected.length ? midiConnected.map((p) => p.name).join("\n") : "No MIDI controller connected"}>
      <span class="led" class:ok={midiConnected.length > 0}></span> MIDI
    </button>
    <button class="plain status" onclick={() => (app.settingsOpen = "spotify")}
      title={app.spotify?.connected ? `Spotify: ${app.spotify.username}` : "Spotify not connected"}>
      <span class="led" class:ok={app.spotify?.connected} class:warn={app.spotify?.connecting}></span> Spotify
    </button>
    <button class="plain icon" title="Settings ({MOD},)" onclick={() => (app.settingsOpen = "general")}><Icon name="gear" /></button>
  </div>
</header>

{#if announceOpen || projOpen}
  <div class="backdrop" role="presentation" onclick={() => { announceOpen = false; projOpen = false; }}></div>
{/if}

<style>
  .topbar { display: flex; align-items: center; gap: 16px; height: 42px; padding: 0 12px; background: var(--bg-elev); border-bottom: 1px solid var(--border); }
  .brand { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .logo { color: var(--accent); font-size: 18px; }
  .name { font: 700 var(--fs-md) var(--font-display); letter-spacing: 0.02em; }
  .profile { font-size: var(--fs-sm); white-space: nowrap; }
  .clock { display: flex; gap: 12px; align-items: baseline; margin: 0 auto; }
  .time { font: 600 var(--fs-md) var(--font-display); }
  .settime { color: var(--text-dim); font-size: var(--fs-sm); }
  .settime.over { color: var(--red); font-weight: 600; }
  .right { display: flex; align-items: center; gap: 8px; }
  .small { font-size: var(--fs-sm); }
  button.on { color: var(--yellow); border-color: color-mix(in srgb, var(--yellow) 50%, transparent); }
  .pop-wrap { position: relative; }
  .pop { position: absolute; right: 0; top: 32px; z-index: 60; width: 320px; padding: 8px; display: flex; flex-direction: column; gap: 4px;
    background: rgba(36, 37, 42, 0.98); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: var(--shadow); }
  .item { justify-content: flex-start; white-space: normal; height: auto; min-height: 26px; padding: 4px 8px; text-align: left; gap: 8px; }
  .cur { font-size: var(--fs-sm); color: var(--yellow); }
  .sep { height: 1px; background: var(--border-strong); margin: 4px 0; }
  .row { display: flex; gap: 6px; margin-top: 4px; }
  .backdrop { position: fixed; inset: 0; z-index: 50; }
  .vol { display: flex; align-items: center; gap: 4px; color: var(--text-dim); }
  .vol input { width: 70px; }
  .meter { width: 34px; height: 6px; background: var(--bg); border-radius: 3px; overflow: hidden; border: 1px solid var(--border); }
  .meter div { height: 100%; background: var(--green); transition: width 60ms linear; }
  .meter div.hot { background: var(--red); }
  .status { gap: 5px; font-size: var(--fs-sm); color: var(--text-dim); padding: 0 6px; }
  .led { width: 7px; height: 7px; border-radius: 50%; background: var(--text-faint); }
  .led.ok { background: var(--green); }
  .led.bad { background: var(--red); }
  .led.warn { background: var(--yellow); }
  .danger { color: var(--red); }
</style>

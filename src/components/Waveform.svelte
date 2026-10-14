<script lang="ts">
  // Colored overview waveform (Serato-style RGB: low = red, mid = green, high = blue).
  import { api } from "../lib/api";

  interface Props {
    trackId: string | null;
    durationMs: number;
    positionMs?: number;
    startMs?: number;
    endMs?: number | null;
    fadeAtMs?: number | null;
    accent?: string;
    height?: number;
    /** Changes when the waveform may have become available (e.g. analysis finished). */
    revision?: unknown;
    /** Called with a position when the user clicks (null = seeking disabled). */
    onseek?: ((ms: number) => void) | null;
    seekHint?: string;
  }
  let {
    trackId, durationMs, positionMs = 0, startMs = 0, endMs = null, fadeAtMs = null, accent = "var(--accent)",
    height = 56, onseek = null, seekHint = "", revision = null,
  }: Props = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let data = $state<number[]>([]);
  let width = $state(600);
  let hover = $state<number | null>(null);

  const cache = new Map<string, number[]>();

  $effect(() => {
    const id = trackId;
    void revision;
    if (!id) {
      data = [];
      return;
    }
    if (cache.has(id)) {
      data = cache.get(id)!;
      return;
    }
    data = [];
    api.waveform(id).then((w) => {
      if (w.length) cache.set(id, w);
      if (trackId === id) data = w;
    });
  });

  $effect(() => {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.floor(width * dpr));
    canvas.height = Math.floor(height * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const bins = data.length / 4;
    const mid = height / 2;
    if (bins === 0) {
      ctx.fillStyle = "rgba(255,255,255,0.06)";
      ctx.fillRect(0, mid - 1, width, 2);
    } else {
      for (let x = 0; x < width; x++) {
        const b = Math.min(bins - 1, Math.floor((x / width) * bins));
        const peak = data[b * 4] / 255;
        const lo = data[b * 4 + 1], mi = data[b * 4 + 2], hi = data[b * 4 + 3];
        const m = Math.max(lo, mi, hi, 1);
        const r = Math.round(60 + 195 * (lo / m)), g = Math.round(60 + 195 * (mi / m) * 0.9), bl = Math.round(70 + 185 * (hi / m));
        const h = Math.max(1, peak * (height / 2 - 2));
        ctx.fillStyle = `rgb(${r},${g},${bl})`;
        ctx.fillRect(x, mid - h, 1, h * 2);
      }
    }
    if (durationMs > 0) {
      const px = (ms: number) => (ms / durationMs) * width;
      // Trimmed regions (before start / after end) are shaded.
      ctx.fillStyle = "rgba(0,0,0,0.55)";
      if (startMs > 0) ctx.fillRect(0, 0, px(startMs), height);
      if (endMs != null && endMs < durationMs) ctx.fillRect(px(endMs), 0, width - px(endMs), height);
      // Played portion is dimmed.
      if (positionMs > startMs) {
        ctx.fillStyle = "rgba(13,14,16,0.5)";
        ctx.fillRect(px(startMs), 0, px(positionMs) - px(startMs), height);
      }
      if (fadeAtMs != null) {
        ctx.strokeStyle = "rgba(255,214,10,0.9)";
        ctx.setLineDash([3, 3]);
        ctx.beginPath();
        ctx.moveTo(px(fadeAtMs) + 0.5, 0);
        ctx.lineTo(px(fadeAtMs) + 0.5, height);
        ctx.stroke();
        ctx.setLineDash([]);
      }
      const p = px(positionMs);
      ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--ph").trim() || "#fff";
      ctx.fillRect(Math.round(p) - 1, 0, 2, height);
      if (hover != null) {
        ctx.fillStyle = "rgba(255,255,255,0.35)";
        ctx.fillRect(Math.round(hover), 0, 1, height);
      }
    }
  });

  function click(e: MouseEvent) {
    if (!onseek || durationMs <= 0) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    onseek(((e.clientX - rect.left) / rect.width) * durationMs);
  }
</script>

<div
  class="wf"
  class:seekable={!!onseek}
  bind:clientWidth={width}
  style:height="{height}px"
  style:--ph={accent}
  title={seekHint}
  role="slider"
  aria-label="Track position"
  aria-valuemin={0}
  aria-valuemax={durationMs}
  aria-valuenow={positionMs}
  tabindex="-1"
  onclick={click}
  onkeydown={() => {}}
  onmousemove={(e) => {
    if (!onseek) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hover = e.clientX - r.left;
  }}
  onmouseleave={() => (hover = null)}
>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
</div>

<style>
  .wf { position: relative; width: 100%; background: #08090a; border-radius: 6px; overflow: hidden; border: 1px solid var(--border); }
  .wf.seekable { cursor: pointer; }
  canvas { display: block; }
</style>

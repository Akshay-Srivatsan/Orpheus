<script lang="ts">
  // A min–max range (either end may be empty), as numbers, lengths (m:ss) or percentages.
  import type { Range } from "../lib/clubs";

  interface Props {
    value: Range | null;
    onchange: (r: Range | null) => void;
    kind?: "num" | "time" | "pct";
    title?: string;
  }
  let { value, onchange, kind = "num", title = "" }: Props = $props();

  function show(v: number | null | undefined): string {
    if (v == null) return "";
    if (kind === "time") return `${Math.floor(v / 60)}:${String(Math.round(v % 60)).padStart(2, "0")}`;
    return String(Math.round(v * 10) / 10);
  }

  function parse(t: string): number | null {
    const s = t.trim().replace(/%$/, "");
    if (!s) return null;
    if (kind === "time" && s.includes(":")) {
      const [m, sec] = s.split(":");
      const n = Number(m) * 60 + Number(sec);
      return isFinite(n) ? n : null;
    }
    const n = Number(s);
    return isFinite(n) ? n : null;
  }

  function set(which: "min" | "max", t: string) {
    const r = { min: value?.min ?? null, max: value?.max ?? null, [which]: parse(t) };
    onchange(r.min == null && r.max == null ? null : r);
  }
</script>

<span class="range" {title}>
  <input type="text" class="end" placeholder={kind === "time" ? "m:ss" : "min"} value={show(value?.min)} onchange={(e) => set("min", e.currentTarget.value)} />
  <span class="dash">–</span>
  <input type="text" class="end" placeholder={kind === "time" ? "m:ss" : "max"} value={show(value?.max)} onchange={(e) => set("max", e.currentTarget.value)} />
  {#if kind === "pct"}<span class="unit">%</span>{/if}
</span>

<style>
  .range { display: inline-flex; align-items: center; gap: 2px; }
  .end { width: 52px; text-align: center; padding: 0 4px; }
  .dash, .unit { color: var(--text-faint); }
</style>

<script lang="ts">
  // m:ss.s time input; empty = null (inherit).
  import { fmtTime } from "../lib/tempo";
  interface Props { value: number | null | undefined; placeholder?: string; onchange: (ms: number | null) => void; title?: string }
  let { value, placeholder = "", onchange, title = "" }: Props = $props();

  function parse(s: string): number | null {
    const t = s.trim();
    if (!t) return null;
    const parts = t.split(":").map(Number);
    if (parts.some((p) => isNaN(p))) return undefined as unknown as null;
    const secs = parts.length === 2 ? parts[0] * 60 + parts[1] : parts[0];
    return Math.round(secs * 1000);
  }
</script>

<input class="time num" type="text" {title} {placeholder}
  value={value == null ? "" : fmtTime(value, { tenths: true })}
  onchange={(e) => {
    const v = parse(e.currentTarget.value);
    if (v === undefined) e.currentTarget.value = value == null ? "" : fmtTime(value, { tenths: true });
    else onchange(v);
  }} />

<style>
  .time { width: 70px; }
</style>

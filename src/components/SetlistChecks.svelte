<script lang="ts">
  import type { CheckResult, RuleResult } from "../lib/rules";
  import type { Id } from "../lib/types";
  import { fmtTime } from "../lib/tempo";

  let { result, onSelectEntries }: { result: CheckResult; onSelectEntries?: (ids: Id[]) => void } = $props();

  let showPassing = $state(true);

  const failing = $derived(result.results.filter((r) => !r.ok));
  const errors = $derived(failing.filter((r) => r.severity === "error").length);
  const warnings = $derived(failing.filter((r) => r.severity === "warning").length);
  const sorted = $derived.by(() => {
    const rank = (r: RuleResult) => (r.ok ? 3 : r.severity === "error" ? 0 : r.severity === "warning" ? 1 : 2);
    return [...result.results].sort((a, b) => rank(a) - rank(b) || a.index - b.index).filter((r) => showPassing || !r.ok);
  });

  function icon(r: RuleResult) {
    if (r.ok) return "✓";
    return r.severity === "error" ? "✕" : r.severity === "warning" ? "!" : "i";
  }

  function tone(r: RuleResult) {
    if (r.ok) return "var(--ok)";
    return r.severity === "error" ? "var(--err)" : r.severity === "warning" ? "var(--warn)" : "var(--teal)";
  }

  /** Bar scale: fill = count relative to the larger of min/max/count. */
  function barScale(c: { count: number; min?: number; max?: number }) {
    return Math.max(c.count, c.min ?? 0, c.max ?? 0, 1);
  }
</script>

<div class="checks">
  <div class="summary">
    {#if errors === 0 && warnings === 0}
      <span class="headline" style="color: var(--ok)">✓ All requirements met</span>
    {:else}
      <span class="headline" style="color: {errors ? 'var(--err)' : 'var(--warn)'}">
        {errors ? `${errors} problem${errors > 1 ? "s" : ""}` : ""}{errors && warnings ? ", " : ""}{warnings
          ? `${warnings} warning${warnings > 1 ? "s" : ""}`
          : ""}
      </span>
    {/if}
    <span class="total num" title="Total playing time (pauses not included)">
      {fmtTime(result.totalMs)}{#if result.unknownDurations}<span class="faint"> +{result.unknownDurations} ?</span>{/if}
    </span>
  </div>

  <div class="toolbar">
    <span class="section-title">Requirements</span>
    <label class="toggle"><input type="checkbox" bind:checked={showPassing} /> Show passing</label>
  </div>

  <ul class="rules">
    {#each sorted as r (r.index + ":" + r.label)}
      {@const clickable = !r.ok && r.entryIds.length > 0 && !!onSelectEntries}
      <li>
        <button
          class="rule"
          class:ok={r.ok}
          class:clickable
          disabled={!clickable}
          title={clickable ? "Select the affected songs" : undefined}
          onclick={() => clickable && onSelectEntries?.(r.entryIds)}
        >
          <span class="icon" style="--tone: {tone(r)}">{icon(r)}</span>
          <span class="text">
            <span class="label">{r.label}</span>
            <span class="detail">{r.detail}</span>
          </span>
        </button>
      </li>
    {/each}
  </ul>

  {#if result.counts.length}
    <div class="toolbar"><span class="section-title">Styles</span></div>
    <table class="counts">
      <tbody>
        {#each result.counts as c (c.key)}
          {@const scale = barScale(c)}
          {@const low = c.min != null && c.count < c.min}
          {@const high = c.max != null && c.count > c.max}
          <tr>
            <td class="name"><span class="dot" style="background: {c.color}"></span>{c.name}</td>
            <td class="num count" class:bad={low || high}>
              {c.count}{#if c.min != null}<span class="faint">/{c.min}</span>{/if}{#if c.max != null}<span class="faint">
                  ≤{c.max}</span
                >{/if}
            </td>
            <td class="barcell">
              <div class="bar">
                <div
                  class="fill"
                  style="width: {(c.count / scale) * 100}%; background: {low || high ? 'var(--err)' : c.color}"
                ></div>
                {#if c.min != null}<div class="mark" style="left: {(c.min / scale) * 100}%" title="Minimum {c.min}"></div>{/if}
                {#if c.max != null}<div class="mark max" style="left: {(c.max / scale) * 100}%" title="Maximum {c.max}"></div>{/if}
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .checks { display: flex; flex-direction: column; gap: 6px; font-size: var(--fs-sm); min-width: 0; }
  .summary { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; padding: 2px 2px 4px; }
  .headline { font-weight: 600; font-size: var(--fs); }
  .total { color: var(--text-dim); }
  .toolbar { display: flex; align-items: center; justify-content: space-between; padding: 4px 2px 0; }
  .toggle { font-size: var(--fs-xs); color: var(--text-faint); }
  .rules { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 1px; }
  .rule {
    display: flex; align-items: flex-start; gap: 8px; width: 100%; height: auto; padding: 4px 6px;
    background: transparent; border: none; border-radius: var(--radius-sm); text-align: left; color: var(--text);
    justify-content: flex-start; white-space: normal;
  }
  .rule:disabled { opacity: 1; cursor: default; }
  .rule.clickable { cursor: pointer; }
  .rule.clickable:hover { background: var(--bg-hover); }
  .rule.ok .label { color: var(--text-dim); }
  .icon {
    flex: none; width: 16px; height: 16px; margin-top: 1px; border-radius: 50%;
    display: inline-flex; align-items: center; justify-content: center;
    font-size: 10px; font-weight: 700; color: var(--tone);
    background: color-mix(in srgb, var(--tone) 18%, transparent);
  }
  .text { display: flex; flex-direction: column; min-width: 0; }
  .label { line-height: 1.3; }
  .detail { color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.3; }
  .counts { width: 100%; border-collapse: collapse; }
  .counts td { padding: 2px 2px; vertical-align: middle; }
  .name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 150px; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; }
  .count { text-align: right; white-space: nowrap; width: 1%; padding-right: 8px !important; }
  .count.bad { color: var(--err); font-weight: 600; }
  .barcell { width: 40%; }
  .bar { position: relative; height: 6px; background: var(--bg-elev-3); border-radius: 3px; }
  .fill { height: 100%; border-radius: 3px; opacity: 0.85; }
  .mark { position: absolute; top: -2px; width: 2px; height: 10px; margin-left: -1px; background: var(--text-dim); border-radius: 1px; }
  .mark.max { background: var(--warn); }
</style>

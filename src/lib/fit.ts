// Shrink text to fit its box instead of truncating it (the projector).
//
// `use:fit` keeps the element's own font size when the text fits; otherwise
// it finds the largest size that does (on one line, or within `lines`
// lines), down to `min` of the normal size. Re-fits when the text or the
// space changes, and once web fonts have loaded.

export interface FitOptions {
  /** Lines the text may take (1 = never wrap). */
  lines?: number;
  /** Smallest size, as a fraction of the normal size (below it, CSS's ellipsis applies). */
  min?: number;
}

/** Smallest size by default: half the normal size stays readable on a projector. */
const MIN_SCALE = 0.5;
/** Steps of the size search (each halves the range: 8 = within 0.4 %). */
const SEARCH_STEPS = 8;

export function fit(node: HTMLElement, opts: FitOptions = {}) {
  let o = opts;
  let frame = 0;

  function fits(lines: number): boolean {
    if (node.scrollWidth > node.clientWidth + 1) return false;
    if (lines === 1) return true;
    const cs = getComputedStyle(node);
    const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2;
    return node.scrollHeight <= lh * lines + 1;
  }

  function apply() {
    const lines = o.lines ?? 1;
    node.style.fontSize = "";
    if (lines === 1) node.style.whiteSpace = "nowrap";
    const full = parseFloat(getComputedStyle(node).fontSize);
    if (!full || !node.clientWidth || fits(lines)) return;
    let lo = full * (o.min ?? MIN_SCALE);
    let hi = full;
    for (let i = 0; i < SEARCH_STEPS; i++) {
      const mid = (lo + hi) / 2;
      node.style.fontSize = `${mid}px`;
      if (fits(lines)) lo = mid;
      else hi = mid;
    }
    node.style.fontSize = `${lo}px`;
  }

  const run = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(apply);
  };
  // The box's width comes from its container (resizing the text itself would loop).
  const ro = new ResizeObserver(run);
  ro.observe(node.parentElement ?? node);
  const mo = new MutationObserver(run);
  mo.observe(node, { childList: true, characterData: true, subtree: true });
  document.fonts?.ready.then(run);
  run();

  return {
    update(next: FitOptions = {}) {
      o = next;
      run();
    },
    destroy() {
      cancelAnimationFrame(frame);
      ro.disconnect();
      mo.disconnect();
    },
  };
}

/** A group of elements that shrink together (`fitAll`). */
export interface FitGroup {
  selector: string;
  /** The group shrinks together down to this fraction; an element that needs more shrinks on its own. */
  together?: number;
  /** Smallest size of any element, as a fraction (below it, CSS's ellipsis applies). */
  min?: number;
}

/** By default a list shrinks evenly to three quarters; one very long entry shouldn't shrink the rest further. */
const TOGETHER_SCALE = 0.75;
/** A little under the exact ratio: glyph widths don't scale perfectly linearly. */
const SCALE_MARGIN = 0.98;

/**
 * `use:fitAll={[{ selector: ".style" }, …]}` on a list: for each group, the
 * elements share one size, the largest at which all of them fit on one line,
 * so a list stays even instead of each row shrinking by its own amount.
 */
export function fitAll(node: HTMLElement, groups: FitGroup[]) {
  let gs = groups;
  let frame = 0;

  /** The scale at which `el` fits on one line (1 = fits as is). */
  const needed = (el: HTMLElement) =>
    el.clientWidth > 0 && el.scrollWidth > el.clientWidth + 1 ? (el.clientWidth / el.scrollWidth) * SCALE_MARGIN : 1;

  function apply() {
    for (const g of gs) {
      const els = [...node.querySelectorAll<HTMLElement>(g.selector)];
      for (const el of els) el.style.fontSize = "";
      const min = g.min ?? MIN_SCALE;
      const scale = Math.max(g.together ?? TOGETHER_SCALE, min, Math.min(1, ...els.map(needed)));
      if (scale < 1) for (const el of els) el.style.fontSize = `${parseFloat(getComputedStyle(el).fontSize) * scale}px`;
      for (const el of els) {
        const more = needed(el);
        if (more < 1) el.style.fontSize = `${parseFloat(el.style.fontSize || getComputedStyle(el).fontSize) * Math.max(more, min / scale)}px`;
      }
    }
  }

  const run = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(apply);
  };
  const ro = new ResizeObserver(run);
  ro.observe(node);
  const mo = new MutationObserver(run);
  mo.observe(node, { childList: true, characterData: true, subtree: true });
  document.fonts?.ready.then(run);
  run();
  return {
    update(next: FitGroup[]) {
      gs = next;
      run();
    },
    destroy() {
      cancelAnimationFrame(frame);
      ro.disconnect();
      mo.disconnect();
    },
  };
}

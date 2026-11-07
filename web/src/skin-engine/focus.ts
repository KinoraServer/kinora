// Focus and TV remotes: one spatial navigation engine for every skin.
//
// Arrow keys (a TV remote's D-pad) move focus to the nearest focusable
// element in that direction, by position on screen, so any layout works
// without configuration. Authors can steer it:
//   data-kn-focus-group     left/right stay inside the group; coming back with
//                           up/down returns to the item focused there last
//   data-kn-focus-default   where focus starts on a page
//   data-kn-focus-scroll    "center" or "nearest": how focused items scroll into view

const FOCUSABLE =
  "a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1']), [role='link'], [role='button']";

const lastInGroup = new WeakMap<Element, Element>();

export type Direction = "left" | "right" | "up" | "down";

function visible(el: HTMLElement): boolean {
  if (el.closest("[inert]") || el.closest("[hidden]")) return false;
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return false;
  const style = getComputedStyle(el);
  return style.visibility !== "hidden" && style.display !== "none";
}

export function focusables(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(visible);
}

interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
  cx: number;
  cy: number;
}

const box = (el: Element): Box => {
  const r = el.getBoundingClientRect();
  return {
    left: r.left,
    right: r.right,
    top: r.top,
    bottom: r.bottom,
    cx: r.left + r.width / 2,
    cy: r.top + r.height / 2,
  };
};

/**
 * Pick the best candidate in a direction: it must lie that way, and the
 * score favours being close along the direction and aligned across it.
 */
export function pickInDirection(
  from: Box,
  candidates: { el: HTMLElement; b: Box }[],
  dir: Direction,
): HTMLElement | null {
  let best: HTMLElement | null = null;
  let bestScore = Infinity;
  for (const { el, b } of candidates) {
    let primary: number;
    let cross: number;
    switch (dir) {
      case "right":
        if (b.cx <= from.cx + 1 || b.left < from.left + 1) continue;
        primary = Math.max(0, b.left - from.right);
        cross = Math.abs(b.cy - from.cy);
        break;
      case "left":
        if (b.cx >= from.cx - 1 || b.right > from.right - 1) continue;
        primary = Math.max(0, from.left - b.right);
        cross = Math.abs(b.cy - from.cy);
        break;
      case "down":
        if (b.cy <= from.cy + 1 || b.top < from.top + 1) continue;
        primary = Math.max(0, b.top - from.bottom);
        cross = Math.abs(b.cx - from.cx);
        break;
      case "up":
        if (b.cy >= from.cy - 1 || b.bottom > from.bottom - 1) continue;
        primary = Math.max(0, from.top - b.bottom);
        cross = Math.abs(b.cx - from.cx);
        break;
    }
    // Overlap across the direction counts as aligned.
    const overlap =
      dir === "left" || dir === "right"
        ? Math.min(from.bottom, b.bottom) - Math.max(from.top, b.top)
        : Math.min(from.right, b.right) - Math.max(from.left, b.left);
    const score = primary + (overlap > 0 ? 0 : cross * 2) + cross * 0.1;
    if (score < bestScore) {
      bestScore = score;
      best = el;
    }
  }
  return best;
}

function focusEl(el: HTMLElement) {
  const group = el.closest("[data-kn-focus-group]");
  if (group) lastInGroup.set(group, el);
  el.focus({ preventScroll: true });
  const mode =
    el.closest<HTMLElement>("[data-kn-focus-scroll]")?.dataset.knFocusScroll === "center" ? "center" : "nearest";
  el.scrollIntoView({ block: "nearest", inline: mode, behavior: "smooth" });
  // Keep the row in view vertically too, with a little room for headers.
  const r = el.getBoundingClientRect();
  if (r.top < 90 || r.bottom > window.innerHeight - 20) {
    window.scrollBy({ top: r.top < 90 ? r.top - 120 : r.bottom - window.innerHeight + 60, behavior: "smooth" });
  }
}

/** Move focus one step. Returns false when there's nowhere to go. */
export function moveFocus(scopeRoot: ParentNode, dir: Direction): boolean {
  const all = focusables(scopeRoot);
  if (!all.length) return false;
  const active = document.activeElement as HTMLElement | null;
  if (!active || active === document.body || !all.includes(active)) {
    const start = (scopeRoot as Element).querySelector?.<HTMLElement>("[data-kn-focus-default]");
    focusEl(start && visible(start) ? start : all[0]);
    return true;
  }
  const from = box(active);
  const group = active.closest("[data-kn-focus-group]");
  let pool = all.filter((el) => el !== active);
  if (group && (dir === "left" || dir === "right")) {
    pool = pool.filter((el) => group.contains(el));
  }
  const target = pickInDirection(
    from,
    pool.map((el) => ({ el, b: box(el) })),
    dir,
  );
  if (!target) return false;
  // Entering another group: go back to where we were in it.
  const targetGroup = target.closest("[data-kn-focus-group]");
  if (targetGroup && targetGroup !== group && (dir === "up" || dir === "down")) {
    const remembered = lastInGroup.get(targetGroup) as HTMLElement | undefined;
    if (remembered && remembered.isConnected && visible(remembered)) {
      focusEl(remembered);
      return true;
    }
  }
  focusEl(target);
  return true;
}

const KEYS: Record<string, Direction> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

/**
 * Listen for arrow keys while a skin is shown. Text fields keep left/right
 * (and selects keep up/down) so typing and picking still work.
 */
export function installSpatialNavigation(onRemote: () => void, isTv: () => boolean): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;
    const dir = KEYS[e.key];
    const active = document.activeElement as HTMLElement | null;
    const root = document.querySelector<HTMLElement>("[data-app-skin]");
    if (!root) return;
    if ((e.key === "Escape" || e.key === "Backspace" || e.key === "GoBack") && isTv()) {
      const typing = active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA");
      if (typing && e.key === "Backspace") return;
      if (document.querySelector(":popover-open")) return;
      if (location.hash !== "#/" && location.hash !== "") {
        e.preventDefault();
        history.back();
      }
      return;
    }
    if (!dir) return;
    if (active && active !== document.body && !root.contains(active) && !active.closest("[data-app-skin]")) return;
    if (active?.tagName === "INPUT" && (dir === "left" || dir === "right")) return;
    if (active?.tagName === "SELECT" && (dir === "up" || dir === "down")) return;
    if (active?.closest("[data-kn-standard]")) return;
    onRemote();
    if (moveFocus(document, dir)) e.preventDefault();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}

/**
 * On a new page: focus its data-kn-focus-default, unless the viewer is typing
 * somewhere. Returns true once there's nothing left to do.
 */
export function focusDefault(root: ParentNode): boolean {
  const active = document.activeElement;
  if (active && active !== document.body && (active.tagName === "INPUT" || active.tagName === "SELECT")) return true;
  const el = root.querySelector<HTMLElement>("[data-kn-focus-default]");
  if (!el || !visible(el)) return false;
  el.focus({ preventScroll: true });
  return true;
}

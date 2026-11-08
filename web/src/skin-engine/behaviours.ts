// Behaviours: the things that would need JavaScript in a skin, done by
// Kinora on plain markup. They attach to a skin root and follow DOM changes.
//
//   data-kn-rail              horizontal scroller (scroll position remembered per row)
//   data-kn-scroll=prev|next  pages the nearest rail; disabled at the ends
//   data-kn-carousel="8s"     rotates its children; sets data-current; pauses on hover/focus
//   data-kn-carousel-dots     filled with one button per slide
//   root data-scrolled        "true" once the page has scrolled a little

import { i18n } from "@/i18n";

const railMemory = new Map<string, number>();

function railKey(rail: HTMLElement): string | null {
  const row = rail.closest<HTMLElement>("[data-kn-each]");
  const id = row?.getAttribute("data-row") ?? row?.querySelector("[data-kn-text]")?.textContent ?? null;
  return id ? `${location.hash.split("?")[0]}|${id}` : null;
}

/** The rail a scroll button controls: inside the nearest container that has one. */
export function railFor(button: Element): HTMLElement | null {
  let el: Element | null = button;
  while (el) {
    if (el.matches("[data-kn-rail]")) return el as HTMLElement;
    const inner = el.querySelector<HTMLElement>("[data-kn-rail]");
    if (inner) return inner;
    el = el.parentElement;
  }
  return null;
}

function updateRailButtons(root: HTMLElement) {
  for (const btn of root.querySelectorAll<HTMLButtonElement>("[data-kn-scroll]")) {
    const rail = railFor(btn);
    if (!rail) continue;
    const atStart = rail.scrollLeft <= 2;
    const atEnd = rail.scrollLeft + rail.clientWidth >= rail.scrollWidth - 2;
    const disabled = btn.dataset.knScroll === "prev" ? atStart : atEnd;
    if (btn.disabled !== disabled) btn.disabled = disabled;
  }
}

function slidesOf(carousel: HTMLElement): HTMLElement[] {
  return Array.from(carousel.children).filter(
    (c): c is HTMLElement =>
      c instanceof HTMLElement && !c.hasAttribute("data-kn-carousel-dots") && !c.hasAttribute("data-kn-scroll"),
  );
}

function parseDuration(v: string | null): number {
  const m = /^(\d+(?:\.\d+)?)(ms|s)$/.exec((v ?? "").trim());
  if (!m) return 8000;
  const n = Number(m[1]) * (m[2] === "s" ? 1000 : 1);
  return Math.max(2000, n);
}

interface CarouselState {
  index: number;
  timer?: number;
  paused: boolean;
  count: number;
}

const carousels = new WeakMap<HTMLElement, CarouselState>();

function showSlide(carousel: HTMLElement, state: CarouselState, index: number) {
  const slides = slidesOf(carousel);
  if (!slides.length) return;
  state.index = ((index % slides.length) + slides.length) % slides.length;
  slides.forEach((s, i) => {
    const current = i === state.index;
    s.setAttribute("data-current", String(current));
    s.toggleAttribute("inert", !current);
    s.setAttribute("aria-hidden", String(!current));
  });
  const dots = carousel.querySelector<HTMLElement>("[data-kn-carousel-dots]");
  if (dots) {
    if (dots.children.length !== slides.length) {
      dots.replaceChildren(
        ...slides.map((_, i) => {
          const b = document.createElement("button");
          b.type = "button";
          b.setAttribute("aria-label", i18n.t("skins.carousel_dot", { index: i + 1, total: slides.length }));
          b.addEventListener("click", () => {
            const st = carousels.get(carousel);
            if (st) showSlide(carousel, st, i);
          });
          return b;
        }),
      );
    }
    Array.from(dots.children).forEach((d, i) => d.setAttribute("data-current", String(i === state.index)));
  }
}

function reducedMotion() {
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function setupCarousel(carousel: HTMLElement, root: HTMLElement) {
  let state = carousels.get(carousel);
  const count = slidesOf(carousel).length;
  if (state && state.count === count) return;
  if (!state) {
    state = { index: 0, paused: false, count };
    carousels.set(carousel, state);
    const st = state;
    const pause = () => (st.paused = true);
    const resume = () => (st.paused = false);
    carousel.addEventListener("pointerenter", pause);
    carousel.addEventListener("pointerleave", resume);
    carousel.addEventListener("focusin", pause);
    carousel.addEventListener("focusout", resume);
    const interval = parseDuration(carousel.getAttribute("data-kn-carousel"));
    st.timer = window.setInterval(() => {
      const off =
        root.getAttribute("data-opt-hero-rotate") === "false" || carousel.getAttribute("data-rotate") === "false";
      if (!carousel.isConnected) {
        window.clearInterval(st.timer);
        return;
      }
      if (st.paused || off || reducedMotion() || document.hidden) return;
      showSlide(carousel, st, st.index + 1);
    }, interval);
  }
  state.count = count;
  showSlide(carousel, state, state.index);
}

/**
 * Wire behaviours into a skin root. Returns a cleanup function. Safe to call
 * once per root; it follows DOM changes on its own.
 */
export function attachBehaviours(root: HTMLElement): () => void {
  const sync = () => {
    for (const c of root.querySelectorAll<HTMLElement>("[data-kn-carousel]")) setupCarousel(c, root);
    for (const rail of root.querySelectorAll<HTMLElement>("[data-kn-rail]")) {
      if (rail.dataset.knRailReady) continue;
      rail.dataset.knRailReady = "1";
      const key = railKey(rail);
      const saved = key ? railMemory.get(key) : undefined;
      if (saved) rail.scrollLeft = saved;
    }
    updateRailButtons(root);
  };
  let frame = 0;
  const schedule = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      sync();
    });
  };
  const observer = new MutationObserver(schedule);
  observer.observe(root, { childList: true, subtree: true });

  const onClick = (e: MouseEvent) => {
    const btn = (e.target as Element).closest<HTMLElement>("[data-kn-scroll]");
    if (!btn || !root.contains(btn)) return;
    const rail = railFor(btn);
    if (!rail) return;
    e.preventDefault();
    const dir = btn.dataset.knScroll === "prev" ? -1 : 1;
    rail.scrollBy({ left: dir * rail.clientWidth * 0.85, behavior: reducedMotion() ? "auto" : "smooth" });
  };
  const onScroll = (e: Event) => {
    const t = e.target;
    if (t instanceof HTMLElement && t.hasAttribute("data-kn-rail")) {
      const key = railKey(t);
      if (key) railMemory.set(key, t.scrollLeft);
      updateRailButtons(root);
    }
  };
  const onWindowScroll = () => {
    const scrolled = String(window.scrollY > 24);
    if (root.getAttribute("data-scrolled") !== scrolled) root.setAttribute("data-scrolled", scrolled);
  };
  root.addEventListener("click", onClick);
  root.addEventListener("scroll", onScroll, true);
  window.addEventListener("scroll", onWindowScroll, { passive: true });
  window.addEventListener("resize", schedule);
  onWindowScroll();
  sync();
  return () => {
    observer.disconnect();
    cancelAnimationFrame(frame);
    root.removeEventListener("click", onClick);
    root.removeEventListener("scroll", onScroll, true);
    window.removeEventListener("scroll", onWindowScroll);
    window.removeEventListener("resize", schedule);
    for (const c of root.querySelectorAll<HTMLElement>("[data-kn-carousel]")) {
      const st = carousels.get(c);
      if (st?.timer) window.clearInterval(st.timer);
      carousels.delete(c);
    }
  };
}

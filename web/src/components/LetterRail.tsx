import { type KeyboardEvent, type PointerEvent, type RefObject, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { firstBelow, firstIndexByLetter, LETTERS, letterOf, OTHER } from "@/lib/letters";
import { cn } from "@/lib/utils";

/** Space between the sticky header and the row a jump lands on, in px. */
const GAP = 12;

/** The bottom of the app's sticky header (0 when there isn't one). */
function stickyTop(): number {
  const header = document.querySelector("[data-sticky-header]");
  return header ? Math.max(0, header.getBoundingClientRect().bottom) : 0;
}

const reducedMotion = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * A thin, frosted A–Z rail on the right edge of a long title-sorted grid.
 * Click (or drag along it) to jump to a letter; the highlighted letter follows
 * the page as it scrolls. Letters with no titles are dimmed. Arrow keys move
 * between letters for keyboards and TV remotes.
 *
 * `grid` is the element whose children are the tiles, in the same order as
 * `titles`. The window is the scroller (as on the library page).
 */
export function LetterRail({ titles, grid }: { titles: readonly string[]; grid: RefObject<HTMLElement | null> }) {
  const { t } = useTranslation();
  const firsts = useMemo(() => firstIndexByLetter(titles), [titles]);
  const [active, setActive] = useState<string | null>(null);
  const [scrollable, setScrollable] = useState(false);
  const [scrolling, setScrolling] = useState(false);
  const [bubble, setBubble] = useState<{ letter: string; y: number } | null>(null);
  // After a jump, the clicked letter stays lit until the viewer scrolls by themselves:
  // its first title can share a row with the letter before it.
  const pinned = useRef<string | null>(null);
  const jumping = useRef(false);
  const railRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const jumpTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(jumpTimer.current), []);

  // Follow the page: the highlighted letter is the one of the first row under the header.
  useEffect(() => {
    let frame = 0;
    let idle: number | undefined;
    const measure = () => {
      frame = 0;
      const el = grid.current;
      const doc = document.documentElement;
      setScrollable(doc.scrollHeight > window.innerHeight + 1);
      if (!el || el.children.length === 0 || pinned.current) return;
      const tiles = el.children;
      const line = stickyTop() + GAP;
      const i = firstBelow(tiles.length, (n) => tiles[n].getBoundingClientRect().bottom, line);
      setActive(letterOf(titles[i] ?? ""));
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };
    const onScroll = () => {
      if (!jumping.current) pinned.current = null;
      schedule();
      setScrolling(true);
      window.clearTimeout(idle);
      idle = window.setTimeout(() => setScrolling(false), 900);
    };
    const onScrollEnd = () => {
      jumping.current = false;
    };
    schedule();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("scrollend", onScrollEnd);
    window.addEventListener("resize", schedule);
    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(schedule) : null;
    if (grid.current) ro?.observe(grid.current);
    return () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(idle);
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("scrollend", onScrollEnd);
      window.removeEventListener("resize", schedule);
      ro?.disconnect();
    };
  }, [grid, titles]);

  /** The letter itself if it has titles, otherwise the nearest one after it (then before it). */
  const resolve = (letter: string): string | null => {
    if (firsts.has(letter)) return letter;
    const at = LETTERS.indexOf(letter);
    for (let i = at + 1; i < LETTERS.length; i++) if (firsts.has(LETTERS[i])) return LETTERS[i];
    for (let i = at - 1; i >= 0; i--) if (firsts.has(LETTERS[i])) return LETTERS[i];
    return null;
  };

  const jump = (letter: string, smooth: boolean) => {
    const target = resolve(letter);
    const index = target ? firsts.get(target) : undefined;
    const tile = index !== undefined ? (grid.current?.children[index] as HTMLElement | undefined) : undefined;
    if (!target || !tile) return;
    const top = tile.getBoundingClientRect().top + window.scrollY - stickyTop() - GAP;
    pinned.current = target;
    jumping.current = true;
    setActive(target);
    window.scrollTo({ top: Math.max(0, top), behavior: smooth && !reducedMotion() ? "smooth" : "instant" });
    // Browsers without scrollend (or a jump that didn't move): stop treating scrolls as ours soon.
    window.clearTimeout(jumpTimer.current);
    jumpTimer.current = window.setTimeout(() => (jumping.current = false), 1000);
  };

  /** The rail letter under a pointer, from its y position. */
  const letterAt = (y: number): string | null => {
    const buttons = railRef.current?.querySelectorAll<HTMLElement>("[data-letter]");
    if (!buttons?.length) return null;
    for (const b of buttons) {
      const r = b.getBoundingClientRect();
      if (y < r.bottom) return b.dataset.letter ?? null;
    }
    return buttons[buttons.length - 1].dataset.letter ?? null;
  };

  const scrub = (e: PointerEvent<HTMLDivElement>) => {
    const letter = letterAt(e.clientY);
    if (!letter) return;
    const target = resolve(letter);
    if (!target) return;
    const rail = railRef.current?.getBoundingClientRect();
    setBubble({ letter: target, y: rail ? e.clientY - rail.top : 0 });
    if (target !== pinned.current) jump(target, false);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    scrub(e);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (dragging.current) scrub(e);
  };
  const endDrag = () => {
    dragging.current = false;
    setBubble(null);
  };

  // Up / down step between letters that have titles, jumping as they go (TV remotes, keyboards).
  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, letter: string) => {
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    for (let i = LETTERS.indexOf(letter) + step; i >= 0 && i < LETTERS.length; i += step) {
      if (!firsts.has(LETTERS[i])) continue;
      jump(LETTERS[i], true);
      railRef.current?.querySelector<HTMLElement>(`[data-letter="${CSS.escape(LETTERS[i])}"]`)?.focus();
      return;
    }
  };

  if (!scrollable || firsts.size < 2) return null;
  const focusable = active && firsts.has(active) ? active : [...firsts.keys()][0];

  return (
    <nav
      aria-label={t("library.jump_to")}
      data-letter-rail
      className="pointer-events-none fixed top-19 right-0.5 bottom-3 z-30 flex items-center sm:right-1.5"
    >
      {bubble && (
        <div
          aria-hidden
          className="absolute right-full mr-3 flex h-14 w-14 -translate-y-1/2 items-center justify-center rounded-2xl border border-foreground/10 bg-card/60 text-3xl font-semibold text-foreground shadow-xl backdrop-blur-xl backdrop-saturate-150"
          style={{ top: `calc(50% - ${(railRef.current?.offsetHeight ?? 0) / 2}px + ${bubble.y}px)` }}
        >
          {bubble.letter}
        </div>
      )}
      <div
        ref={railRef}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onLostPointerCapture={endDrag}
        className={cn(
          // Frosted glass, like a macOS overlay scrollbar: thin and quiet until you reach for it.
          "group pointer-events-auto flex max-h-full w-5 touch-none flex-col items-center overflow-hidden rounded-full py-1.5 select-none",
          "border border-foreground/10 bg-card/30 shadow-[0_4px_24px_rgb(0_0_0/0.18)] backdrop-blur-xl backdrop-saturate-150",
          "opacity-60 transition-[width,opacity,background-color] duration-200 ease-out",
          "hover:w-6 hover:bg-card/55 hover:opacity-100 focus-within:w-6 focus-within:bg-card/55 focus-within:opacity-100",
          (scrolling || bubble) && "opacity-100",
        )}
      >
        {LETTERS.map((letter) => {
          const has = firsts.has(letter);
          const isActive = letter === active;
          return (
            <button
              key={letter}
              type="button"
              data-letter={letter}
              tabIndex={letter === focusable ? 0 : -1}
              aria-disabled={!has || undefined}
              aria-current={isActive || undefined}
              aria-label={letter === OTHER ? t("library.jump_other") : t("library.jump_letter", { letter })}
              onClick={(e) => {
                // Pointer clicks were handled on pointerdown; this is Enter / Space / a remote's OK.
                if (e.detail === 0 && has) jump(letter, true);
              }}
              onKeyDown={(e) => onKeyDown(e, letter)}
              className={cn(
                "flex min-h-0 w-4 flex-[1_1_1.05rem] cursor-pointer items-center justify-center rounded-full text-[0.625rem] leading-none font-semibold tabular-nums transition-colors duration-150",
                "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                isActive
                  ? "bg-primary text-primary-foreground"
                  : has
                    ? "text-foreground/75 group-hover:text-foreground"
                    : "text-muted-foreground/35",
              )}
            >
              {letter}
            </button>
          );
        })}
      </div>
    </nav>
  );
}

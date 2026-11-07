// App skin CSS: Kinora's small base layers and the skin's own stylesheet.
//
// Cascade layers decide who wins, not specificity:
//   Tailwind's layers (the standard UI) < kn.reset < kn.base < kn.layout < skin
// so anything a skin writes beats Kinora's defaults without !important. The
// base rules also use :where(), which has zero specificity.
//
// The skin's CSS is wrapped in
//   @scope ([data-app-skin="id"]) to ([data-kn-standard]) { :scope { …skin… } }
// Selectors only match inside the skin, and stop at [data-kn-standard]: the
// boundary around Kinora's own pages when they show inside a skin's shell.
// Top-level declarations style the skin's root (CSS nesting under :scope).

import { assetUrl } from "./compile";

export const BASE_CSS = `
@layer kn.reset, kn.base, kn.layout, skin;

@layer kn.base {
  :where([data-app-skin]) { display: block; min-height: 100%; }
  :where([data-app-skin] img) { max-width: 100%; }
  :where([data-app-skin] :focus-visible) { outline: 2px solid var(--kn-ring); outline-offset: 2px; }
  :where([data-app-skin] [data-kn-rail]) {
    display: flex; overflow-x: auto; overscroll-behavior-x: contain; scroll-snap-type: x proximity;
    scrollbar-width: none; scroll-behavior: smooth;
  }
  :where([data-app-skin] [data-kn-rail])::-webkit-scrollbar { display: none; }
  :where([data-app-skin] [data-kn-rail] > *, [data-app-skin] [data-kn-rail] > :where([data-kn-case], [data-kn-default]) > *) {
    scroll-snap-align: start; flex-shrink: 0;
  }
  :where([data-app-skin] [data-kn-carousel]) { position: relative; }
  :where([data-app-skin] [data-kn-sentinel]) { display: block; width: 1px; height: 1px; }
  :where([data-app-skin] [data-kn-each][data-kn-virtual]) { content-visibility: auto; contain-intrinsic-size: auto 320px; }
  :where([data-app-skin] [data-kn-standard]) { display: block; }
  :where([data-app-skin] [data-kn-component]) { display: contents; }
  @media (prefers-reduced-motion: reduce) {
    :where([data-app-skin] [data-kn-rail]) { scroll-behavior: auto; }
  }
}

@layer kn.layout {
  :where([data-kn-layout="stack"]) { display: flex; flex-direction: column; gap: var(--kn-gap, 1rem); }
  :where([data-kn-layout="row"]) { display: flex; flex-wrap: wrap; align-items: center; gap: var(--kn-gap, 1rem); }
  :where([data-kn-layout="grid"]) {
    display: grid; gap: var(--kn-gap, 1rem);
    grid-template-columns: repeat(var(--kn-cols, auto-fill), minmax(min(var(--kn-card-w, 160px), 100%), 1fr));
  }
  :where([data-kn-layout="rail"]) { display: flex; gap: var(--kn-gap, 1rem); overflow-x: auto; }
  :where([data-kn-layout="rail"] > *) { flex: 0 0 var(--kn-card-w, 160px); }
  :where([data-kn-layout="hero"]) {
    position: relative; display: grid; align-content: end; min-height: var(--kn-hero-h, 60vh);
    padding: var(--kn-gap, 1rem); background: var(--kn-bg, none) center / cover no-repeat;
  }
  :where([data-kn-layout="split"]) {
    display: grid; gap: var(--kn-gap, 1.5rem);
    grid-template-columns: var(--kn-split, minmax(0, 1fr) minmax(0, 2fr)); align-items: start;
  }
  @media (max-width: 640px) { :where([data-kn-layout="split"]) { grid-template-columns: minmax(0, 1fr); } }
  :where([data-kn-layout="overlay"]) { display: grid; }
  :where([data-kn-layout="overlay"] > *) { grid-area: 1 / 1; }
  :where([data-kn-layout="center"]) { display: grid; place-items: center; }
}
`;

/** Can this browser scope skin CSS? (Older engines get Kinora's own UI.) */
export function supportsScope(): boolean {
  return typeof window !== "undefined" && "CSSScopeRule" in window;
}

/** Remove blocks that start with `at` (e.g. "@keyframes") and return them separately. */
function extractBlocks(css: string, at: RegExp): { rest: string; blocks: string[] } {
  let rest = "";
  const blocks: string[] = [];
  let i = 0;
  for (;;) {
    at.lastIndex = i;
    const m = at.exec(css);
    if (!m) {
      rest += css.slice(i);
      break;
    }
    rest += css.slice(i, m.index);
    const open = css.indexOf("{", m.index);
    if (open === -1) break;
    let depth = 0;
    let j = open;
    for (; j < css.length; j++) {
      if (css[j] === "{") depth++;
      else if (css[j] === "}" && --depth === 0) break;
    }
    blocks.push(css.slice(m.index, j + 1));
    i = j + 1;
  }
  return { rest, blocks };
}

/** `html`, `body` and `:root` in selectors mean the skin's root. */
function rewriteRootSelectors(css: string): string {
  let out = "";
  let preludeStart = 0;
  for (let i = 0; i < css.length; i++) {
    const ch = css[i];
    if (ch === "{") {
      const prelude = css.slice(preludeStart, i);
      out +=
        prelude.startsWith("@") || /^\s*@/.test(prelude)
          ? prelude
          : prelude.replace(/(^|[\s,>~+(])(?:html|body|:root)(?=$|[\s,.:#[>~+)])/g, "$1&");
      out += ch;
      preludeStart = i + 1;
    } else if (ch === "}" || ch === ";") {
      out += css.slice(preludeStart, i + 1);
      preludeStart = i + 1;
    }
  }
  return out + css.slice(preludeStart);
}

const cssString = (s: string) => `"${s.replace(/["\\\n]/g, "")}"`;

/** Make a skin's CSS safe and local. */
export function scopeAppSkinCss(skinId: string, css: string): string {
  const id = skinId.replace(/[^a-z0-9-]/gi, "");
  let cleaned = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/@import[^;]*;?/gi, "")
    .replace(/@namespace[^;]*;?/gi, "")
    .replace(/(expression\s*\(|-moz-binding|behavior\s*:)[^;}]*/gi, "")
    .replace(/<\/?[a-z][^>]*>/gi, "");
  // url(): only the skin's own assets and inline images.
  cleaned = cleaned.replace(/url\(\s*(['"]?)([^)'"]*)\1\s*\)/gi, (_m, _q, raw: string) => {
    const value = raw.trim();
    if (/^asset:/i.test(value)) {
      const name = value.slice(6);
      return /^[a-zA-Z0-9._-]{1,100}$/.test(name) && !name.startsWith(".")
        ? `url(${cssString(assetUrl(id, name))})`
        : "none";
    }
    if (/^data:image\/(png|jpe?g|gif|webp|svg\+xml|avif)[;,]/i.test(value)) return `url(${cssString(value)})`;
    return "none";
  });
  // Unquoted url( leftovers with odd characters.
  cleaned = cleaned.replace(/url\((?!")[^)]*\)/gi, "none");

  // Keyframes and font faces can't be nested: hoist them, with names made unique to the skin.
  const kf = extractBlocks(cleaned, /@(?:-webkit-)?keyframes\b/g);
  const ff = extractBlocks(kf.rest, /@font-face\b/g);
  let rest = ff.rest;
  const names = kf.blocks
    .map((b) => /@(?:-webkit-)?keyframes\s+([a-zA-Z_][\w-]*)/.exec(b)?.[1])
    .filter((n): n is string => !!n);
  const rename = (n: string) => `kn-${id}-${n}`;
  const keyframes = kf.blocks.map((b) =>
    b.replace(/(@(?:-webkit-)?keyframes\s+)([a-zA-Z_][\w-]*)/, (_m, p: string, n: string) => p + rename(n)),
  );
  if (names.length) {
    const re = new RegExp(`\\b(${names.map((n) => n.replace(/[-]/g, "\\-")).join("|")})\\b`, "g");
    rest = rest.replace(
      /(animation(?:-name)?\s*:)([^;}]*)/gi,
      (_m, prop: string, value: string) => prop + value.replace(re, (n) => rename(n)),
    );
  }
  // url()s are already limited to the skin's own files, so font faces are safe to keep.
  const fonts = ff.blocks;

  rest = rewriteRootSelectors(rest);
  const scope = `[data-app-skin="${id}"]`;
  return `@layer skin {\n${keyframes.join("\n")}\n${fonts.join("\n")}\n@scope (${scope}) to ([data-kn-standard]) {\n:scope {\n${rest}\n}\n}\n}\n`;
}

// Skin markup -> safe template nodes.
//
// Skins are written by other people, so everything is checked against an
// allow-list (skin-rules.json): unknown tags are dropped with their content,
// unknown attributes are dropped, event handlers and URLs never survive.
// Navigation only happens through data-kn-link, images through data-kn-img
// (or `asset:` files that ship with the skin). A skin's bundled player goes
// through the player's own sanitiser, like any player skin.

import { ALLOWED_TAGS, RULES } from "./rules";
import type { SkinElement, SkinNode } from "./types";

const URL_ATTRS = new Set([
  "href",
  "src",
  "xlink:href",
  "srcset",
  "action",
  "formaction",
  "poster",
  "data",
  "background",
]);
const ID_REF_ATTRS = new Set([
  "id",
  "for",
  "popovertarget",
  "aria-controls",
  "aria-labelledby",
  "aria-describedby",
  "aria-owns",
]);
const SVG_TAGS = new Set([
  "svg", "g", "path", "circle", "rect", "line", "polyline", "polygon", "ellipse", "defs", "lineargradient",
  "radialgradient", "stop", "title", "desc", "clippath", "mask", "text", "tspan", "symbol", "use",
]); // prettier-ignore

const isDataImage = (v: string) => /^\s*data:image\/(png|jpe?g|gif|webp|svg\+xml|avif)[;,]/i.test(v);
const unsafeStyle = (v: string) => /url\s*\(|expression|@import|behavior|-moz-binding/i.test(v);

function allowedAttr(tag: string, name: string): boolean {
  const a = RULES.attributes;
  return (
    a.global.includes(name) ||
    a.prefixes.some((p) => name.startsWith(p)) ||
    (a.perTag[tag]?.includes(name) ?? false) ||
    (SVG_TAGS.has(tag) && a.svg.includes(name))
  );
}

/** Clean a parsed tree in place. */
function cleanTree(root: Element) {
  for (const child of Array.from(root.children)) {
    const tag = child.localName.toLowerCase();
    const needs = RULES.tagsNeedBinding[tag];
    if (!ALLOWED_TAGS.has(tag) || (needs && !child.hasAttribute(needs))) {
      child.remove();
      continue;
    }
    for (const attr of Array.from(child.attributes)) {
      const name = attr.name.toLowerCase();
      const value = attr.value;
      let keep: boolean;
      if (name.startsWith("on")) keep = false;
      else if (name.startsWith("data-kn-")) keep = true;
      else if (URL_ATTRS.has(name)) {
        keep =
          (name === "src" && tag === "img" && (isDataImage(value) || /^asset:/.test(value))) ||
          ((name === "href" || name === "xlink:href") && tag === "use" && value.startsWith("#"));
      } else if (name === "style") keep = !unsafeStyle(value);
      else if (name === "tabindex") keep = value.trim() === "0" || value.trim() === "-1";
      else keep = allowedAttr(tag, name);
      if (!keep) child.removeAttribute(attr.name);
    }
    if (tag === "input") child.setAttribute("type", "search");
    cleanTree(child);
  }
}

function parse(html: string): HTMLElement {
  return new DOMParser().parseFromString(`<body>${html}</body>`, "text/html").body;
}

/** Ids are prefixed per skin so they can't clash with Kinora's own. */
export const namespaceId = (skinId: string, id: string) => `kn-${skinId}-${id}`;

/** Asset URL for `asset:name` (relative, so it works behind any reverse-proxy path). */
export const assetUrl = (skinId: string, name: string) =>
  `api/app-skins/${encodeURIComponent(skinId)}/assets/${encodeURIComponent(name)}`;

function toNodes(parent: Node, skinId: string): SkinNode[] {
  const out: SkinNode[] = [];
  for (const node of Array.from(parent.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) {
      const v = node.textContent ?? "";
      // Whitespace between elements collapses to one space (it can matter inline).
      out.push({ t: "text", v: v.trim() === "" ? " " : v });
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as Element;
      const tag = el.localName;
      const attrs: Record<string, string> = {};
      const kn: Record<string, string> = {};
      for (const a of Array.from(el.attributes)) {
        const name = a.name.toLowerCase();
        if (name.startsWith("data-kn-")) {
          const full = RULES.renamed.bindings[name] ?? name;
          kn[full.slice("data-kn-".length)] = a.value;
          continue;
        }
        let value = a.value;
        if (ID_REF_ATTRS.has(name)) {
          value = value
            .split(/\s+/)
            .filter(Boolean)
            .map((id) => namespaceId(skinId, id))
            .join(" ");
        } else if ((name === "href" || name === "xlink:href") && value.startsWith("#")) {
          value = `#${namespaceId(skinId, value.slice(1))}`;
        } else if (name === "src" && value.startsWith("asset:")) {
          value = assetUrl(skinId, value.slice("asset:".length));
        }
        attrs[name] = value;
      }
      const element: SkinElement = { t: "el", tag, attrs, kn, children: toNodes(el, skinId) };
      out.push(element);
    }
  }
  // Drop leading/trailing whitespace-only text.
  while (out.length && out[0].t === "text" && out[0].v === " ") out.shift();
  while (out.length && out[out.length - 1].t === "text" && (out[out.length - 1] as { v: string }).v === " ") out.pop();
  return out;
}

/** Sanitise and compile one template. */
export function compileTemplate(html: string, skinId: string): SkinNode[] {
  const body = parse(html);
  cleanTree(body);
  return toNodes(body, skinId);
}

/** Every template of a bundle, by key (`shell`, `pages/home`, `partials/card`, ...). */
export function compileTemplates(templates: Record<string, string>, skinId: string): Map<string, SkinNode[]> {
  const out = new Map<string, SkinNode[]>();
  for (const [key, html] of Object.entries(templates)) out.set(key, compileTemplate(html, skinId));
  return out;
}

/**
 * The template for a page, most specific first:
 * pages/item.series.tv, pages/item.series, pages/item.tv, pages/item.
 */
export function pickTemplate(
  templates: Map<string, SkinNode[]>,
  page: string,
  variant: string | null,
  device: string,
): { key: string; nodes: SkinNode[] } | null {
  const keys = [
    variant && `pages/${page}.${variant}.${device}`,
    variant && `pages/${page}.${variant}`,
    `pages/${page}.${device}`,
    `pages/${page}`,
  ].filter((k): k is string => !!k);
  for (const key of keys) {
    const nodes = templates.get(key);
    if (nodes) return { key, nodes };
  }
  return null;
}

// Template nodes + view model -> React elements.
//
// Bindings are resolved here: repeat (each), conditions (if, empty, switch),
// partials (use), values (text, attr, img, bg, progress), links, actions,
// and the few interactive pieces Kinora renders itself (search inputs,
// selects, components). Text always becomes text nodes, never HTML, and
// paths only read the view model's own properties.

import { createElement, Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router";
import { RULES } from "./rules";
import type { AppVM, CardVM, CompiledSkin, SkinElement, SkinNode } from "./types";

// ---------------------------------------------------------------------------
// Scope and paths
// ---------------------------------------------------------------------------

export interface Loop {
  index: number;
  count: number;
}

export interface Scope {
  /** Values, innermost last. values[0] is the page (or the app, in the shell). */
  values: unknown[];
  loops: Loop[];
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const own = (o: unknown, k: string) => (isObject(o) || Array.isArray(o)) && Object.prototype.hasOwnProperty.call(o, k);

export function resolve(path: string, scope: Scope, env: Pick<RenderEnv, "app" | "page">): unknown {
  const p = path.trim();
  if (p === "" || p === ".") return scope.values[scope.values.length - 1];
  if (p.startsWith("@")) {
    const loop = scope.loops[scope.loops.length - 1];
    if (!loop) return undefined;
    switch (p) {
      case "@index":
        return loop.index;
      case "@number":
        return loop.index + 1;
      case "@first":
        return loop.index === 0;
      case "@last":
        return loop.index === loop.count - 1;
      case "@count":
        return loop.count;
      default:
        return undefined;
    }
  }
  const segs = p.split(".").map((s) => RULES.renamed.fields[s] ?? s);
  let cur: unknown;
  const [first, ...rest] = segs;
  if (first === "app") cur = env.app;
  else if (first === "page") cur = env.page;
  else {
    cur = undefined;
    for (let i = scope.values.length - 1; i >= 0; i--) {
      if (own(scope.values[i], first)) {
        cur = (scope.values[i] as Record<string, unknown>)[first];
        break;
      }
    }
  }
  for (const seg of rest) {
    if (!own(cur, seg)) return undefined;
    cur = (cur as Record<string, unknown>)[seg];
  }
  return cur;
}

export function truthy(v: unknown): boolean {
  if (Array.isArray(v)) return v.length > 0;
  return !!v;
}

export function toText(v: unknown): string {
  if (v == null || typeof v === "boolean") return "";
  if (Array.isArray(v)) return v.map(toText).filter(Boolean).join(", ");
  if (typeof v === "object") return "";
  return String(v);
}

/** `path`, `!path`, `path=value`, `path!=value`. */
export function evalCondition(cond: string, scope: Scope, env: Pick<RenderEnv, "app" | "page">): boolean {
  let c = cond.trim();
  let negate = false;
  if (c.startsWith("!") && !c.includes("=")) {
    negate = true;
    c = c.slice(1);
  }
  let result: boolean;
  const ne = c.indexOf("!=");
  const eq = c.indexOf("=");
  if (ne > 0) result = toText(resolve(c.slice(0, ne), scope, env)) !== c.slice(ne + 2).trim();
  else if (eq > 0) result = toText(resolve(c.slice(0, eq), scope, env)) === c.slice(eq + 1).trim();
  else result = truthy(resolve(c, scope, env));
  return negate ? !result : result;
}

/** The innermost value of one of these contract types (what a link or action is about). */
export function nearest(
  scope: Scope,
  env: Pick<RenderEnv, "page" | "app">,
  types: string[],
): Record<string, unknown> | null {
  for (let i = scope.values.length - 1; i >= 0; i--) {
    const v = scope.values[i];
    if (isObject(v) && typeof v.$type === "string" && types.includes(v.$type)) return v;
  }
  for (const v of [env.page, env.app]) {
    if (isObject(v) && typeof v.$type === "string" && types.includes(v.$type)) return v;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

export type LinkTarget = { href: string; onClick?: () => void } | null;

export interface SkinActions {
  /** Where a data-kn-link goes. */
  link(kind: string, scope: Scope): LinkTarget;
  /** Run a data-kn-action. */
  run(action: string, scope: Scope): void;
  /** A data-kn-select changed. */
  select(kind: string, value: string): void;
}

export interface RenderEnv {
  skin: CompiledSkin;
  app: AppVM;
  page: unknown;
  /** What data-kn-slot="outlet" shows (the shell only). */
  outlet?: ReactNode;
  actions: SkinActions;
  /** Current route (path + query) for aria-current. */
  current: string;
  /** Kinora's own widgets for data-kn-component. */
  components?: Record<string, () => ReactNode>;
}

// ---------------------------------------------------------------------------
// Attributes -> React props
// ---------------------------------------------------------------------------

const PROP_NAMES: Record<string, string> = {
  class: "className",
  for: "htmlFor",
  tabindex: "tabIndex",
  popovertarget: "popoverTarget",
  popovertargetaction: "popoverTargetAction",
  spellcheck: "spellCheck",
  autocomplete: "autoComplete",
  enterkeyhint: "enterKeyHint",
  datetime: "dateTime",
  viewbox: "viewBox",
  preserveaspectratio: "preserveAspectRatio",
  gradientunits: "gradientUnits",
  gradienttransform: "gradientTransform",
  pathlength: "pathLength",
  "xlink:href": "xlinkHref",
};
const BOOLEAN_ATTRS = new Set(["hidden", "open", "disabled", "selected"]);
const camel = (s: string) => s.replace(/-([a-z])/g, (_m, c: string) => c.toUpperCase());

export function parseStyle(style: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const decl of style.split(";")) {
    const i = decl.indexOf(":");
    if (i <= 0) continue;
    const prop = decl.slice(0, i).trim();
    const value = decl.slice(i + 1).trim();
    if (!prop || !value) continue;
    out[prop.startsWith("--") ? prop : camel(prop.toLowerCase())] = value;
  }
  return out;
}

function toProps(el: SkinElement): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  const svg =
    el.tag !== el.tag.toLowerCase() ||
    [
      "svg",
      "g",
      "path",
      "circle",
      "rect",
      "line",
      "polyline",
      "polygon",
      "ellipse",
      "stop",
      "use",
      "text",
      "tspan",
      "mask",
      "symbol",
      "defs",
    ].includes(el.tag);
  for (const [name, value] of Object.entries(el.attrs)) {
    if (name === "style") props.style = parseStyle(value);
    else if (BOOLEAN_ATTRS.has(name)) props[name] = true;
    else if (name === "popover") props.popover = value === "manual" ? "manual" : "auto";
    else if (name.startsWith("aria-") || name.startsWith("data-")) props[name] = value;
    else if (PROP_NAMES[name]) props[PROP_NAMES[name]] = name === "tabindex" ? Number(value) : value;
    else if (svg && name.includes("-")) props[camel(name)] = value;
    else props[name] = value;
  }
  // Binding attributes stay in the DOM so CSS (and Kinora's behaviours) can target them.
  for (const [name, value] of Object.entries(el.kn)) props[`data-kn-${name}`] = value;
  return props;
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

const ACTION_LABELS: Record<string, string> = {
  play: "Play",
  resume: "Resume",
  restart: "Play from the start",
  "toggle-played": "Mark watched or unwatched",
  logout: "Sign out",
  "next-season": "Next season",
  "prev-season": "Previous season",
};

function imageUrl(
  kinds: string,
  size: string | undefined,
  scope: Scope,
  env: RenderEnv,
): { url: string; kind: string } | null {
  const w = size && RULES.imageWidths.includes(Number(size)) ? `?w=${Number(size)}` : "";
  for (const kind of kinds.split(/\s+/).filter(Boolean)) {
    if (kind === "avatar") {
      if (env.app.user.$avatarUrl) return { url: env.app.user.$avatarUrl, kind };
      continue;
    }
    const card = nearest(scope, env, ["Card", "Item"]) as unknown as CardVM | null;
    if (!card) continue;
    const available =
      kind === "poster"
        ? card.hasPoster
        : kind === "backdrop"
          ? card.hasBackdrop
          : kind === "thumb"
            ? card.hasThumb || card.hasBackdrop
            : kind === "logo"
              ? card.hasLogo
              : false;
    if (available) return { url: `api/items/${encodeURIComponent(card.id)}/images/${kind}${w}`, kind };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

const VOID = new Set(RULES.voidTags);

function keyOf(item: unknown, index: number): string {
  return isObject(item) && typeof item.id === "string" && item.id ? item.id : `i${index}`;
}

function push(scope: Scope, value: unknown, loop?: Loop): Scope {
  return { values: [...scope.values, value], loops: loop ? [...scope.loops, loop] : scope.loops };
}

export function renderNodes(nodes: SkinNode[], scope: Scope, env: RenderEnv, depth = 0, fill?: number): ReactNode[] {
  const out: ReactNode[] = [];
  nodes.forEach((node, i) => {
    if (node.t === "text") {
      out.push(node.v);
      return;
    }
    if (node.kn.each !== undefined) {
      const list = resolve(node.kn.each, scope, env);
      if (!Array.isArray(list) || list.length === 0) return;
      const make = (item: unknown, index: number) =>
        renderElement(
          node,
          push(scope, item, { index, count: list.length }),
          env,
          keyOf(item, index),
          depth,
          fill,
          true,
        );
      if (node.kn.virtual !== undefined && list.length > CHUNK) {
        out.push(<Chunked key={`v${i}`} items={list} render={make} />);
      } else {
        out.push(<Fragment key={`e${i}`}>{list.map(make)}</Fragment>);
      }
      return;
    }
    out.push(renderElement(node, scope, env, `n${i}`, depth, fill, false));
  });
  return out;
}

function renderElement(
  el: SkinElement,
  scope: Scope,
  env: RenderEnv,
  key: string,
  depth: number,
  fill: number | undefined,
  eachRoot: boolean,
): ReactNode {
  const kn = el.kn;
  if (kn.if !== undefined && !evalCondition(kn.if, scope, env)) return null;
  if (kn.empty !== undefined && truthy(resolve(kn.empty, scope, env))) return null;

  const props = toProps(el);
  props.key = key;
  const style = (props.style as Record<string, string>) ?? {};
  let children: ReactNode = undefined;

  // State attributes on repeated elements, so CSS can style variants.
  if (eachRoot) {
    const item = scope.values[scope.values.length - 1];
    const loop = scope.loops[scope.loops.length - 1];
    if (isObject(item)) {
      if (typeof item.kind === "string") props["data-kind"] = item.kind;
      if (item.played === true) props["data-played"] = "true";
      if (item.inProgress === true) props["data-in-progress"] = "true";
      if (item.active === true) props["data-active"] = "true";
    }
    if (loop?.index === 0) props["data-first"] = "true";
    if (loop && loop.index === loop.count - 1) props["data-last"] = "true";
  }

  if (kn.attr) {
    for (const pair of kn.attr.split(/\s+/)) {
      const [name, path] = pair.split(":");
      if (!name || !path) continue;
      const ok =
        name === "title" ||
        name === "alt" ||
        name.startsWith("aria-") ||
        (name.startsWith("data-") && !name.startsWith("data-kn-"));
      if (ok) props[name] = toText(resolve(path, scope, env));
    }
  }

  if (kn.img !== undefined && el.tag === "img") {
    const img = imageUrl(kn.img, kn.size, scope, env);
    if (img) {
      props.src = img.url;
      props.loading = "lazy";
      props.decoding = "async";
    } else {
      props["data-missing"] = "";
    }
    if (props.alt === undefined) {
      const card = nearest(scope, env, ["Card", "Item"]);
      props.alt = img?.kind === "logo" && card ? toText(card.title) : "";
    }
  }
  if (kn.bg !== undefined) {
    const img = imageUrl(kn.bg, kn.size, scope, env);
    if (img) style["--kn-bg"] = `url("${img.url}")`;
    else props["data-missing"] = "";
  }
  let progress = fill;
  if (kn.progress !== undefined) {
    const n = Number(resolve(kn.progress, scope, env));
    progress = Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0;
    style["--kn-progress"] = String(progress);
    props.role = props.role ?? "progressbar";
    props["aria-valuemin"] = 0;
    props["aria-valuemax"] = 100;
    props["aria-valuenow"] = Math.round(progress * 100);
    if (props["aria-label"] === undefined) props["aria-label"] = "Progress";
  }
  if (kn.fill !== undefined && fill !== undefined) style.width = `${fill * 100}%`;

  if (kn.link !== undefined) {
    const target = env.actions.link(kn.link, scope);
    if (target) {
      if (el.tag === "a") props.href = target.href;
      else {
        props.role = "link";
        props.tabIndex = props.tabIndex ?? 0;
      }
      const go = target.onClick;
      if (go || el.tag !== "a") {
        props.onClick = (e: { preventDefault(): void }) => {
          e.preventDefault();
          if (go) go();
          else window.location.hash = target.href.slice(1);
        };
        if (el.tag !== "a")
          props.onKeyDown = (e: { key: string; preventDefault(): void }) => {
            if (e.key === "Enter") {
              e.preventDefault();
              if (go) go();
              else window.location.hash = target.href.slice(1);
            }
          };
      }
      const here = env.current;
      const path = target.href.replace(/^#/, "");
      const season = kn.link === "season" ? nearest(scope, env, ["SeasonTab"]) : null;
      const active = season
        ? season.active === true
        : path === here || (kn.link === "library" && here.startsWith(path + "?"));
      if (active && kn.link !== "back" && kn.link !== "play" && kn.link !== "resume") {
        props["aria-current"] = "page";
        props["data-active"] = "true";
      }
    }
  }

  if (kn.action !== undefined) {
    const action = kn.action;
    props.onClick = (e: { preventDefault(): void }) => {
      e.preventDefault();
      env.actions.run(action, scope);
    };
    if (el.tag !== "button") {
      props.role = "button";
      props.tabIndex = props.tabIndex ?? 0;
      props.onKeyDown = (e: { key: string; preventDefault(): void }) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          env.actions.run(action, scope);
        }
      };
    } else if (props.type === undefined) props.type = "button";
    const hasLabel =
      props["aria-label"] !== undefined ||
      kn.text !== undefined ||
      el.children.some((c) => c.t === "text" && c.v.trim());
    if (!hasLabel && ACTION_LABELS[action]) props["aria-label"] = ACTION_LABELS[action];
  }
  if (el.tag === "button" && props.type === undefined) props.type = "button";

  if (Object.keys(style).length) props.style = style;

  // Elements Kinora renders itself.
  if (kn.search !== undefined && el.tag === "input") {
    return <SearchInput key={key} props={props} />;
  }
  if (kn.select !== undefined && el.tag === "select") {
    return <SelectBinding key={key} kind={kn.select} props={props} env={env} />;
  }
  if (VOID.has(el.tag.toLowerCase())) return createElement(el.tag, props);

  if (kn.text !== undefined) {
    const text = toText(resolve(kn.text, scope, env));
    if (text === "") props["data-empty"] = "";
    children = text;
  } else if (kn.component !== undefined) {
    const make = env.components?.[kn.component];
    children = make ? (
      <span data-kn-standard style={{ display: "contents" }}>
        {make()}
      </span>
    ) : null;
  } else if (kn.slot === "outlet") {
    children = env.outlet ?? null;
  } else if (kn.slot !== undefined) {
    children = null;
  } else if (kn.use !== undefined) {
    const partial = env.skin.templates.get(`partials/${kn.use}`);
    children =
      partial && depth < RULES.limits.partialDepth ? renderNodes(partial, scope, env, depth + 1, progress) : null;
  } else if (kn.switch !== undefined) {
    const value = toText(resolve(kn.switch, scope, env));
    const elements = el.children.filter((c): c is SkinElement => c.t === "el");
    const chosen =
      elements.find((c) => c.kn.case !== undefined && c.kn.case === value) ??
      elements.find((c) => c.kn.default !== undefined);
    children = chosen ? renderNodes([chosen], scope, env, depth, progress) : null;
  } else {
    children = renderNodes(el.children, scope, env, depth, progress);
  }
  return createElement(el.tag, props, ...(Array.isArray(children) ? children : [children]));
}

// ---------------------------------------------------------------------------
// Interactive pieces
// ---------------------------------------------------------------------------

/** Cards rendered per step for data-kn-virtual lists. */
const CHUNK = 60;

function Chunked({ items, render }: { items: unknown[]; render: (item: unknown, index: number) => ReactNode }) {
  const [count, setCount] = useState(CHUNK);
  const sentinel = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const el = sentinel.current;
    if (!el || count >= items.length || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => entries.some((e) => e.isIntersecting) && setCount((c) => Math.min(items.length, c + CHUNK)),
      { rootMargin: "1200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [count, items.length]);
  return (
    <>
      {items.slice(0, count).map(render)}
      {count < items.length && <span ref={sentinel} data-kn-sentinel aria-hidden />}
    </>
  );
}

/** data-kn-search: live search. Typing opens the search page; Enter does it at once. */
function SearchInput({ props }: { props: Record<string, unknown> }) {
  const navigate = useNavigate();
  const location = useLocation();
  const q = new URLSearchParams(location.search).get("q") ?? "";
  const onSearchPage = location.pathname === "/search";
  const [value, setValue] = useState(q);
  const focused = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!focused.current) setValue(q);
  }, [q]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const go = (v: string, now: boolean) => {
    window.clearTimeout(timer.current);
    const run = () => {
      const t = v.trim();
      if (t) navigate(`/search?q=${encodeURIComponent(t)}`, { replace: onSearchPage });
      else if (onSearchPage) navigate("/search", { replace: true });
    };
    if (now) run();
    else timer.current = window.setTimeout(run, 300);
  };
  const rest = { ...props };
  delete rest.key;
  return (
    <input
      {...rest}
      type="search"
      value={value}
      onFocus={() => (focused.current = true)}
      onBlur={() => (focused.current = false)}
      onChange={(e) => {
        setValue(e.target.value);
        go(e.target.value, false);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") go(value, true);
        if (e.key === "Escape" && value) {
          e.stopPropagation();
          setValue("");
        }
      }}
    />
  );
}

/** data-kn-select: sort (library) or season (series). */
function SelectBinding({ kind, props, env }: { kind: string; props: Record<string, unknown>; env: RenderEnv }) {
  const page = env.page as Record<string, unknown>;
  const options: { value: string; label: string; active: boolean }[] =
    kind === "sort"
      ? ((page?.sort as { value: string; label: string; active: boolean }[]) ?? [])
      : kind === "season"
        ? ((page?.seasons as { id: string; label: string; active: boolean }[]) ?? []).map((s) => ({
            value: s.id,
            label: s.label,
            active: s.active,
          }))
        : [];
  if (options.length < 2 && kind === "season") return null;
  const rest = { ...props };
  delete rest.key;
  return (
    <select
      {...rest}
      value={options.find((o) => o.active)?.value ?? options[0]?.value ?? ""}
      onChange={(e) => env.actions.select(kind, e.target.value)}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

/** Render one template with a page (or the app, for the shell) at the top of the scope. */
export function SkinTemplate({ nodes, env, root }: { nodes: SkinNode[]; env: RenderEnv; root: unknown }) {
  return <>{renderNodes(nodes, { values: [root], loops: [] }, env)}</>;
}

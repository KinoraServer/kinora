import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { ItemCard } from "@/lib/types";
import { compileTemplate, compileTemplates, pickTemplate } from "./compile";
import { scopeAppSkinCss } from "./css";
import { pickInDirection } from "./focus";
import { SkinTemplate, type RenderEnv } from "./render";
import type { CompiledSkin, SkinBundle } from "./types";
import { buildApp, buildHome, shortCode, toCard } from "./viewmodels";
import { readZip } from "./zip";

const NOW = Date.UTC(2026, 10, 7);

const card = (over: Partial<ItemCard> = {}): ItemCard => ({
  id: "m1",
  library_id: "lib",
  kind: "movie",
  parent_id: null,
  series_id: null,
  title: "Copper Sky",
  year: 2018,
  season_number: null,
  episode_number: null,
  series_title: null,
  has_poster: true,
  has_backdrop: false,
  has_thumb: false,
  duration_ms: 6_720_000,
  position_ms: null,
  played: null,
  rating: 7.4,
  added_at: NOW - 86_400_000,
  ...over,
});

function bundle(templates: Record<string, string>, over: Partial<SkinBundle> = {}): SkinBundle {
  return {
    id: "t",
    name: "Test",
    version: "1.0.0",
    skinApi: 1,
    devices: [],
    options: {},
    optionValues: {},
    templates,
    css: "",
    assets: [],
    hash: "h",
    ...over,
  };
}

function render(html: string, page: unknown, extra: Partial<RenderEnv> = {}) {
  const b = bundle({ "pages/x": html });
  const skin: CompiledSkin = { bundle: b, templates: compileTemplates(b.templates, b.id), css: "" };
  const app = buildApp({
    info: null,
    user: { id: "u", username: "lan", role: "owner", avatar_version: null },
    isAdmin: true,
    libraries: [],
    path: "/",
    query: "",
    options: {},
    device: "desktop",
    input: "pointer",
  });
  const env: RenderEnv = {
    skin,
    app,
    page,
    current: "/",
    actions: {
      link: (kind) => (kind === "item" ? { href: "#/item/m1" } : null),
      run: vi.fn(),
      select: vi.fn(),
    },
    ...extra,
  };
  return renderToStaticMarkup(<SkinTemplate nodes={skin.templates.get("pages/x")!} env={env} root={page} />);
}

describe("compile", () => {
  it("drops scripts, handlers, unknown tags and outside URLs", () => {
    const nodes = compileTemplate(
      `<div onclick="x()" class="a"><script>alert(1)</script><iframe src="https://evil"></iframe>
       <a href="https://evil" data-kn-link="item">x</a><img src="https://evil/x.png"><img src="asset:logo.png"></div>`,
      "t",
    );
    const json = JSON.stringify(nodes);
    expect(json).not.toContain("onclick");
    expect(json).not.toContain("script");
    expect(json).not.toContain("iframe");
    expect(json).not.toContain("evil");
    expect(json).toContain("api/app-skins/t/assets/logo.png");
    expect(json).toContain('"link":"item"');
  });

  it("namespaces ids and the attributes that point at them", () => {
    const json = JSON.stringify(
      compileTemplate(`<button popovertarget="menu"></button><div id="menu" popover></div>`, "t"),
    );
    expect(json).toContain('"popovertarget":"kn-t-menu"');
    expect(json).toContain('"id":"kn-t-menu"');
  });

  it("drops inputs and selects that aren't bound to anything", () => {
    const json = JSON.stringify(compileTemplate(`<input name="x"><select></select><input data-kn-search>`, "t"));
    expect(json.match(/"tag":"input"/g)?.length).toBe(1);
    expect(json).not.toContain('"tag":"select"');
  });

  it("picks the most specific template", () => {
    const t = compileTemplates(
      { "pages/item": "a", "pages/item.series": "b", "pages/item.tv": "c", "pages/item.series.tv": "d" },
      "t",
    );
    expect(pickTemplate(t, "item", "series", "tv")?.key).toBe("pages/item.series.tv");
    expect(pickTemplate(t, "item", "series", "phone")?.key).toBe("pages/item.series");
    expect(pickTemplate(t, "item", "movie", "tv")?.key).toBe("pages/item.tv");
    expect(pickTemplate(t, "item", "movie", "phone")?.key).toBe("pages/item");
    expect(pickTemplate(t, "home", null, "phone")).toBeNull();
  });
});

describe("css", () => {
  it("wraps everything in a layer and a scope that stops at Kinora's own pages", () => {
    const out = scopeAppSkinCss("t", "color: red; .a { color: blue }");
    expect(out).toContain("@layer skin");
    expect(out).toContain('@scope ([data-app-skin="t"]) to ([data-kn-standard])');
    expect(out).toContain(":scope {");
  });

  it("only keeps the skin's own files and inline images", () => {
    const out = scopeAppSkinCss(
      "t",
      `.a { background: url(https://evil/x.png) } .b { background: url("asset:bg.jpg") } .c { background: url(data:image/png;base64,AAA) } @import "https://evil/x.css";`,
    );
    expect(out).not.toContain("evil");
    expect(out).toContain('url("api/app-skins/t/assets/bg.jpg")');
    expect(out).toContain("data:image/png");
  });

  it("hoists and renames keyframes, and points animations at them", () => {
    const out = scopeAppSkinCss("t", "@keyframes fade { to { opacity: 1 } } .a { animation: fade 1s }");
    expect(out).toContain("@keyframes kn-t-fade");
    expect(out).toContain("animation: kn-t-fade 1s");
    expect(out.indexOf("@keyframes")).toBeLessThan(out.indexOf("@scope"));
  });

  it("turns html, body and :root selectors into the skin root", () => {
    const out = scopeAppSkinCss("t", "body { margin: 0 } :root .x { color: red }");
    expect(out).toContain("& { margin: 0 }");
    expect(out).toContain("& .x");
  });

  it("cannot be escaped through the id", () => {
    expect(scopeAppSkinCss('x"] body{', "")).toContain('[data-app-skin="xbody"]');
  });
});

describe("view models", () => {
  it("builds cards authors can use directly", () => {
    const c = toCard(card({ position_ms: 600_000 }), NOW);
    expect(c.title).toBe("Copper Sky");
    expect(c.inProgress).toBe(true);
    expect(c.isNew).toBe(false);
    expect(toCard(card(), NOW).isNew).toBe(true);
    expect(c.runtimeLabel).toBe("1h 52m");
    expect(c.$type).toBe("Card");
    expect(shortCode(1, 2)).toBe("S1 · E2");
  });

  it("marks continue-watching rows", () => {
    const home = buildHome([{ id: "continue", title: "Continue watching", items: [card()] }], [], NOW);
    expect(home.rows[0].kind).toBe("continue");
    expect(home.isEmpty).toBe(false);
    expect(buildHome([], null, NOW).isEmpty).toBe(true);
  });
});

describe("render", () => {
  it("binds text, loops, conditions and state attributes", () => {
    const html = render(
      `<ul><li data-kn-each="items" class="c"><span data-kn-text="title"></span><b data-kn-if="isNew">New</b><i data-kn-if="!isNew">Old</i></li></ul>`,
      {
        $type: "Library",
        items: [toCard(card(), NOW), toCard(card({ id: "m2", title: "Low Tide", played: true, added_at: 0 }), NOW)],
      },
    );
    expect(html).toContain("Copper Sky");
    expect(html).toContain("Low Tide");
    expect(html).toMatch(/<b [^>]*>New<\/b>/);
    expect(html).toMatch(/<i [^>]*>Old<\/i>/);
    expect(html.match(/<b /g)?.length).toBe(1);
    expect(html).toContain('data-played="true"');
    expect(html).toContain('data-first="true"');
  });

  it("builds image URLs from the card, at an allowed size", () => {
    const html = render(`<img data-kn-img="backdrop poster" data-kn-size="480">`, toCard(card(), NOW));
    expect(html).toContain('src="api/items/m1/images/poster?w=480"');
  });

  it("links through Kinora, never through the template", () => {
    const html = render(`<a data-kn-link="item">Open</a>`, toCard(card(), NOW));
    expect(html).toContain('href="#/item/m1"');
  });

  it("renders the matching case of a switch, else the default", () => {
    const tpl = `<div data-kn-switch="kind"><p data-kn-case="series">S</p><p data-kn-default>D</p></div>`;
    expect(render(tpl, { $type: "Card", kind: "series" })).toContain("<p");
    expect(render(tpl, { $type: "Card", kind: "series" })).toContain(">S<");
    expect(render(tpl, { $type: "Card", kind: "movie" })).toContain(">D<");
  });

  it("does not read through prototypes", () => {
    const html = render(`<span data-kn-text="constructor"></span><span data-kn-text="title.length"></span>`, {
      $type: "Card",
      title: "abc",
    });
    expect(html).not.toContain("function");
  });
});

describe("focus", () => {
  const b = (left: number, top: number, w = 100, h = 50) => ({
    left,
    top,
    right: left + w,
    bottom: top + h,
    cx: left + w / 2,
    cy: top + h / 2,
  });
  const el = (name: string) => ({ name }) as unknown as HTMLElement;

  it("moves to the nearest element in the direction pressed", () => {
    const from = b(0, 0);
    const right = el("right");
    const farRight = el("far");
    const below = el("below");
    const cands = [
      { el: right, b: b(120, 0) },
      { el: farRight, b: b(400, 0) },
      { el: below, b: b(0, 80) },
    ];
    expect(pickInDirection(from, cands, "right")).toBe(right);
    expect(pickInDirection(from, cands, "down")).toBe(below);
    expect(pickInDirection(from, cands, "left")).toBeNull();
  });

  it("prefers items in the same row over closer ones out of line", () => {
    const from = b(0, 0);
    const aligned = el("aligned");
    const diagonal = el("diagonal");
    expect(
      pickInDirection(
        from,
        [
          { el: diagonal, b: b(110, 200) },
          { el: aligned, b: b(300, 10) },
        ],
        "right",
      ),
    ).toBe(aligned);
  });
});

describe("zip", () => {
  it("reads stored entries and skips macOS metadata", async () => {
    // A minimal zip with one stored file "a/skin.json" containing "{}" and a __MACOSX entry.
    const enc = new TextEncoder();
    const files = [
      { name: "a/skin.json", data: enc.encode("{}") },
      { name: "__MACOSX/._x", data: enc.encode("x") },
    ];
    const parts: Uint8Array[] = [];
    const central: Uint8Array[] = [];
    let offset = 0;
    for (const f of files) {
      const name = enc.encode(f.name);
      const local = new Uint8Array(30 + name.length);
      const lv = new DataView(local.buffer);
      lv.setUint32(0, 0x04034b50, true);
      lv.setUint32(18, f.data.length, true);
      lv.setUint32(22, f.data.length, true);
      lv.setUint16(26, name.length, true);
      local.set(name, 30);
      const c = new Uint8Array(46 + name.length);
      const cv = new DataView(c.buffer);
      cv.setUint32(0, 0x02014b50, true);
      cv.setUint32(20, f.data.length, true);
      cv.setUint32(24, f.data.length, true);
      cv.setUint16(28, name.length, true);
      cv.setUint32(42, offset, true);
      c.set(name, 46);
      parts.push(local, f.data);
      central.push(c);
      offset += local.length + f.data.length;
    }
    const cdSize = central.reduce((n, c) => n + c.length, 0);
    const end = new Uint8Array(22);
    const ev = new DataView(end.buffer);
    ev.setUint32(0, 0x06054b50, true);
    ev.setUint16(8, files.length, true);
    ev.setUint16(10, files.length, true);
    ev.setUint32(12, cdSize, true);
    ev.setUint32(16, offset, true);
    const entries = await readZip(new Blob([...parts, ...central, end].map((p) => p as BlobPart)));
    expect(entries.map((e) => e.path)).toEqual(["a/skin.json"]);
    expect(new TextDecoder().decode(entries[0].data)).toBe("{}");
  });

  it("refuses files that aren't zips", async () => {
    await expect(readZip(new Blob(["hello"]))).rejects.toThrow(/zip/);
  });
});

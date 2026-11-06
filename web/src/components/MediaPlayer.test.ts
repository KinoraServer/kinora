import { describe, expect, it } from "vitest";
import { resolvePlayerStyle, sanitizeSkinHtml, scopeSkinCss } from "./MediaPlayer";

describe("sanitizeSkinHtml", () => {
  it("keeps wiring attributes and plain markup", () => {
    const out = sanitizeSkinHtml(
      '<div class="a"><button data-kn-action="play">Go</button><span data-kn-text="time"></span></div>',
    );
    expect(out).toContain('data-kn-action="play"');
    expect(out).toContain('data-kn-text="time"');
    expect(out).toContain('class="a"');
  });

  it("removes scripts, handlers and frames", () => {
    const out = sanitizeSkinHtml(
      '<div onclick="x()"><script>alert(1)</script><iframe src="https://evil"></iframe><button onmouseover="x()">b</button></div>',
    );
    expect(out).not.toMatch(/script|iframe|onclick|onmouseover/i);
    expect(out).toContain("<button");
  });

  it("removes external urls but keeps data images and anchors", () => {
    const out = sanitizeSkinHtml(
      '<img src="https://evil/x.png"><img src="data:image/png;base64,AAAA"><a href="javascript:alert(1)">x</a><svg><use href="https://evil/a.svg#x"></use></svg>',
    );
    expect(out).not.toContain("evil");
    expect(out).not.toContain("javascript");
    expect(out).toContain("data:image/png");
  });

  it("removes style attributes that load things", () => {
    const out = sanitizeSkinHtml('<div style="background:url(https://evil/x)"></div><div style="color:red"></div>');
    expect(out).not.toContain("evil");
    expect(out).toContain("color:red");
  });
});

describe("scopeSkinCss", () => {
  it("nests everything under the skin id", () => {
    const out = scopeSkinCss("neon", "color: red; .a { top: 0 }");
    expect(out).toContain('[data-skin="neon"]{');
    expect(out).toContain(".a { top: 0 }");
  });

  it("drops imports, external urls and expressions", () => {
    const out = scopeSkinCss(
      "neon",
      "@import url(https://evil/a.css); .a { background: url(https://evil/x.png); width: expression(alert(1)); } .b { background: url(data:image/png;base64,AAAA) }",
    );
    expect(out).not.toContain("evil");
    expect(out).not.toContain("@import");
    expect(out).not.toContain("expression");
    expect(out).toContain("data:image/png");
  });

  it("hoists keyframes out of the nested block", () => {
    const out = scopeSkinCss("neon", ".a { animation: p 1s } @keyframes p { 0% { opacity: 0 } 100% { opacity: 1 } }");
    const scopeStart = out.indexOf('[data-skin="neon"]');
    expect(out.indexOf("@keyframes p")).toBeLessThan(scopeStart);
    expect(out.slice(scopeStart)).not.toContain("@keyframes");
  });

  it("cannot be escaped through the id", () => {
    expect(scopeSkinCss('x"] body{display:none} [y', "")).toContain('[data-skin="xbodydisplaynoney"]');
  });
});

describe("resolvePlayerStyle", () => {
  it("uses the theme layout unless the viewer chose one", () => {
    expect(resolvePlayerStyle(null, "floating", [])).toBe("floating");
    expect(resolvePlayerStyle("theme", "floating", [])).toBe("floating");
    expect(resolvePlayerStyle("pro", "floating", [])).toBe("pro");
  });

  it("defaults to minimal", () => {
    expect(resolvePlayerStyle(null, null, [])).toBe("minimal");
    expect(resolvePlayerStyle("theme", undefined, [])).toBe("minimal");
  });

  it("accepts installed skins and falls back when one disappears", () => {
    expect(resolvePlayerStyle("neon", null, ["neon"])).toBe("neon");
    expect(resolvePlayerStyle("neon", null, [])).toBe("minimal");
    expect(resolvePlayerStyle(null, "neon", [])).toBe("minimal");
  });
});

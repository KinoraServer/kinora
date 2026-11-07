import { describe, expect, it } from "vitest";
import { applyTheme, clearTheme, groupThemes, pairOf, resolveTheme } from "./theme";
import type { Theme } from "./types";

describe("theme tokens", () => {
  it("applies tokens as --kn-* custom properties and clears them", () => {
    const el = document.createElement("div");
    applyTheme({ id: "t", name: "T", scheme: "light", tokens: { primary: "red", radius: "1rem" } }, el);
    expect(el.style.getPropertyValue("--kn-primary")).toBe("red");
    expect(el.style.getPropertyValue("--kn-radius")).toBe("1rem");
    expect(el.dataset.theme).toBe("t");
    clearTheme(el);
    expect(el.style.getPropertyValue("--kn-primary")).toBe("");
  });
});

const t = (id: string, scheme: "dark" | "light", pair?: string): Theme => ({
  id,
  name: id[0].toUpperCase() + id.slice(1),
  scheme,
  pair,
  tokens: {},
});
const themes = [
  t("kinora", "dark", "kinora-light"),
  t("kinora-light", "light"),
  t("ember", "dark"),
  t("paper", "light"),
];

describe("theme pairs", () => {
  it("groups pairs, dark-only and light-only themes", () => {
    const g = groupThemes(themes);
    expect(g.paired.map((f) => [f.id, f.name, f.dark?.id, f.light?.id])).toEqual([
      ["kinora", "Kinora", "kinora", "kinora-light"],
    ]);
    expect(g.dark.map((f) => f.id)).toEqual(["ember"]);
    expect(g.light.map((f) => f.id)).toEqual(["paper"]);
  });

  it("finds a pair declared on either side, and ignores broken pairs", () => {
    expect(pairOf(themes[1], themes)?.id).toBe("kinora");
    expect(pairOf(t("x", "dark", "missing"), themes)).toBeUndefined();
    expect(pairOf(t("x", "dark", "ember"), themes)).toBeUndefined(); // same scheme
  });

  it("follows the mode for pairs and the theme's own scheme otherwise", () => {
    expect(resolveTheme(themes, "kinora", "light", "dark")?.id).toBe("kinora-light");
    expect(resolveTheme(themes, "kinora-light", "dark", "dark")?.id).toBe("kinora");
    expect(resolveTheme(themes, "kinora", "system", "light")?.id).toBe("kinora-light");
    expect(resolveTheme(themes, "kinora-light", null, "dark")?.id).toBe("kinora-light");
    expect(resolveTheme(themes, "ember", "light", "light")?.id).toBe("ember");
    expect(resolveTheme(themes, "gone", "dark", "dark")).toBeUndefined();
  });
});

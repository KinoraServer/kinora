import { describe, expect, it } from "vitest";
import { applyTheme, clearTheme } from "./theme";

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

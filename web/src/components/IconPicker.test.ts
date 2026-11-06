import { describe, expect, it } from "vitest";
import { ICON_NAMES, ICONS, filterIcons, iconLabel, libraryIcon } from "./IconPicker";

describe("filterIcons", () => {
  it("returns everything for an empty search", () => {
    expect(filterIcons("")).toEqual(ICON_NAMES);
    expect(filterIcons("   ")).toEqual(ICON_NAMES);
  });

  it("matches part of a name, ignoring case", () => {
    expect(filterIcons("FIL")).toContain("film");
    expect(filterIcons("tv")).toContain("tv");
  });

  it("matches multi-word names with spaces or dashes", () => {
    expect(filterIcons("chef hat")).toEqual(["chef-hat"]);
    expect(filterIcons("chef-hat")).toEqual(["chef-hat"]);
  });

  it("returns nothing when nothing matches", () => {
    expect(filterIcons("zzzz")).toEqual([]);
  });
});

describe("icons", () => {
  it("makes readable labels", () => {
    expect(iconLabel("graduation-cap")).toBe("Graduation cap");
    expect(iconLabel("disc-3")).toBe("Disc");
    expect(iconLabel("film")).toBe("Film");
  });

  it("keeps names lowercase and dashed so they are safe to save", () => {
    for (const name of ICON_NAMES) expect(name).toMatch(/^[a-z0-9-]+$/);
    expect(Object.values(ICONS).every(Boolean)).toBe(true);
  });
});

describe("order and defaults", () => {
  it("lists movie and TV icons first", () => {
    expect(ICON_NAMES.slice(0, 6)).toEqual(["film", "clapperboard", "popcorn", "tv", "monitor-play", "video"]);
    expect(ICON_NAMES.indexOf("film")).toBeLessThan(ICON_NAMES.indexOf("music"));
    expect(ICON_NAMES.indexOf("popcorn")).toBeLessThan(ICON_NAMES.indexOf("folder"));
  });

  it("picks a library icon, falling back by type", () => {
    expect(libraryIcon({ icon: "ghost", kind: "movies" })).toBe("ghost");
    expect(libraryIcon({ icon: null, kind: "tv" })).toBe("tv");
    expect(libraryIcon({ kind: "movies" })).toBe("film");
    expect(libraryIcon({ icon: "removed-icon", kind: "movies" })).toBe("film");
  });
});

import { describe, expect, it } from "vitest";
import { firstBelow, firstIndexByLetter, letterOf } from "./letters";

describe("letterOf", () => {
  it("files titles under their first letter", () => {
    expect(letterOf("Orbit")).toBe("O");
    expect(letterOf("northern lights")).toBe("N");
  });

  it("ignores a leading article, like the server's sort", () => {
    expect(letterOf("The Matrix")).toBe("M");
    expect(letterOf("A Quiet Place")).toBe("Q");
    expect(letterOf("An Education")).toBe("E");
    expect(letterOf("Theodore")).toBe("T");
    expect(letterOf("Alien")).toBe("A");
  });

  it("puts numbers and symbols under #", () => {
    expect(letterOf("2001: A Space Odyssey")).toBe("#");
    expect(letterOf("[REC]")).toBe("#");
    expect(letterOf("")).toBe("#");
  });

  it("drops accents", () => {
    expect(letterOf("Élan")).toBe("E");
    expect(letterOf("Ødegaard")).toBe("#");
  });
});

describe("firstIndexByLetter", () => {
  it("keeps the first title of each letter", () => {
    const map = firstIndexByLetter(["2012", "Alien", "The Abyss", "Brick", "Blade"]);
    expect([...map]).toEqual([
      ["#", 0],
      ["A", 1],
      ["B", 3],
    ]);
  });
});

describe("firstBelow", () => {
  // Three rows of four tiles, each row 100px tall.
  const bottom = (i: number) => (Math.floor(i / 4) + 1) * 100;

  it("finds the first tile that reaches past the line", () => {
    expect(firstBelow(12, bottom, 0)).toBe(0);
    expect(firstBelow(12, bottom, 100)).toBe(4);
    expect(firstBelow(12, bottom, 150)).toBe(4);
    expect(firstBelow(12, bottom, 250)).toBe(8);
  });

  it("returns the last tile once everything is above the line", () => {
    expect(firstBelow(12, bottom, 1000)).toBe(11);
  });
});

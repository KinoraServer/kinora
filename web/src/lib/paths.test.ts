import { describe, expect, it } from "vitest";
import { parentFolder } from "./paths";

describe("parentFolder", () => {
  it("returns the folder above", () => {
    expect(parentFolder("/media/movies")).toBe("/media");
    expect(parentFolder("/media/movies/")).toBe("/media");
    expect(parentFolder("/media")).toBe("/");
  });

  it("handles Windows paths", () => {
    expect(parentFolder("D:\\Media\\Movies")).toBe("D:\\Media");
    expect(parentFolder("D:\\Movies")).toBe("D:\\");
  });

  it("returns nothing without a parent", () => {
    expect(parentFolder("")).toBe("");
    expect(parentFolder("movies")).toBe("");
  });
});

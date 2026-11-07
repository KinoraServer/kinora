import { describe, expect, it } from "vitest";
import { centerSquare } from "./avatar-image";

describe("centerSquare", () => {
  it("crops the middle of landscape and portrait images", () => {
    expect(centerSquare(400, 200)).toEqual({ sx: 100, sy: 0, side: 200 });
    expect(centerSquare(300, 500)).toEqual({ sx: 0, sy: 100, side: 300 });
    expect(centerSquare(256, 256)).toEqual({ sx: 0, sy: 0, side: 256 });
  });
});

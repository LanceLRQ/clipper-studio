import { describe, it, expect } from "vitest";
import { getClipColor, CLIP_COLORS, MAX_CLIPS } from "../clip-colors";

describe("getClipColor", () => {
  it("returns first color for index 0", () => {
    expect(getClipColor(0)).toBe("#3b82f6");
  });

  it("returns last color for max index", () => {
    expect(getClipColor(CLIP_COLORS.length - 1)).toBe("#f97316");
  });

  it("wraps around when index exceeds palette length", () => {
    expect(getClipColor(CLIP_COLORS.length)).toBe("#3b82f6");
    expect(getClipColor(CLIP_COLORS.length * 2 + 1)).toBe(
      getClipColor(1)
    );
  });

  it("returns undefined for negative index (JS modulo behavior)", () => {
    const result = getClipColor(-1);
    // JS: -1 % 7 = -1, CLIP_COLORS[-1] = undefined
    expect(result).toBeUndefined();
  });

  it("returns a valid color string for large index", () => {
    const result = getClipColor(999);
    expect(result).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe("CLIP_COLORS", () => {
  it("has 7 colors", () => {
    expect(CLIP_COLORS.length).toBe(7);
  });

  it("all colors are valid hex", () => {
    for (const c of CLIP_COLORS) {
      expect(c).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});

describe("MAX_CLIPS", () => {
  it("is 100", () => {
    expect(MAX_CLIPS).toBe(100);
  });
});

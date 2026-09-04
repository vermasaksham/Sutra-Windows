import { describe, expect, it } from "vitest";
import { isVertical, nearestDock, readDock } from "./toolbarDock";

describe("nearestDock", () => {
  const W = 1000;
  const H = 800;

  it("picks the edge the pointer was dropped nearest", () => {
    expect(nearestDock(500, 20, W, H)).toBe("top");
    expect(nearestDock(500, 780, W, H)).toBe("bottom");
    expect(nearestDock(20, 400, W, H)).toBe("left");
    expect(nearestDock(980, 400, W, H)).toBe("right");
  });

  it("measures in fractions of the window, not pixels", () => {
    // A wide, short window: 100px from the left is 10% across, while 100px
    // from the top is 50% down. By raw pixels these would tie and the top
    // would win every time, making the side edges nearly unhittable.
    expect(nearestDock(100, 100, 1000, 200)).toBe("left");
    // And the reverse shape.
    expect(nearestDock(100, 100, 200, 1000)).toBe("top");
  });

  it("answers a zero-sized window without dividing by it", () => {
    expect(nearestDock(0, 0, 0, 0)).toBe("top");
    expect(nearestDock(10, 10, 100, 0)).toBe("top");
  });

  it("puts a drop in the dead centre somewhere reachable", () => {
    // All four distances are 0.5. Any answer is defensible; none may be
    // undefined, because the toolbar has to land.
    expect(["top", "bottom", "left", "right"]).toContain(
      nearestDock(500, 400, W, H),
    );
  });
});

describe("readDock", () => {
  it("keeps an edge it knows", () => {
    expect(readDock("left")).toBe("left");
    expect(readDock("bottom")).toBe("bottom");
  });

  it("falls back for anything else", () => {
    // A value from a newer build, or from someone editing storage. An unknown
    // edge would position the toolbar with no matching CSS and put it off
    // screen, where it could not be dragged back.
    expect(readDock(null)).toBe("top");
    expect(readDock("floating")).toBe("top");
    expect(readDock("")).toBe("top");
  });
});

describe("isVertical", () => {
  it("knows which edges stack", () => {
    expect(isVertical("left")).toBe(true);
    expect(isVertical("right")).toBe(true);
    expect(isVertical("top")).toBe(false);
    expect(isVertical("bottom")).toBe(false);
  });
});

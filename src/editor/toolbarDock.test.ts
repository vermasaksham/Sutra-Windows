import { describe, expect, it } from "vitest";
import {
  DOCKS,
  SIDE_RESERVE,
  isVertical,
  nearestDock,
  placement,
  readDock,
} from "./toolbarDock";

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

describe("placement", () => {
  // A note column that starts after two sidebars and stops before the context
  // panel, in a 1600×900 window.
  const column = {
    left: 500,
    right: 1300,
    top: 0,
    bottom: 900,
    width: 800,
    height: 900,
  };
  const viewport = { width: 1600, height: 900 };

  it("leaves the top dock in the flow", () => {
    // Sticky, not fixed: it has to *start* between the tags and the first line
    // and only pin once the note scrolls under it. A fixed toolbar is pinned
    // from the first frame and never sits in the document at all.
    expect(placement("top", column, viewport)).toEqual({ position: "sticky" });
  });

  it("pins the side docks to the middle of the column, not the window", () => {
    const left = placement("left", column, viewport);
    expect(left).toMatchObject({
      position: "fixed",
      left: 512,
      top: 450,
      transform: "translateY(-50%)",
    });

    // `right` is measured from the window's edge, so it is the gap between the
    // column's right edge and the window's, plus the inset.
    expect(placement("right", column, viewport)).toMatchObject({
      position: "fixed",
      right: 312,
      top: 450,
    });
  });

  it("keeps a side dock clear of the sidebar", () => {
    // The bug this replaces: a toolbar fixed to the window's own left edge sat
    // on top of the folder tree. It must never be left of the column.
    const left = placement("left", column, viewport);
    expect(left.position).toBe("fixed");
    if (left.position === "fixed") {
      expect(left.left).toBeGreaterThan(column.left);
    }
  });

  it("centres the bottom dock on the column", () => {
    // Not on the window: the note is not centred in the window once a context
    // panel is open on the right.
    expect(placement("bottom", column, viewport)).toMatchObject({
      position: "fixed",
      left: 900,
      bottom: 12,
      transform: "translateX(-50%)",
    });
  });

  it("follows a column that does not fill the window height", () => {
    // A shorter column — a window with something above the note area — must
    // still put the side docks in the middle of the *column*.
    const short = { ...column, top: 100, bottom: 700, height: 600 };
    expect(placement("left", short, viewport)).toMatchObject({ top: 400 });
    expect(placement("bottom", short, viewport)).toMatchObject({ bottom: 212 });
  });

  it("fits inside the strip the scroll container reserves", () => {
    // Two numbers that have to agree and live apart: the inset here, and the
    // padding App puts on the scroll container. If the toolbar can be placed
    // wider than the reserved strip it goes back to covering the first
    // character of every line — the bug the reserve exists to fix.
    //
    // A vertical bar is one 28px button, 4px of padding either side, and a
    // 1px border: 38px.
    const BAR = 38;

    const left = placement("left", column, viewport);
    const right = placement("right", column, viewport);
    if (left.position !== "fixed" || right.position !== "fixed") {
      throw new Error("a side dock must be fixed");
    }
    expect(left.left! - column.left + BAR).toBeLessThanOrEqual(SIDE_RESERVE);
    expect(
      viewport.width - right.right! - column.right + BAR,
    ).toBeLessThanOrEqual(SIDE_RESERVE);
  });

  it("falls back to the flow before anything has been measured", () => {
    // On the first render there is no box yet. Sticky renders in the document
    // and is always visible; a fixed toolbar positioned from a null box would
    // land in the window's top-left corner.
    for (const dock of DOCKS) {
      expect(placement(dock, null, viewport)).toEqual({ position: "sticky" });
    }
  });
});

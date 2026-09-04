import { describe, expect, it } from "vitest";
import {
  closePane,
  collectPaneIds,
  isValidLayout,
  neighborPane,
  pane,
  paneRects,
  resizeSplit,
  splitPane,
} from "./layoutTree";

describe("layout tree", () => {
  it("builds an arbitrary asymmetric split", () => {
    let root = splitPane(pane("a"), "a", "horizontal", "b");
    root = splitPane(root, "a", "vertical", "c");
    expect(collectPaneIds(root)).toEqual(["a", "c", "b"]);
    expect(isValidLayout(root)).toBe(true);
  });
  it("normalizes resized percentages", () => {
    const root = splitPane(pane("a"), "a", "horizontal", "b");
    const resized = resizeSplit(root, [], [1, 3]);
    expect(resized.type).toBe("split");
    if (resized.type === "split") expect(resized.sizes).toEqual([25, 75]);
    expect(isValidLayout(resized)).toBe(true);
  });
  it("collapses a split after closing a pane", () => {
    const root = splitPane(pane("a"), "a", "vertical", "b");
    expect(closePane(root, "b")).toEqual(pane("a"));
  });
  it("does not mutate the prior tree", () => {
    const root = splitPane(pane("a"), "a", "vertical", "b");
    const json = JSON.stringify(root);
    resizeSplit(root, [], [20, 80]);
    expect(JSON.stringify(root)).toBe(json);
  });
});

describe("spatial pane navigation", () => {
  // [ [a / c] | b ]: a top-left, c bottom-left, b full-height right.
  let root = splitPane(pane("a"), "a", "horizontal", "b");
  root = splitPane(root, "a", "vertical", "c");

  it("resolves pane geometry that tiles the workspace", () => {
    const rects = Object.fromEntries(paneRects(root).map((entry) => [entry.paneId, entry.rect]));
    expect(rects.a).toEqual({ x: 0, y: 0, w: 50, h: 50 });
    expect(rects.c).toEqual({ x: 0, y: 50, w: 50, h: 50 });
    expect(rects.b).toEqual({ x: 50, y: 0, w: 50, h: 100 });
  });

  it("moves focus to the spatially adjacent pane", () => {
    expect(neighborPane(root, "a", "right")).toBe("b");
    expect(neighborPane(root, "a", "down")).toBe("c");
    expect(neighborPane(root, "c", "up")).toBe("a");
    expect(neighborPane(root, "c", "right")).toBe("b");
    expect(neighborPane(root, "b", "left")).toBe("a");
  });

  it("returns undefined at an edge (no pane in that direction)", () => {
    expect(neighborPane(root, "a", "left")).toBeUndefined();
    expect(neighborPane(root, "a", "up")).toBeUndefined();
    expect(neighborPane(root, "b", "up")).toBeUndefined();
    expect(neighborPane(root, "b", "right")).toBeUndefined();
  });

  it("picks the perpendicular-overlapping neighbor, not a diagonal one", () => {
    // From b (right, spanning full height) moving left, a and c both touch the seam; the sort is
    // deterministic and never jumps to a pane that does not overlap b's vertical span.
    expect(neighborPane(pane("solo"), "solo", "left")).toBeUndefined();
  });
});

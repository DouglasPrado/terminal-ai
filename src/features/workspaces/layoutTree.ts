import type { LayoutNode } from "../../lib/ipc";

export type Direction = "horizontal" | "vertical";
export function pane(paneId: string): LayoutNode {
  return { type: "pane", paneId };
}
export function splitPane(
  root: LayoutNode,
  targetId: string,
  direction: Direction,
  newPaneId: string,
): LayoutNode {
  return mapNode(root, targetId, (node) => ({
    type: "split",
    direction,
    sizes: [50, 50],
    children: [node, pane(newPaneId)],
  }));
}
export function resizeSplit(root: LayoutNode, path: number[], sizes: number[]): LayoutNode {
  if (path.length === 0) {
    if (root.type !== "split" || sizes.length !== root.children.length)
      throw new Error("Invalid split resize");
    return { ...root, sizes: normalizeSizes(sizes) };
  }
  if (root.type !== "split") throw new Error("Split path points to a pane");
  const [index, ...rest] = path;
  if (!root.children[index]) throw new Error("Split path is out of bounds");
  return {
    ...root,
    children: root.children.map((child, childIndex) =>
      childIndex === index ? resizeSplit(child, rest, sizes) : child,
    ),
  };
}
export function closePane(root: LayoutNode, targetId: string): LayoutNode | null {
  if (root.type === "pane") return root.paneId === targetId ? null : root;
  const kept = root.children
    .map((child) => closePane(child, targetId))
    .filter((child): child is LayoutNode => child !== null);
  if (kept.length === 0) return null;
  if (kept.length === 1) return kept[0];
  const weights = root.children
    .map((child, index) => (closePane(child, targetId) === null ? null : root.sizes[index]))
    .filter((size): size is number => size !== null);
  return { ...root, children: kept, sizes: normalizeSizes(weights) };
}
export function collectPaneIds(root: LayoutNode): string[] {
  return root.type === "pane" ? [root.paneId] : root.children.flatMap(collectPaneIds);
}

export type FocusDirection = "left" | "right" | "up" | "down";
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

// Resolve each pane's geometry as a percentage rectangle within the workspace, walking the split
// tree with its persisted `sizes`. A "horizontal" split divides the x-axis, a "vertical" split the
// y-axis (matching splitPane / react-resizable-panels). Pure — used for spatial focus navigation.
export function paneRects(
  root: LayoutNode,
  rect: Rect = { x: 0, y: 0, w: 100, h: 100 },
): Array<{ paneId: string; rect: Rect }> {
  if (root.type === "pane") return [{ paneId: root.paneId, rect }];
  const horizontal = root.direction === "horizontal";
  let offset = 0;
  return root.children.flatMap((child, index) => {
    const fraction = root.sizes[index] / 100;
    const childRect: Rect = horizontal
      ? { x: rect.x + (offset / 100) * rect.w, y: rect.y, w: fraction * rect.w, h: rect.h }
      : { x: rect.x, y: rect.y + (offset / 100) * rect.h, w: rect.w, h: fraction * rect.h };
    offset += root.sizes[index];
    return paneRects(child, childRect);
  });
}

// The pane spatially adjacent to `fromPaneId` in `direction`, or undefined at an edge. Considers
// only panes strictly on that side; prefers ones whose perpendicular span overlaps the source,
// then the nearest by primary-axis gap, then by perpendicular center distance.
export function neighborPane(
  root: LayoutNode,
  fromPaneId: string,
  direction: FocusDirection,
): string | undefined {
  const EPS = 0.5;
  const rects = paneRects(root);
  const from = rects.find((entry) => entry.paneId === fromPaneId);
  if (!from) return undefined;
  const f = from.rect;
  const span = (aStart: number, aLen: number, bStart: number, bLen: number) =>
    Math.max(0, Math.min(aStart + aLen, bStart + bLen) - Math.max(aStart, bStart));

  const scored = rects
    .filter((entry) => entry.paneId !== fromPaneId)
    .map(({ paneId, rect: c }) => {
      const horizontal = direction === "left" || direction === "right";
      const gap =
        direction === "left"
          ? f.x - (c.x + c.w)
          : direction === "right"
            ? c.x - (f.x + f.w)
            : direction === "up"
              ? f.y - (c.y + c.h)
              : c.y - (f.y + f.h);
      const overlap = horizontal ? span(f.y, f.h, c.y, c.h) : span(f.x, f.w, c.x, c.w);
      const perp = horizontal
        ? Math.abs(c.y + c.h / 2 - (f.y + f.h / 2))
        : Math.abs(c.x + c.w / 2 - (f.x + f.w / 2));
      return { paneId, gap, overlap, perp };
    })
    .filter((entry) => entry.gap >= -EPS);
  if (scored.length === 0) return undefined;

  const overlapping = scored.filter((entry) => entry.overlap > EPS);
  const pool = overlapping.length > 0 ? overlapping : scored;
  pool.sort((a, b) => a.gap - b.gap || a.perp - b.perp);
  return pool[0].paneId;
}
export function isValidLayout(root: LayoutNode): boolean {
  if (root.type === "pane") return root.paneId.length > 0;
  if (root.children.length < 2 || root.sizes.length !== root.children.length) return false;
  return (
    Math.abs(root.sizes.reduce((a, b) => a + b, 0) - 100) < 0.1 &&
    root.sizes.every((size) => size >= 0 && size <= 100) &&
    root.children.every(isValidLayout)
  );
}
function mapNode(
  root: LayoutNode,
  targetId: string,
  transform: (node: LayoutNode) => LayoutNode,
): LayoutNode {
  if (root.type === "pane") return root.paneId === targetId ? transform(root) : root;
  return { ...root, children: root.children.map((child) => mapNode(child, targetId, transform)) };
}
function normalizeSizes(sizes: number[]): number[] {
  const sum = sizes.reduce((a, b) => a + b, 0);
  if (sum <= 0) throw new Error("Split sizes must be positive");
  return sizes.map((size) => (size / sum) * 100);
}

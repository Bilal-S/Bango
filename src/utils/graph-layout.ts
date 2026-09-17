import type Graph from 'graphology';

/** Golden angle in radians (sunflower phyllotaxis). */
const GOLDEN_ANGLE = 2.399963229728653;

/**
 * Deterministic, evenly spaced seed position for layout initialization
 * (sunflower spiral). Replaces `Math.random()` seeding, which can place two
 * nodes nearly coincident and leave them stuck in a ForceAtlas2 local
 * minimum (observed: two author nodes 0.12 units apart in a graph whose
 * typical spacing is ~18-30).
 *
 * @param index - Node index in `[0, total)`.
 * @param total - Total node count.
 * @param scale - Spiral radius; all seeds stay within it.
 */
export function spiralPosition(
  index: number,
  total: number,
  scale = 100
): { x: number; y: number } {
  const angle = index * GOLDEN_ANGLE;
  const r = scale * Math.sqrt((index + 0.5) / Math.max(total, 1));
  return { x: r * Math.cos(angle), y: r * Math.sin(angle) };
}

/** Options for {@link applyMinimumSeparation}. */
export interface MinimumSeparationOptions {
  /** Absolute minimum node spacing. Default: 1% of the largest extent. */
  minDistance?: number;
}

/**
 * Post-layout guard: push apart nodes that ended up (nearly) coincident.
 *
 * ForceAtlas2 can leave a pair of nodes at a near-zero distance when they
 * were seeded together and share attraction hubs; the renderer then stacks
 * their labels. This pass walks nodes in insertion order against a spatial
 * grid (cell size = `minDistance`), moving any node that lands within
 * `minDistance` of an already-placed node out along their connecting vector
 * (a deterministic angle when exactly coincident).
 *
 * @returns The number of nodes moved.
 */
export function applyMinimumSeparation(g: Graph, options: MinimumSeparationOptions = {}): number {
  if (g.order < 2) return 0;

  let minDistance = options.minDistance;
  if (minDistance === undefined) {
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    g.forEachNode((_n, attrs) => {
      const x = (attrs.x as number) ?? 0;
      const y = (attrs.y as number) ?? 0;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    });
    const extent = Math.max(maxX - minX, maxY - minY);
    minDistance = Math.max(extent * 0.01, 1e-6);
  }
  const cell = Math.max(minDistance, 1e-6);
  const clear = minDistance * 1.05;
  const grid = new Map<string, Array<{ x: number; y: number }>>();
  let moved = 0;

  g.forEachNode((id, attrs) => {
    let x = (attrs.x as number) ?? 0;
    let y = (attrs.y as number) ?? 0;
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      x = 0;
      y = 0;
    }
    const startX = x;
    const startY = y;

    // Push away from clashing placed nodes until clear (bounded rounds).
    for (let round = 0; round < 10; round++) {
      const cx = Math.floor(x / cell);
      const cy = Math.floor(y / cell);
      let clashX = 0;
      let clashY = 0;
      let found = false;
      for (let dx = -1; dx <= 1 && !found; dx++) {
        for (let dy = -1; dy <= 1 && !found; dy++) {
          const bucket = grid.get(`${cx + dx}|${cy + dy}`);
          if (!bucket) continue;
          for (const p of bucket) {
            const ddx = x - p.x;
            const ddy = y - p.y;
            const dist = Math.hypot(ddx, ddy);
            if (dist < minDistance) {
              if (dist > 1e-9) {
                clashX = p.x + (ddx / dist) * clear;
                clashY = p.y + (ddy / dist) * clear;
              } else {
                // Exactly coincident: deterministic 30-degree escape vector.
                clashX = p.x + Math.cos(Math.PI / 6) * clear;
                clashY = p.y + Math.sin(Math.PI / 6) * clear;
              }
              found = true;
              break;
            }
          }
        }
      }
      if (!found) break;
      x = clashX;
      y = clashY;
    }

    if (x !== startX || y !== startY) {
      g.setNodeAttribute(id, 'x', x);
      g.setNodeAttribute(id, 'y', y);
      moved++;
    }
    const key = `${Math.floor(x / cell)}|${Math.floor(y / cell)}`;
    const bucket = grid.get(key);
    if (bucket) bucket.push({ x, y });
    else grid.set(key, [{ x, y }]);
  });

  return moved;
}

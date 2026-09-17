import { describe, it, expect } from 'vitest';
import Graph from 'graphology';
import { applyMinimumSeparation, spiralPosition } from '@/utils/graph-layout';

describe('spiralPosition', () => {
  it('is deterministic for the same inputs', () => {
    expect(spiralPosition(7, 100)).toEqual(spiralPosition(7, 100));
    expect(spiralPosition(7, 100)).not.toEqual(spiralPosition(8, 100));
  });

  it('keeps sunflower-level spacing between seeds (no coincident starts)', () => {
    const N = 100;
    const pts = Array.from({ length: N }, (_, i) => spiralPosition(i, N));
    let min = Infinity;
    for (let i = 0; i < N; i++) {
      for (let j = i + 1; j < N; j++) {
        min = Math.min(min, Math.hypot(pts[i]!.x - pts[j]!.x, pts[i]!.y - pts[j]!.y));
      }
    }
    // Sunflower min spacing for scale=100, N=100 is ~19; 8 is a safe floor.
    expect(min).toBeGreaterThan(8);
  });

  it('stays within the seeding scale', () => {
    const p = spiralPosition(999, 1000);
    expect(Math.hypot(p.x, p.y)).toBeLessThanOrEqual(100);
  });
});

describe('applyMinimumSeparation', () => {
  it('is a no-op for well-spaced graphs', () => {
    const g = new Graph();
    g.addNode('a', { x: 0, y: 0 });
    g.addNode('b', { x: 10, y: 0 });
    g.addNode('c', { x: 0, y: 10 });
    const moved = applyMinimumSeparation(g, { minDistance: 1 });
    expect(moved).toBe(0);
    expect(g.getNodeAttribute('a', 'x')).toBe(0);
    expect(g.getNodeAttribute('b', 'x')).toBe(10);
  });

  it('separates a coincident pair to at least minDistance', () => {
    const g = new Graph();
    g.addNode('a', { x: 5, y: 5 });
    g.addNode('b', { x: 5, y: 5 });
    const moved = applyMinimumSeparation(g, { minDistance: 5 });
    expect(moved).toBeGreaterThanOrEqual(1);
    const d = Math.hypot(
      (g.getNodeAttribute('a', 'x') as number) - (g.getNodeAttribute('b', 'x') as number),
      (g.getNodeAttribute('a', 'y') as number) - (g.getNodeAttribute('b', 'y') as number)
    );
    expect(d).toBeGreaterThanOrEqual(5);
    expect(Number.isFinite(g.getNodeAttribute('a', 'x') as number)).toBe(true);
    expect(Number.isFinite(g.getNodeAttribute('b', 'y') as number)).toBe(true);
  });

  it('derives the default minDistance from the graph extent (1%)', () => {
    const g = new Graph();
    // Extent 100x100 via corner anchors, plus a coincident pair in the middle.
    g.addNode('c1', { x: 0, y: 0 });
    g.addNode('c2', { x: 100, y: 100 });
    g.addNode('p', { x: 50, y: 50 });
    g.addNode('q', { x: 50, y: 50 });
    const moved = applyMinimumSeparation(g);
    expect(moved).toBeGreaterThanOrEqual(1);
    const d = Math.hypot(
      (g.getNodeAttribute('p', 'x') as number) - (g.getNodeAttribute('q', 'x') as number),
      (g.getNodeAttribute('p', 'y') as number) - (g.getNodeAttribute('q', 'y') as number)
    );
    expect(d).toBeGreaterThanOrEqual(1);
    // Anchors are far apart; they must not have moved.
    expect(g.getNodeAttribute('c1', 'x')).toBe(0);
    expect(g.getNodeAttribute('c2', 'x')).toBe(100);
  });

  it('separates a coincident triple pairwise', () => {
    const g = new Graph();
    g.addNode('a', { x: 3, y: 3 });
    g.addNode('b', { x: 3, y: 3 });
    g.addNode('c', { x: 3, y: 3 });
    applyMinimumSeparation(g, { minDistance: 2 });
    const xs = ['a', 'b', 'c'].map((n) => g.getNodeAttribute(n, 'x') as number);
    const ys = ['a', 'b', 'c'].map((n) => g.getNodeAttribute(n, 'y') as number);
    const d01 = Math.hypot(xs[0]! - xs[1]!, ys[0]! - ys[1]!);
    const d02 = Math.hypot(xs[0]! - xs[2]!, ys[0]! - ys[2]!);
    const d12 = Math.hypot(xs[1]! - xs[2]!, ys[1]! - ys[2]!);
    expect(d01).toBeGreaterThanOrEqual(2);
    expect(d02).toBeGreaterThanOrEqual(2);
    expect(d12).toBeGreaterThanOrEqual(2);
  });

  it('handles empty and single-node graphs', () => {
    const empty = new Graph();
    expect(applyMinimumSeparation(empty)).toBe(0);
    const single = new Graph();
    single.addNode('only', { x: 1, y: 1 });
    expect(applyMinimumSeparation(single)).toBe(0);
  });
});

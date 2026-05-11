import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('nextPaint', () => {
  let rafCallbacks: FrameRequestCallback[];

  beforeEach(() => {
    rafCallbacks = [];
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      rafCallbacks.push(cb);
      return rafCallbacks.length;
    });
  });

  async function flushRafs(): Promise<void> {
    // Flush all queued rAF callbacks in order
    while (rafCallbacks.length > 0) {
      const cb = rafCallbacks.shift()!;
      cb(0); // pass a dummy timestamp
      // Allow microtasks (promise resolutions) to settle
      await new Promise((r) => setTimeout(r, 0));
    }
  }

  it('returns a promise', async () => {
    const { nextPaint } = await import('@/utils/next-paint');
    const result = nextPaint();
    expect(result).toBeInstanceOf(Promise);
    await flushRafs();
  });

  it('calls requestAnimationFrame twice (double-rAF pattern)', async () => {
    const { nextPaint } = await import('@/utils/next-paint');
    const promise = nextPaint();

    // First rAF should have been scheduled
    expect(rafCallbacks.length).toBe(1);

    // Flush the first rAF - should schedule the second
    rafCallbacks.shift()!(0);
    expect(rafCallbacks.length).toBe(1);

    // Flush the second rAF - promise should resolve
    rafCallbacks.shift()!(0);
    await promise;
  });

  it('resolves after two animation frames', async () => {
    const { nextPaint } = await import('@/utils/next-paint');
    let resolved = false;
    const promise = nextPaint();
    promise.then(() => {
      resolved = true;
    });

    // Not resolved after 0 frames
    expect(resolved).toBe(false);

    // Not resolved after 1 frame
    rafCallbacks.shift()!(0);
    await new Promise((r) => setTimeout(r, 0));
    expect(resolved).toBe(false);

    // Resolved after 2 frames
    rafCallbacks.shift()!(0);
    await promise;
    expect(resolved).toBe(true);
  });

  it('does not resolve if frames are not flushed', async () => {
    const { nextPaint } = await import('@/utils/next-paint');
    let resolved = false;
    nextPaint().then(() => {
      resolved = true;
    });

    // Give microtasks a chance to settle
    await new Promise((r) => setTimeout(r, 0));
    expect(resolved).toBe(false);
  });
});

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NexusBridge } from '../../lib/nexus-bridge';

// A view that passes its own AbortSignal to bridge.fetch must be able to
// cancel the real request, and such a request must not share the 5 s GET cache.
describe('NexusBridge.fetch — caller AbortSignal', () => {
  let seenSignals: AbortSignal[];

  beforeEach(() => {
    seenSignals = [];
    vi.stubGlobal(
      'fetch',
      vi.fn((_url: string, init: RequestInit) => {
        seenSignals.push(init.signal as AbortSignal);
        return new Promise((_resolve, reject) => {
          const signal = init.signal as AbortSignal;
          // Like the real fetch: reject at once if the signal is already aborted.
          if (signal.aborted) {
            reject(new DOMException('aborted', 'AbortError'));
            return;
          }
          signal.addEventListener('abort', () =>
            reject(new DOMException('aborted', 'AbortError')),
          );
        });
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('aborting the caller signal aborts the underlying request', async () => {
    const bridge = new NexusBridge(() => undefined, () => undefined, 'http://127.0.0.1:1');
    const controller = new AbortController();
    const pending = bridge.fetch('/api/v1/sig-abort', { signal: controller.signal });
    expect(seenSignals).toHaveLength(1);
    expect(seenSignals[0].aborted).toBe(false);
    controller.abort();
    await expect(pending).rejects.toBeDefined();
    expect(seenSignals[0].aborted).toBe(true);
  });

  it('an already-aborted caller signal aborts immediately', async () => {
    const bridge = new NexusBridge(() => undefined, () => undefined, 'http://127.0.0.1:1');
    const controller = new AbortController();
    controller.abort();
    const pending = bridge.fetch('/api/v1/sig-pre', { signal: controller.signal });
    await expect(pending).rejects.toBeDefined();
    expect(seenSignals[0].aborted).toBe(true);
  });

  it('requests with their own signal do not share a cached promise', async () => {
    const bridge = new NexusBridge(() => undefined, () => undefined, 'http://127.0.0.1:1');
    const a = new AbortController();
    const b = new AbortController();
    const first = bridge.fetch('/api/v1/sig-cache', { signal: a.signal });
    const second = bridge.fetch('/api/v1/sig-cache', { signal: b.signal });
    expect(seenSignals).toHaveLength(2);
    a.abort();
    await expect(first).rejects.toBeDefined();
    expect(seenSignals[1].aborted).toBe(false);
    b.abort();
    await expect(second).rejects.toBeDefined();
  });
});

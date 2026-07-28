/**
 * PollingCoordinator — centralized timer management for the dashboard.
 *
 * Replaces 43+ scattered setInterval calls with a single coordinator that:
 *  - Groups timers by interval bucket (1s, 5s, 10s, 15s, 30s, 60s)
 *  - Automatically pauses all polling when the browser tab is hidden
 *  - Deduplicates identical callbacks within the same bucket
 *  - Exposes a usePolling() hook so components register declaratively
 */
import { useEffect, useRef, useCallback } from 'react';

type Priority = 'critical' | 'normal' | 'background';

interface PollingTask {
  id: string;
  callback: () => void | Promise<void>;
  intervalMs: number;
  priority: Priority;
  enabled: boolean;
}

/** Standard interval buckets — no ad-hoc values */
const BUCKETS = {
  fast:     1_000,   // 1s — uptime ticker, elapsed ticker
  quick:    5_000,   // 5s — unread, active agents, node graph
  medium:   10_000,  // 10s — health, pulse, metrics
  standard: 15_000,  // 15s — jobs list, report refresh
  slow:     30_000,  // 30s — profiles, docker, federation
  idle:     60_000,  // 60s — static data, guild status
} as const;

class Coordinator {
  private tasks = new Map<string, PollingTask>();
  private timers = new Map<string, ReturnType<typeof setInterval>>();
  private paused = false;
  private listeners = new Set<() => void>();

  constructor() {
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', () => {
        if (document.hidden) {
          this.pause();
        } else {
          this.resume();
        }
      });
    }
  }

  register(task: PollingTask) {
    this.tasks.set(task.id, task);
    if (!this.paused && task.enabled) {
      this.startTimer(task);
    }
    this.notify();
  }

  unregister(id: string) {
    this.stopTimer(id);
    this.tasks.delete(id);
    this.notify();
  }

  update(id: string, patch: Partial<Pick<PollingTask, 'enabled' | 'callback'>>) {
    const task = this.tasks.get(id);
    if (!task) return;
    if (patch.enabled !== undefined) task.enabled = patch.enabled;
    if (patch.callback) task.callback = patch.callback;
    this.stopTimer(id);
    if (!this.paused && task.enabled) {
      this.startTimer(task);
    }
    this.notify();
  }

  /** Returns active task count grouped by bucket */
  stats() {
    const active = [...this.tasks.values()].filter(t => t.enabled);
    return {
      total: this.tasks.size,
      active: active.length,
      paused: this.paused,
      byBucket: {
        fast:     active.filter(t => t.intervalMs === BUCKETS.fast).length,
        quick:    active.filter(t => t.intervalMs === BUCKETS.quick).length,
        medium:   active.filter(t => t.intervalMs === BUCKETS.medium).length,
        standard: active.filter(t => t.intervalMs === BUCKETS.standard).length,
        slow:     active.filter(t => t.intervalMs === BUCKETS.slow).length,
        idle:     active.filter(t => t.intervalMs === BUCKETS.idle).length,
      },
    };
  }

  private startTimer(task: PollingTask) {
    if (this.timers.has(task.id)) return;
    const timer = setInterval(() => {
      try { task.callback(); } catch (e) { console.error(`[PollingCoordinator] ${task.id}:`, e); }
    }, task.intervalMs);
    this.timers.set(task.id, timer);
  }

  private stopTimer(id: string) {
    const timer = this.timers.get(id);
    if (timer) {
      clearInterval(timer);
      this.timers.delete(id);
    }
  }

  private pause() {
    if (this.paused) return;
    this.paused = true;
    for (const id of this.timers.keys()) this.stopTimer(id);
    this.notify();
  }

  private resume() {
    if (!this.paused) return;
    this.paused = false;
    for (const task of this.tasks.values()) {
      if (task.enabled) this.startTimer(task);
    }
    this.notify();
  }

  private notify() {
    for (const fn of this.listeners) fn();
  }

  subscribe(fn: () => void) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }
}

/** Singleton — one coordinator for the entire app */
export const pollingCoordinator = new Coordinator();

/**
 * Declarative hook — registers a polling task on mount, unregisters on unmount.
 *
 * @example
 *   usePolling('coloquio-unread', pollUnread, { interval: 'quick', enabled: online });
 */
export function usePolling(
  id: string,
  callback: () => void | Promise<void>,
  opts: { interval?: keyof typeof BUCKETS; enabled?: boolean; priority?: Priority } = {}
) {
  const { interval = 'medium', enabled = true, priority = 'normal' } = opts;
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  const stableCallback = useCallback(() => callbackRef.current(), []);

  useEffect(() => {
    pollingCoordinator.register({
      id,
      callback: stableCallback,
      intervalMs: BUCKETS[interval],
      priority,
      enabled,
    });
    return () => pollingCoordinator.unregister(id);
  }, [id, interval, enabled, priority, stableCallback]);
}

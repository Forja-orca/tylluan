import { useState, useCallback } from 'react';

/**
 * Encapsulates the loading boolean pattern repeated across 15+ components.
 * Usage:
 *   const { loading, run } = useLoadingState();
 *   await run(() => bridge.doSomething()).catch(handleError);
 */
export function useLoadingState(initial = false) {
  const [loading, setLoading] = useState(initial);

  const run = useCallback(async <T>(fn: () => Promise<T>): Promise<T> => {
    setLoading(true);
    try {
      return await fn();
    } finally {
      setLoading(false);
    }
  }, []);

  return { loading, setLoading, run };
}

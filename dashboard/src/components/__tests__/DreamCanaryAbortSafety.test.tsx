import { render, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { DreamStatusWidget } from '../DreamStatusWidget';
import { CanaryStatusWidget } from '../CanaryStatusWidget';
import type { NexusBridge } from '../../lib/api-client';

vi.mock('../../hooks/usePolling', () => ({
  usePolling: vi.fn(),
}));

describe('DreamStatusWidget & CanaryStatusWidget — AbortError safety', () => {
  it('no registra console.error cuando fetchRaw aborta con DOMException AbortError', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const abortErr = new DOMException('The operation was aborted', 'AbortError');
    const mockBridge = {
      fetchRaw: vi.fn().mockRejectedValue(abortErr),
    } as unknown as NexusBridge;

    const { unmount: unmountDream } = render(<DreamStatusWidget bridge={mockBridge} />);
    const { unmount: unmountCanary } = render(<CanaryStatusWidget bridge={mockBridge} />);

    await waitFor(() => {
      expect(mockBridge.fetchRaw).toHaveBeenCalledTimes(2);
    });

    unmountDream();
    unmountCanary();

    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });
});

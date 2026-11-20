import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { HomeostasisWidget } from '../HomeostasisWidget';
import type { NexusBridge } from '../../lib/nexus-bridge';

// UI cycle 1 (consumption): /api/v1/health/detailed is the most expensive call the
// dashboard makes (53 s cold). The shared hook already fetches it, so the widget
// must reuse that data and never poll on its own when it is handed the value.
function bridgeWith(spy: ReturnType<typeof vi.fn>): NexusBridge {
  return { health_detailed: spy } as unknown as NexusBridge;
}

describe('HomeostasisWidget health source', () => {
  it('does not call health_detailed when the shared health is passed', async () => {
    const spy = vi.fn().mockResolvedValue({ status: 'healthy', score: 10 });
    render(
      <HomeostasisWidget
        bridge={bridgeWith(spy)}
        health={{ status: 'healthy', score: 92 }}
      />,
    );
    await waitFor(() => expect(screen.getByText('Córtex Estable')).toBeTruthy());
    expect(spy).not.toHaveBeenCalled();
  });

  it('shows the shared score, not a locally fetched one', async () => {
    const spy = vi.fn().mockResolvedValue({ status: 'healthy', score: 10 });
    render(
      <HomeostasisWidget
        bridge={bridgeWith(spy)}
        health={{ status: 'healthy', score: 92 }}
      />,
    );
    await waitFor(() => expect(screen.getByText('92')).toBeTruthy());
  });

  it('keeps the old behaviour (fetches once) when no shared health is passed', async () => {
    const spy = vi.fn().mockResolvedValue({ status: 'healthy', score: 77 });
    render(<HomeostasisWidget bridge={bridgeWith(spy)} />);
    await waitFor(() => expect(spy).toHaveBeenCalled());
  });
});

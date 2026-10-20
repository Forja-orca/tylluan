import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';

// D5 — /health?verbose=true exposes components.sovereign_tools {ok,count}
// (crates/tylluan-kernel/src/transport/http/mod.rs:1394-1397). The Trust
// Console must show the count and flag it red when count != 6 or ok = false.

const fetchRaw = vi.fn();

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({ bridge: { fetchRaw } }),
}));
vi.mock('../../hooks/usePolling', () => ({
  usePolling: () => undefined,
}));

import { TrustConsoleTab } from '../TrustConsoleTab';

function mockHealth(sovereign: { ok: boolean; count: number } | undefined) {
  fetchRaw.mockImplementation((url: string) => {
    if (url === '/health?verbose=true') {
      return Promise.resolve({
        status: 'healthy',
        version: '0.17.1',
        commit: '69061e5abcdef',
        components: sovereign ? { sovereign_tools: sovereign } : {},
      });
    }
    if (url === '/api/v1/guilds') return Promise.resolve([]);
    return Promise.resolve({});
  });
}

describe('TrustConsoleTab — sovereign tools counter (D5)', () => {
  beforeEach(() => fetchRaw.mockReset());

  it('shows 6 / 6 VERIFIED when the kernel reports count=6, ok=true', async () => {
    mockHealth({ ok: true, count: 6 });
    render(<TrustConsoleTab />);
    await waitFor(() => expect(screen.getByText('VERIFIED')).toBeDefined());
    expect(screen.getByText(/6 \/ 6/)).toBeDefined();
    expect(screen.queryByText('DRIFT ALERT')).toBeNull();
  });

  it('shows DRIFT ALERT when count != 6', async () => {
    mockHealth({ ok: false, count: 5 });
    render(<TrustConsoleTab />);
    await waitFor(() => expect(screen.getByText('DRIFT ALERT')).toBeDefined());
    expect(screen.getByText(/5 \/ 6/)).toBeDefined();
    expect(screen.getByText(/expected 6 tools \(ok=true\), got count=5 \(ok=false\)/)).toBeDefined();
  });

  it('shows DRIFT ALERT when ok=false even if count is 6', async () => {
    mockHealth({ ok: false, count: 6 });
    render(<TrustConsoleTab />);
    await waitFor(() => expect(screen.getByText('DRIFT ALERT')).toBeDefined());
  });

  it('shows PENDING (not a fake 6/6) when the field is absent', async () => {
    mockHealth(undefined);
    render(<TrustConsoleTab />);
    await waitFor(() => expect(screen.getByText('PENDING')).toBeDefined());
    expect(screen.getByText(/— \/ 6/)).toBeDefined();
  });

  describe('Salud del recall integration (LOTE AD)', () => {
    it('shows "kernel sin telemetría de recall (actualiza)" when recall field is absent in components', async () => {
      fetchRaw.mockImplementation((url: string) => {
        if (url === '/health?verbose=true') {
          return Promise.resolve({
            status: 'healthy',
            version: '0.17.1',
            commit: '69061e5abcdef',
            components: { sovereign_tools: { ok: true, count: 6 } }, // No recall field
          });
        }
        return Promise.resolve([]);
      });

      render(<TrustConsoleTab />);
      await waitFor(() => expect(screen.getByText('kernel sin telemetría de recall (actualiza)')).toBeDefined());
    });

    it('shows recall metrics when components.recall is provided', async () => {
      fetchRaw.mockImplementation((url: string) => {
        if (url === '/health?verbose=true') {
          return Promise.resolve({
            status: 'healthy',
            version: '0.17.1',
            commit: '69061e5abcdef',
            components: {
              sovereign_tools: { ok: true, count: 6 },
              recall: {
                calls: 150,
                empty_calls: 15,
                empty_ratio: 0.1,
                mean_latency_ms: 22.5,
                latency_histogram: { '<=5ms': 10, '<=10ms': 20 },
              },
            },
          });
        }
        return Promise.resolve([]);
      });

      render(<TrustConsoleTab />);
      await waitFor(() => expect(screen.getByText('10.0%')).toBeDefined());
      expect(screen.getByText('22.5 ms')).toBeDefined();
    });
  });
});

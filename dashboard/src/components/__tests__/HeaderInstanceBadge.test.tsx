import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { HeaderInstanceBadge } from '../ui/HeaderInstanceBadge';
import type { NexusBridge } from '../../lib/nexus-bridge';

describe('HeaderInstanceBadge (T4368)', () => {
  it('muestra puerto · commit sin aviso cuando coincide con el commit de compilación', async () => {
    const bridge = {
      getBaseUrl: () => 'http://127.0.0.1:47005',
      fetchRaw: vi.fn().mockResolvedValue({
        status: 'ok',
        version: '0.17.2',
        commit: '90785663',
      }),
    } as unknown as NexusBridge;

    render(<HeaderInstanceBadge bridge={bridge} buildCommit="90785663" />);

    await waitFor(() => {
      const badge = screen.getByTestId('header-instance-badge');
      expect(badge).toHaveTextContent(':47005 · 90785663');
    });
    expect(screen.queryByText(/Commit distinto/i)).not.toBeInTheDocument();
  });

  it('muestra aviso visible cuando el commit del kernel difiere del commit del dashboard', async () => {
    const bridge = {
      getBaseUrl: () => 'http://127.0.0.1:47004',
      fetchRaw: vi.fn().mockResolvedValue({
        status: 'ok',
        version: '0.17.2',
        commit: '8a893aff',
      }),
    } as unknown as NexusBridge;

    render(<HeaderInstanceBadge bridge={bridge} buildCommit="90785663" />);

    await waitFor(() => {
      const badge = screen.getByTestId('header-instance-badge');
      expect(badge).toHaveTextContent(':47004 · 8a893aff');
      expect(badge).toHaveTextContent(/Commit distinto \(90785663\)/i);
    });
  });

  it('muestra agent_id y rol reales desde /api/v1/whoami en la insignia y tooltip', async () => {
    const fetchRaw = vi.fn().mockImplementation(async (url: string) => {
      if (url === '/health') {
        return { status: 'ok', version: '0.17.2', commit: '621c9b44' };
      }
      if (url === '/api/v1/whoami') {
        return { agent_id: 'operator', activity: { role: 'admin' } };
      }
      throw new Error('HTTP Error: 404');
    });

    const bridge = {
      getBaseUrl: () => 'http://127.0.0.1:47005',
      fetchRaw,
    } as unknown as NexusBridge;

    render(<HeaderInstanceBadge bridge={bridge} buildCommit="621c9b44" />);

    await waitFor(() => {
      const badge = screen.getByTestId('header-instance-badge');
      expect(badge).toHaveTextContent('operator · :47005 · 621c9b44');
      expect(badge).toHaveAttribute('title', expect.stringContaining('whoami operator (admin)'));
    });
  });

  it('degrada sin romper si /api/v1/whoami devuelve 401', async () => {
    const fetchRaw = vi.fn().mockImplementation(async (url: string) => {
      if (url === '/health') {
        return { status: 'ok', version: '0.17.2', commit: '621c9b44' };
      }
      if (url === '/api/v1/whoami') {
        throw new Error('Unauthorized');
      }
      throw new Error('HTTP Error: 404');
    });

    const bridge = {
      getBaseUrl: () => 'http://127.0.0.1:47004',
      fetchRaw,
    } as unknown as NexusBridge;

    render(<HeaderInstanceBadge bridge={bridge} buildCommit="621c9b44" />);

    await waitFor(() => {
      const badge = screen.getByTestId('header-instance-badge');
      expect(badge).toHaveTextContent(':47004 · 621c9b44');
    });
  });
});

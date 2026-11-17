import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import axe from 'axe-core';
import { HeaderProdLagBadge } from '../ui/HeaderProdLagBadge';
import type { NexusBridge } from '../../lib/nexus-bridge';

describe('HeaderProdLagBadge (T4511)', () => {
  it('muestra estado cargando mientras /health está pendiente y pasa axe', async () => {
    const bridge = {
      fetchRaw: vi.fn(() => new Promise(() => {})),
    } as unknown as NexusBridge;

    const { container } = render(
      <HeaderProdLagBadge bridge={bridge} mainCommit="cd420050" />,
    );

    const badge = screen.getByTestId('header-prod-lag-badge');
    expect(badge).toHaveTextContent(/Comprobando despliegue/i);
    expect(badge).toHaveAttribute('aria-busy', 'true');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    const results = await axe.run(container);
    expect(results.violations).toEqual([]);
  });

  it('muestra Producción al día cuando el commit de /health coincide con main y pasa axe', async () => {
    const bridge = {
      fetchRaw: vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({ status: 'ok', commit: 'cd420050' }),
      }),
    } as unknown as NexusBridge;

    const { container } = render(
      <HeaderProdLagBadge bridge={bridge} mainCommit="cd420050" />,
    );

    await waitFor(() => {
      expect(screen.getByTestId('header-prod-lag-badge')).toHaveTextContent(
        /Producción al día/i,
      );
    });
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    const results = await axe.run(container);
    expect(results.violations).toEqual([]);
  });

  it('muestra Producción atrasada · N commits sin desplegar (solo lectura, sin botones) y pasa axe', async () => {
    const bridge = {
      fetchRaw: vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({ status: 'ok', commit: '8a893aff' }),
      }),
    } as unknown as NexusBridge;

    const { container } = render(
      <HeaderProdLagBadge
        bridge={bridge}
        mainCommit="cd420050"
        undeployedCommits={7}
      />,
    );

    await waitFor(() => {
      const badge = screen.getByTestId('header-prod-lag-badge');
      expect(badge).toHaveTextContent(/Producción atrasada · 7 commits sin desplegar/i);
    });
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    const results = await axe.run(container);
    expect(results.violations).toEqual([]);
  });

  it('sin cifra conocida NO inventa el número de commits (ni /health ni la prop lo dan)', async () => {
    const bridge = {
      fetchRaw: vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({ status: 'ok', commit: '8a893aff' }),
      }),
    } as unknown as NexusBridge;

    render(<HeaderProdLagBadge bridge={bridge} mainCommit="cd420050" />);

    await waitFor(() => {
      const badge = screen.getByTestId('header-prod-lag-badge');
      expect(badge).toHaveTextContent(/Producción atrasada · versión anterior a main/i);
      expect(badge).not.toHaveTextContent(/\d+ commits? sin desplegar/i);
    });
  });

  it('muestra error de lectura si /health falla (sin botones de reinicio) y pasa axe', async () => {
    const bridge = {
      fetchRaw: vi.fn().mockRejectedValue(new Error('network down')),
    } as unknown as NexusBridge;

    const { container } = render(
      <HeaderProdLagBadge bridge={bridge} mainCommit="cd420050" />,
    );

    await waitFor(() => {
      expect(screen.getByTestId('header-prod-lag-badge')).toHaveTextContent(
        /Estado de despliegue no disponible/i,
      );
    });
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    const results = await axe.run(container);
    expect(results.violations).toEqual([]);
  });
});

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import axe from 'axe-core';
import { StrataConcurrencyPanel, type StrataStatusPayload } from '../StrataConcurrencyPanel';

describe('StrataConcurrencyPanel (T4500)', () => {
  it('renderiza métricas mock de serving/requested, ttft y tok/s por agente sin violaciones axe', async () => {
    const { container } = render(<StrataConcurrencyPanel />);

    expect(screen.getByRole('heading', { name: /Concurrencia \(Strata\)/i })).toBeInTheDocument();
    expect(screen.getByTestId('strata-serving-count')).toHaveTextContent('2');
    expect(screen.getByTestId('strata-requested-count')).toHaveTextContent('4');
    expect(screen.getByText('@deep')).toBeInTheDocument();
    expect(screen.getByText('142 ms')).toBeInTheDocument();
    expect(screen.getByText('48.6 tok/s')).toBeInTheDocument();

    const results = await axe.run(container);
    expect(results.violations).toEqual([]);
  });

  it('muestra estado cargando con botón deshabilitado y luego datos al resolver', async () => {
    let resolveFn!: (val: StrataStatusPayload) => void;
    const fetchStatus = vi.fn(
      () =>
        new Promise<StrataStatusPayload>((resolve) => {
          resolveFn = resolve;
        }),
    );

    const { container } = render(<StrataConcurrencyPanel fetchStatus={fetchStatus} />);

    expect(screen.getByRole('status')).toHaveTextContent(/Cargando métricas de concurrencia/i);
    expect(screen.getByRole('button', { name: /Consultando\.\.\./i })).toBeDisabled();
    const loadingAxe = await axe.run(container);
    expect(loadingAxe.violations).toEqual([]);

    resolveFn({
      concurrency: { serving: 3, requested: 5 },
      agents: [{ agent_id: 'antigravity', ttft_ms: 95, tok_per_sec: 55.2 }],
    });

    await waitFor(() => {
      expect(screen.getByTestId('strata-serving-count')).toHaveTextContent('3');
      expect(screen.getByTestId('strata-requested-count')).toHaveTextContent('5');
      expect(screen.getByText('@antigravity')).toBeInTheDocument();
      expect(screen.getByText('95 ms')).toBeInTheDocument();
      expect(screen.getByText('55.2 tok/s')).toBeInTheDocument();
    });
  });

  it('muestra estado de error con Reintentar recuperable y pasa axe', async () => {
    const fetchStatus = vi
      .fn()
      .mockRejectedValueOnce(new Error('Strata mock timeout'))
      .mockResolvedValueOnce({
        concurrency: { serving: 1, requested: 1 },
        agents: [{ agent_id: 'dsh', ttft_ms: 120, tok_per_sec: 44.0 }],
      });

    const { container } = render(<StrataConcurrencyPanel fetchStatus={fetchStatus} />);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(/Strata mock timeout/i);
    const errAxe = await axe.run(container);
    expect(errAxe.violations).toEqual([]);

    fireEvent.click(screen.getByRole('button', { name: /Reintentar/i }));
    await waitFor(() => {
      expect(screen.getByText('@dsh')).toBeInTheDocument();
      expect(screen.getByText('44.0 tok/s')).toBeInTheDocument();
    });
    expect(fetchStatus).toHaveBeenCalledTimes(2);
  });

  it('muestra estado vacío cuando agents es [] y pasa axe', async () => {
    const fetchStatus = vi.fn().mockResolvedValue({
      concurrency: { serving: 0, requested: 0 },
      agents: [],
    });

    const { container } = render(<StrataConcurrencyPanel fetchStatus={fetchStatus} />);

    const empty = await screen.findByText(/Sin agentes activos en Strata/i);
    expect(empty).toBeInTheDocument();
    expect(screen.queryByRole('table')).not.toBeInTheDocument();
    const emptyAxe = await axe.run(container);
    expect(emptyAxe.violations).toEqual([]);
  });
});

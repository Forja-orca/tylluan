import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import axe from 'axe-core';
import { RepoImporterMockup } from '../RepoImporterMockup';
import { GuildsTab } from '../GuildsTab';
import type { NexusBridge } from '../../lib/nexus-bridge';

const EMPTY_GUILDS: never[] = [];
const MOCK_REFRESH = vi.fn();

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({ guilds: EMPTY_GUILDS, refreshData: MOCK_REFRESH }),
}));

vi.mock('../../hooks/usePolling', () => ({
  usePolling: vi.fn(),
}));

vi.mock('../GuildInspector', () => ({
  GuildInspector: () => <div>MockInspector</div>,
}));

describe('RepoImporterMockup + Caja R5 Accesibilidad (T4479)', () => {
  it('pasa axe-core sin violaciones en idle, error y cuarentena', async () => {
    const { container } = render(<RepoImporterMockup />);

    const idleResults = await axe.run(container);
    expect(idleResults.violations).toEqual([]);

    // Provocar error de validación -> aria-invalid + aria-describedby
    const input = screen.getByLabelText(/URL del repositorio a importar/i);
    fireEvent.change(input, { target: { value: 'invalida' } });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));

    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'repo-importer-url-error');
    const errorResults = await axe.run(container);
    expect(errorResults.violations).toEqual([]);

    // Avanzar a cuarentena -> gestiona foco al panel de estado
    fireEvent.change(input, { target: { value: 'https://github.com/forja-orca/guild-a11y' } });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));

    const statusPanel = await screen.findByText(/Estado: cuarentena/i);
    expect(statusPanel.closest('[role="status"]')).toHaveFocus();
    const activeResults = await axe.run(container);
    expect(activeResults.violations).toEqual([]);
  });

  it('mantiene focus-visible:ring-2 y contraste AA en botones de transición y caja R5 de GuildsTab', async () => {
    const bridge = {
      getGuilds: vi.fn().mockResolvedValue([]),
      getConfig: vi.fn().mockResolvedValue({}),
      getDockerStatus: vi.fn().mockResolvedValue({ status: 'unavailable' }),
      fetchRaw: vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => ({}) }),
    } as unknown as NexusBridge;

    const { container } = render(<GuildsTab bridge={bridge} notify={vi.fn()} events={[]} />);

    const r5Btn = screen.getByRole('button', { name: /Importador R5/i });
    expect(r5Btn.className).toContain('focus-visible:ring-2');

    fireEvent.click(r5Btn);

    const submitBtn = screen.getByRole('button', { name: /Importar repositorio/i });
    expect(submitBtn.className).toContain('focus-visible:ring-2');

    // Avanzar hasta compuertas y verificar botones emerald-700 / rose-700 + focus-visible
    fireEvent.change(screen.getByLabelText(/URL del repositorio a importar/i), {
      target: { value: 'https://github.com/forja-orca/guild-contrast' },
    });
    fireEvent.click(submitBtn);
    fireEvent.click(screen.getByRole('button', { name: /Continuar a escaneo/i }));
    fireEvent.click(screen.getByRole('button', { name: /Generar propuesta/i }));
    fireEvent.click(screen.getByRole('button', { name: /Pasar a compuertas/i }));

    const approveBtn = screen.getByRole('button', { name: /Aprobar compuertas/i });
    const rejectBtn = screen.getByRole('button', { name: /Rechazar/i });
    expect(approveBtn.className).toContain('bg-emerald-700');
    expect(approveBtn.className).toContain('focus-visible:ring-2');
    expect(rejectBtn.className).toContain('bg-rose-700');
    expect(rejectBtn.className).toContain('focus-visible:ring-2');

    const results = await axe.run(container);
    expect(results.violations).toEqual([]);

    await waitFor(() => expect(bridge.getConfig).toHaveBeenCalled());
  });
});

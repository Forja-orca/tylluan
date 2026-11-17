import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { KnowledgeGraphTab } from '../KnowledgeGraphTab';
import type { NexusBridge } from '../../lib/nexus-bridge';

vi.mock('../RadialSilvaGraph', () => ({
  RadialSilvaGraph: () => <div data-testid="mock-radial-graph" />,
}));

vi.mock('../IngestPanel', () => ({
  IngestPanel: () => <div data-testid="mock-ingest-panel" />,
}));

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({ sysStatus: null }),
}));

describe('KnowledgeGraphTab — Textos accionables de error (T4355)', () => {
  it('indica usar el botón Token ante un error 401', async () => {
    const bridge = {
      getSilvaGraph: vi.fn().mockRejectedValue(new Error('HTTP 401 Unauthorized')),
    } as unknown as NexusBridge;

    render(<KnowledgeGraphTab bridge={bridge} notify={vi.fn()} />);
    fireEvent.click(screen.getByRole('tab', { name: /List Explorer/i }));

    await waitFor(() => {
      expect(screen.getByText(/Error al cargar recuerdos de SilvaDB/i)).toBeInTheDocument();
      expect(screen.getByText(/abre el botón «Token» en la cabecera e introduce tu \.tylluan-token/i)).toBeInTheDocument();
    });
  });

  it('indica tylluan-cli start y conexión local ante red caída o kernel parado', async () => {
    const bridge = {
      getSilvaGraph: vi.fn().mockRejectedValue(new Error('Failed to fetch')),
    } as unknown as NexusBridge;

    render(<KnowledgeGraphTab bridge={bridge} notify={vi.fn()} />);
    fireEvent.click(screen.getByRole('tab', { name: /List Explorer/i }));

    await waitFor(() => {
      expect(screen.getByText(/Error al cargar recuerdos de SilvaDB/i)).toBeInTheDocument();
      expect(screen.getByText(/tylluan-cli start/i)).toBeInTheDocument();
    });
  });
});

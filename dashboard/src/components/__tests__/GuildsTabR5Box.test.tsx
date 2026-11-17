import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
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

describe('GuildsTab — Caja R5 del enrutador (T4457)', () => {
  it('permite abrir la subpestaña Importador R5 y muestra la maqueta simulada ADR-033', () => {
    const bridge = {
      getGuilds: vi.fn().mockResolvedValue([]),
      getConfig: vi.fn().mockResolvedValue({}),
      getDockerStatus: vi.fn().mockResolvedValue({ status: 'unavailable' }),
      fetchRaw: vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => ({}) }),
    } as unknown as NexusBridge;

    render(<GuildsTab bridge={bridge} notify={vi.fn()} events={[]} />);

    const r5Btn = screen.getByRole('button', { name: /Importador R5/i });
    expect(r5Btn).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(r5Btn);

    expect(r5Btn).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByLabelText(/URL del repositorio a importar/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Importar repositorio/i })).toBeInTheDocument();
  });
});

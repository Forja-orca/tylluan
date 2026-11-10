import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import App from '../../App';

// Mock de NexusProvider y hooks necesarios para renderizar App limpiamente
vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({
    online: true,
    events: [],
    guilds: [],
    stats: null,
    memoryStats: null,
    approvals: [],
    error: null,
    goldenSignals: null,
    guildsUtilization: null,
    memoryRetention: null,
    sloSummary: null,
    healthDetailed: {
      score: 40,
      status: 'critical',
      components: {
        embeddings: { ok: true },
        reranker: { ok: false },
        guilds: { active: 1, total: 3 },
        silva: { nodes: 10, edges: 5 }
      }
    },
    sysStatus: { dev_mode: false },
    interoception: null,
    bridge: {
      fetchRaw: vi.fn(),
      setToken: vi.fn(),
      getToken: vi.fn(() => 'mock-token'),
      getColoquioThread: vi.fn().mockResolvedValue([]),
      getRepoMap: vi.fn().mockResolvedValue({}),
    },
    refreshData: vi.fn(),
    clearLogs: vi.fn(),
  }),
}));

vi.mock('../../hooks/useNexusSSE', () => ({
  useNexusSSE: () => ({
    connectionStatus: 'connected',
    reconnectAttempts: 0,
    standbyCountdownSecs: 60,
    currentStepSecs: 60,
    isProbing: false,
    triggerImmediateReconnect: vi.fn(),
  }),
}));

vi.mock('../../hooks/useThemeManager', () => {
  let currentTheme = 'dark';
  return {
    useThemeManager: () => ({
      theme: currentTheme,
      setTheme: (t: string) => { currentTheme = t; },
    }),
  };
});

describe('Header Responsive Mobile (390px)', () => {
  it('renderiza el toggle móvil de tema (sm:hidden) y el grupo de escritorio (hidden sm:flex)', () => {
    render(<App />);

    const mobileThemeBtn = screen.getByTestId('mobile-theme-toggle-btn');
    expect(mobileThemeBtn).toBeInTheDocument();
    expect(mobileThemeBtn.className).toContain('sm:hidden');

    const desktopGroup = screen.getByRole('group', { name: /selector de tema/i });
    expect(desktopGroup).toBeInTheDocument();
    expect(desktopGroup.className).toContain('hidden sm:flex');
  });

  it('el chip de salud critical es visible (flex) en móvil y no usa hidden sm:flex', () => {
    render(<App />);

    const healthChip = screen.getByTestId('header-health-status');
    expect(healthChip).toBeInTheDocument();
    expect(healthChip.className).toContain('flex');
    expect(healthChip.className).not.toContain('hidden sm:flex');
    expect(healthChip.textContent).toContain('critical');
  });

  it('el botón integrado de sincronizar/recargar está presente y accesible', () => {
    render(<App />);

    const refreshBtn = screen.getByRole('button', { name: /sincronizar y recargar estado/i });
    expect(refreshBtn).toBeInTheDocument();
    expect(refreshBtn).not.toBeDisabled();
  });
});

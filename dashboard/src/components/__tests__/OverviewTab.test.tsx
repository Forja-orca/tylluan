import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import { OverviewTab } from '../OverviewTab';
import type { AgentProfile, NexusBridge } from '../../lib/api-client';
import type { MemoryStats } from '../../hooks/useNexus';

const mockProfiles: AgentProfile[] = [
  {
    agent_id: 'deep',
    competencies: { rust: 0.95 },
    first_seen: '2026-08-01 12:00:00',
    total_calls: 10,
  },
];

let currentAgentProfiles: AgentProfile[] = [];

const fetchRaw = vi.fn().mockImplementation((url: string) => {
  if (url === '/api/v1/setup-hint') {
    return Promise.resolve({
      mcp_clients: {
        claude_desktop: {
          config: {
            mcpServers: {
              tylluan: { url: 'http://127.0.0.1:47004/sse' },
            },
          },
        },
      },
    });
  }
  return Promise.resolve({});
});

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({
    agentProfiles: currentAgentProfiles,
    bridge: { fetchRaw },
  }),
}));

vi.mock('../../hooks/usePolling', () => ({
  usePolling: () => undefined,
}));

// Mock widgets that may fetch background data or render canvas
vi.mock('../HomeostasisWidget', () => ({
  HomeostasisWidget: () => <div data-testid="mock-homeostasis-widget">Homeostasis Widget</div>,
}));

vi.mock('../DreamStatusWidget', () => ({
  DreamStatusWidget: () => <div data-testid="mock-dream-status-widget">Dream Status Widget</div>,
}));

vi.mock('../CanaryStatusWidget', () => ({
  CanaryStatusWidget: () => <div data-testid="mock-canary-status-widget">Canary Status Widget</div>,
}));

vi.mock('../ui/MetricPrimitives', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../ui/MetricPrimitives');
  return {
    ...actual,
    MiniSparkline: () => <div data-testid="mock-mini-sparkline" />,
  };
});

describe('OverviewTab — Z1 Onboarding Checklist & Populated Kernel Handling', () => {
  beforeEach(() => {
    localStorage.clear();
    currentAgentProfiles = [];
    fetchRaw.mockReset();
    fetchRaw.mockImplementation((url: string) => {
      if (url === '/api/v1/setup-hint') {
        return Promise.resolve({
          mcp_clients: {
            claude_desktop: {
              config: {
                mcpServers: {
                  tylluan: { url: 'http://127.0.0.1:47004/sse' },
                },
              },
            },
          },
        });
      }
      return Promise.resolve({});
    });
  });

  const mockBridge = {
    fetchRaw,
    getBaseUrl: () => 'http://127.0.0.1:47004',
    fetchSessionDigests: vi.fn().mockResolvedValue([]),
  } as unknown as NexusBridge;

  const baseProps = {
    bridge: mockBridge,
    goldenSignals: null,
    guildsUtilization: null,
    memoryRetention: null,
    sloSummary: null,
    guilds: [],
    approvals: [],
    healthDetailed: null,
    sysStatus: { embeddings_loaded: true, silva_healthy: true },
    events: [],
    notify: vi.fn(),
    refreshData: vi.fn().mockResolvedValue(undefined),
    online: true,
  };

  it('renders WelcomeEmptyState when kernel has zero data and clean localStorage', async () => {
    // Zero data in kernel
    currentAgentProfiles = [];
    const memoryStats: MemoryStats = { document_count: 0, disk_usage_bytes: 0, node_count: 0, edge_count: 0 };

    render(
      <OverviewTab
        {...baseProps}
        memoryStats={memoryStats}
      />
    );

    // Welcome checklist should be shown
    expect(await screen.findByText(/Bienvenido a/i)).toBeInTheDocument();
    expect(screen.getByText(/Lista de Inicio/i)).toBeInTheDocument();
    expect(screen.queryByTestId('mock-homeostasis-widget')).not.toBeInTheDocument();
  });

  it('renders main dashboard overview when kernel is populated with nodes (memoryStats.node_count > 0)', () => {
    // Populated kernel with 10518 nodes
    currentAgentProfiles = [];
    const memoryStats: MemoryStats = { document_count: 10518, disk_usage_bytes: 1024, node_count: 10518, edge_count: 19329 };

    render(
      <OverviewTab
        {...baseProps}
        memoryStats={memoryStats}
      />
    );

    // Welcome checklist should NOT be shown
    expect(screen.queryByText(/Bienvenido a/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Lista de Inicio/i)).not.toBeInTheDocument();

    // Main overview widgets are rendered
    expect(screen.getByTestId('mock-homeostasis-widget')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: /Tylluan is/i })).toBeInTheDocument();
  });

  it('renders main dashboard overview when kernel has agent profiles even if memoryStats.node_count is 0', () => {
    // Populated kernel via agent profiles
    currentAgentProfiles = mockProfiles;
    const memoryStats: MemoryStats = { document_count: 0, disk_usage_bytes: 0, node_count: 0, edge_count: 0 };

    render(
      <OverviewTab
        {...baseProps}
        memoryStats={memoryStats}
      />
    );

    // Welcome checklist should NOT be shown
    expect(screen.queryByText(/Bienvenido a/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Lista de Inicio/i)).not.toBeInTheDocument();

    expect(screen.getByTestId('mock-homeostasis-widget')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: /Tylluan is/i })).toBeInTheDocument();
  });

  it('does NOT render WelcomeEmptyState when wizard was already completed in localStorage', () => {
    // Empty kernel, but wizard completed in localStorage
    localStorage.setItem('tylluan_wizard_query', 'true');
    localStorage.setItem('tylluan_wizard_mcp', 'true');
    currentAgentProfiles = [];
    const memoryStats: MemoryStats = { document_count: 0, disk_usage_bytes: 0, node_count: 0, edge_count: 0 };

    render(
      <OverviewTab
        {...baseProps}
        memoryStats={memoryStats}
      />
    );

    expect(screen.queryByText(/Bienvenido a/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Lista de Inicio/i)).not.toBeInTheDocument();
    expect(screen.getByTestId('mock-homeostasis-widget')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: /Tylluan is/i })).toBeInTheDocument();
  });
});

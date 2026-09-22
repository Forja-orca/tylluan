import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import { AgentCompetencyPanel, parseCompetencies } from '../AgentCompetencyPanel';

describe('AgentCompetencyPanel', () => {
  const mockAgentsData = {
    count: 3,
    agents: [
      {
        agent_id: 'deep',
        role: 'builder',
        total_calls: 1420,
        first_seen: '2026-07-28T10:00:00Z',
        last_intent: 'Refactor transport/server and implement Noise XK handshake',
        identity_node: 'agent_identity_deep',
        competencies: {
          bash: 0.96,
          code: 0.94,
          filesystem: 0.91,
        },
        domains: [
          { domain: 'bash', agent_id: 'deep', successes: 48, failures: 2, total: 50, rate: 0.96 },
          { domain: 'code', agent_id: 'deep', successes: 32, failures: 2, total: 34, rate: 0.94 },
        ],
      },
      {
        agent_id: 'antigravity',
        role: 'designer',
        total_calls: 890,
        first_seen: '2026-08-01T12:00:00Z',
        last_intent: 'Design AgentCompetencyPanel with ADR-014 scheduler integration',
        identity_node: 'agent_identity_antigravity',
        competencies: {
          coloquio: 0.98,
          filesystem: 0.88,
          research: 0.92,
        },
        domains: [
          { domain: 'coloquio', agent_id: 'antigravity', successes: 60, failures: 1, total: 61, rate: 0.98 },
          { domain: 'research', agent_id: 'antigravity', successes: 25, failures: 2, total: 27, rate: 0.92 },
        ],
      },
      {
        agent_id: 'claude-code',
        role: 'lead',
        total_calls: 2150,
        first_seen: '2026-07-15T08:00:00Z',
        last_intent: 'Supervise Coloquio and arbitrate work contracts',
        identity_node: 'agent_identity_claude-code',
        competencies: JSON.stringify({
          coloquio: 0.99,
          code: 0.89,
          research: 0.95,
        }),
        domains: [
          { domain: 'coloquio', agent_id: 'claude-code', successes: 120, failures: 1, total: 121, rate: 0.99 },
        ],
      },
    ],
  };

  let mockBridge: { fetchRaw: ReturnType<typeof vi.fn> };
  let mockNotify: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockNotify = vi.fn();
    mockBridge = {
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url.startsWith('/api/v1/agents')) {
          return Promise.resolve(mockAgentsData);
        }
        return Promise.reject(new Error(`Unhandled URL: ${url}`));
      }),
    };
  });

  it('helper parseCompetencies correctly handles object, string, and invalid inputs', () => {
    expect(parseCompetencies({ bash: 0.9, fs: '0.8' })).toEqual({ bash: 0.9, fs: 0.8 });
    expect(parseCompetencies('{"bash": 0.95, "code": 0.85}')).toEqual({ bash: 0.95, code: 0.85 });
    expect(parseCompetencies(null)).toEqual({});
    expect(parseCompetencies('invalid json')).toEqual({});
    expect(parseCompetencies(12345)).toEqual({});
  });

  it('renders header, metric cards, and agent competency cards from bridge', async () => {
    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    expect(screen.getByText('Agent Competency & Cognitive Domains')).toBeDefined();
    expect(screen.getByText('ADR-014 Scheduler')).toBeDefined();

    await waitFor(() => {
      expect(screen.getAllByText('@deep').length).toBeGreaterThan(0);
      expect(screen.getAllByText('@antigravity').length).toBeGreaterThan(0);
      expect(screen.getAllByText('@claude-code').length).toBeGreaterThan(0);
    });

    // Metric cards
    expect(screen.getByText('Registered Agents')).toBeDefined();
    expect(screen.getByText('Specialist Agents')).toBeDefined();
    expect(screen.getByText('Cognitive Domains')).toBeDefined();
    expect(screen.getByText('Total Task Calls')).toBeDefined();
  });

  it('displays domain outcomes and competency scores with correct progress bars', async () => {
    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('@deep').length).toBeGreaterThan(0);
      expect(screen.getAllByText('96%').length).toBeGreaterThan(0);
      expect(screen.getAllByText('98%').length).toBeGreaterThan(0);
    });
  });

  it('filters agents by search query text', async () => {
    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('@deep').length).toBeGreaterThan(0);
    });

    const searchInput = screen.getByPlaceholderText('Filter by agent id, role, or intent...');
    fireEvent.change(searchInput, { target: { value: 'antigravity' } });

    await waitFor(() => {
      expect(screen.getAllByText('@antigravity').length).toBeGreaterThan(0);
      expect(screen.queryByText('@deep')).toBeNull();
    });
  });

  it('filters agents by domain selector', async () => {
    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('@deep').length).toBeGreaterThan(0);
    });

    const domainSelect = screen.getByLabelText('Filter by Domain');
    fireEvent.change(domainSelect, { target: { value: 'bash' } });

    await waitFor(() => {
      expect(screen.getAllByText('@deep').length).toBeGreaterThan(0);
      expect(screen.queryByText('@antigravity')).toBeNull();
    });
  });

  it('switches to Domain Matrix view and displays ranked agents', async () => {
    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getByText('Domain Matrix')).toBeDefined();
    });

    const matrixBtn = screen.getByText('Domain Matrix');
    fireEvent.click(matrixBtn);

    await waitFor(() => {
      expect(screen.getAllByText(/Top: @/i).length).toBeGreaterThan(0);
    });
  });

  it('opens and closes agent detail inspector modal', async () => {
    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('Inspect').length).toBeGreaterThan(0);
    });

    const inspectButtons = screen.getAllByText('Inspect');
    fireEvent.click(inspectButtons[0]);

    await waitFor(() => {
      expect(screen.getByText('Granular Domain Outcomes (ADR-014)')).toBeDefined();
      expect(screen.getByText('Raw Profile Data')).toBeDefined();
    });

    const closeBtn = screen.getByRole('button', { name: 'Close modal' });
    fireEvent.click(closeBtn);

    await waitFor(() => {
      expect(screen.queryByText('Granular Domain Outcomes (ADR-014)')).toBeNull();
    });
  });

  it('handles empty state and error response gracefully', async () => {
    mockBridge.fetchRaw.mockImplementation((url: string) => {
      if (url.startsWith('/api/v1/agents')) {
        return Promise.reject(new Error('Network error 500'));
      }
      return Promise.reject(new Error(`Unhandled URL: ${url}`));
    });

    render(<AgentCompetencyPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getByText(/Error loading agent competencies/i)).toBeDefined();
      expect(screen.getByText('No agents matched the selected criteria.')).toBeDefined();
    });
  });
});

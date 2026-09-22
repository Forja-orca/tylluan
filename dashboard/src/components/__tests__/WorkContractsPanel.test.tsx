import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import { WorkContractsPanel } from '../WorkContractsPanel';

describe('WorkContractsPanel', () => {
  const mockActiveContract = {
    id: 'bwc-4e091c09-5271-48c3-8b4f-14645cbf7d23',
    task: 'Investigacion de convergencia: como resolver el bloqueo real del event loop.',
    budget: 12,
    budget_remaining: 10,
    team: ['deep', 'buffy', 'antigravity', 'claude-code'],
    consolidator: 'claude-code',
    channel_id: 'general',
    status: 'in_progress',
    created_at: 1789916067,
    deliveries: [
      {
        agent_id: 'deep',
        artifact: 'docs/investigation/event_loop_concurrency.md',
        ts: 1789916100,
      },
    ],
    votes: [
      {
        agent_id: 'claude-code',
        vote: 'approve',
        cycles: 1,
      },
    ],
    extensions: 0,
  };

  const mockPastContract = {
    id: 'bwc-12345678-abcd-1111',
    task: 'ADR-015: Stigmergic Fleet Coordination specification and review.',
    budget: 15,
    budget_remaining: 15,
    team: ['antigravity', 'claude-code'],
    consolidator: 'claude-code',
    channel_id: 'general',
    status: 'done',
    created_at: 1789900000,
    deliveries: [
      {
        agent_id: 'antigravity',
        artifact: 'docs/reference/adr/ADR015_stigmergic_fleet_coordination.md',
        ts: 1789905000,
      },
    ],
    votes: [],
  };

  let mockBridge: { fetchRaw: ReturnType<typeof vi.fn> };
  let mockNotify: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockNotify = vi.fn();
    mockBridge = {
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url.startsWith('/api/v1/work-contracts/active')) {
          return Promise.resolve({
            contract_id: mockActiveContract.id,
            budget_remaining: mockActiveContract.budget_remaining,
            status: mockActiveContract.status,
          });
        }
        if (url.startsWith(`/api/v1/work-contracts/${mockActiveContract.id}`)) {
          return Promise.resolve(mockActiveContract);
        }
        if (url.startsWith(`/api/v1/work-contracts/${mockPastContract.id}`)) {
          return Promise.resolve(mockPastContract);
        }
        if (url.startsWith('/api/v1/coloquio/channels/general')) {
          return Promise.resolve({
            messages: [
              {
                content: `Referencing contract ${mockPastContract.id} in this thread`,
              },
            ],
          });
        }
        return Promise.reject(new Error(`Unhandled URL: ${url}`));
      }),
    };
  });

  it('renders header, metric cards, and active contract details', async () => {
    render(<WorkContractsPanel bridge={mockBridge} notify={mockNotify} />);

    expect(screen.getByText('Bounded Work Contracts (BWC)')).toBeDefined();
    expect(screen.getByText('Finite Protocol · BWC-1..4')).toBeDefined();

    await waitFor(() => {
      expect(screen.getAllByText(new RegExp(mockActiveContract.id.slice(0, 8), 'i')).length).toBeGreaterThan(0);
      expect(screen.getAllByText(/Investigacion de convergencia/i).length).toBeGreaterThan(0);
    });

    // Metric cards
    expect(screen.getByText('Active Contract')).toBeDefined();
    expect(screen.getByText('Budget Remaining')).toBeDefined();
    expect(screen.getByText('Team Members')).toBeDefined();
    expect(screen.getByText('Deliveries Tracked')).toBeDefined();
  });

  it('displays team members and assigned consolidator', async () => {
    render(<WorkContractsPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('@deep').length).toBeGreaterThan(0);
      expect(screen.getAllByText('@claude-code').length).toBeGreaterThan(0);
      expect(screen.getByText('CONSOLIDATOR')).toBeDefined();
    });
  });

  it('filters contracts by search text', async () => {
    render(<WorkContractsPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText(/Investigacion de convergencia/i).length).toBeGreaterThan(0);
    });

    const searchInput = screen.getByPlaceholderText('Filter contracts by task, id or agent...');
    fireEvent.change(searchInput, { target: { value: 'ADR-015' } });

    await waitFor(() => {
      expect(screen.getAllByText(/ADR-015/i).length).toBeGreaterThan(0);
    });
  });

  it('opens and closes contract detail inspector modal', async () => {
    render(<WorkContractsPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText(/Investigacion de convergencia/i).length).toBeGreaterThan(0);
    });

    const inspectBtn = screen.getByText('Inspect Details & Deliverables');
    fireEvent.click(inspectBtn);

    await waitFor(() => {
      expect(screen.getByText('Contract Specification')).toBeDefined();
      expect(screen.getByText('Registered Deliverables (1)')).toBeDefined();
    });

    const closeBtn = screen.getByRole('button', { name: 'Close' });
    fireEvent.click(closeBtn);

    await waitFor(() => {
      expect(screen.queryByText('Contract Specification')).toBeNull();
    });
  });

  it('renders fallback when no active contract exists', async () => {
    mockBridge.fetchRaw.mockImplementation((url: string) => {
      if (url.startsWith('/api/v1/work-contracts/active')) {
        return Promise.reject(new Error('404 not found'));
      }
      if (url.startsWith('/api/v1/coloquio/channels/general')) {
        return Promise.resolve({ messages: [] });
      }
      return Promise.reject(new Error(`Unhandled URL: ${url}`));
    });

    render(<WorkContractsPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getByText('No Active Work Contract in #general')).toBeDefined();
    });
  });
});

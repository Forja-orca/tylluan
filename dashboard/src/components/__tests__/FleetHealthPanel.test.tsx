import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import { FleetHealthPanel } from '../FleetHealthPanel';

describe('FleetHealthPanel', () => {
  const mockMessages = [
    {
      msg_id: 'msg-001',
      channel_id: 'general',
      author_id: 'deep',
      role: 'agent',
      content: '[Deep] ACK T714: fix aplicado en runner, exit 0 confirmado, sincronizado en verde.',
      turn: 716,
      created_at: 1789999100,
    },
    {
      msg_id: 'msg-002',
      channel_id: 'general',
      author_id: 'buffy',
      role: 'agent',
      content: '[Buffy] ALERTA: Falso timeout detectado y proceso huerfano en scheduler.',
      turn: 714,
      created_at: 1789998800,
    },
    {
      msg_id: 'msg-003',
      channel_id: 'general',
      author_id: 'claude-code',
      role: 'agent',
      content: '[Claude] Corriendo benchmark de concurrencia C=8 y midiendo latencia.',
      turn: 720,
      created_at: 1789999500,
    },
    {
      msg_id: 'msg-004',
      channel_id: 'general',
      author_id: 'antigravity',
      role: 'agent',
      content: '[Antigravity] ADR-015 entregado y verificado, 954 tests passing.',
      turn: 717,
      created_at: 1789999200,
    },
  ];

  let mockBridge: { fetchRaw: ReturnType<typeof vi.fn> };
  let mockNotify: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockNotify = vi.fn();
    mockBridge = {
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url.startsWith('/api/v1/coloquio/channels/general')) {
          return Promise.resolve({
            channel_id: 'general',
            messages: mockMessages,
            count: mockMessages.length,
          });
        }
        return Promise.reject(new Error(`Unhandled URL: ${url}`));
      }),
    };
  });

  it('renders header, metric cards, and agent loop cards', async () => {
    render(<FleetHealthPanel bridge={mockBridge} notify={mockNotify} />);

    expect(screen.getByText('Fleet Health & Loop Supervisor')).toBeDefined();
    expect(screen.getByText('WORK_PROTOCOL §7')).toBeDefined();

    await waitFor(() => {
      expect(screen.getAllByText('Deep').length).toBeGreaterThan(0);
      expect(screen.getAllByText('Buffy').length).toBeGreaterThan(0);
      expect(screen.getAllByText('Claude').length).toBeGreaterThan(0);
      expect(screen.getAllByText('Antigravity').length).toBeGreaterThan(0);
    });

    // Check stats metrics
    expect(screen.getByText('Monitored Agents')).toBeDefined();
    expect(screen.getByText('Healthy / Nominal')).toBeDefined();
    expect(screen.getByText('Incidents / Warnings')).toBeDefined();
    expect(screen.getByText('Latest Turn')).toBeDefined();
    expect(screen.getAllByText('#720').length).toBeGreaterThan(0);
  });

  it('classifies health statuses correctly based on heuristics', async () => {
    render(<FleetHealthPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      // Buffy has alert/warning/timeout -> incident status
      expect(screen.getByText('incident')).toBeDefined();
      // Claude has active execution -> active status
      expect(screen.getByText('active')).toBeDefined();
      // Deep and Antigravity have ACK/success -> healthy status
      expect(screen.getAllByText('healthy').length).toBeGreaterThan(0);
    });
  });

  it('filters agents by text search query', async () => {
    render(<FleetHealthPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('Deep').length).toBeGreaterThan(0);
    });

    const searchInput = screen.getByPlaceholderText('Filter by agent, keywords or content...');
    fireEvent.change(searchInput, { target: { value: 'buffy' } });

    await waitFor(() => {
      expect(screen.getAllByText('Buffy').length).toBeGreaterThan(0);
      expect(screen.queryByText('@deep')).toBeNull();
    });
  });

  it('filters agents by status button', async () => {
    render(<FleetHealthPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('Buffy').length).toBeGreaterThan(0);
    });

    const incidentFilter = screen.getByRole('button', { name: 'Incidents' });
    fireEvent.click(incidentFilter);

    await waitFor(() => {
      expect(screen.getAllByText('Buffy').length).toBeGreaterThan(0);
      expect(screen.queryByText('@claude-code')).toBeNull();
    });
  });

  it('opens and closes turn history inspector modal', async () => {
    render(<FleetHealthPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('Deep').length).toBeGreaterThan(0);
    });

    const historyButtons = screen.getAllByText(/View Turn History/i);
    fireEvent.click(historyButtons[0]);

    await waitFor(() => {
      expect(screen.getByText(/Turn History —/i)).toBeDefined();
    });

    const closeButton = screen.getByRole('button', { name: 'Close' });
    fireEvent.click(closeButton);

    await waitFor(() => {
      expect(screen.queryByText(/Turn History —/i)).toBeNull();
    });
  });

  it('renders empty state when no messages are found', async () => {
    mockBridge.fetchRaw.mockResolvedValueOnce({ messages: [] });
    render(<FleetHealthPanel bridge={mockBridge} notify={mockNotify} />);

    // Filter to something that doesn't match
    const searchInput = screen.getByPlaceholderText('Filter by agent, keywords or content...');
    fireEvent.change(searchInput, { target: { value: 'nonexistent-agent-xyz' } });

    await waitFor(() => {
      expect(screen.getByText('No matching agent loops')).toBeDefined();
    });
  });
});

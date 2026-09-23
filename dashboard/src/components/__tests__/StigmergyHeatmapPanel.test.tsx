import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import {
  StigmergyHeatmapPanel,
  getHeatCategory,
  formatTimeAgo,
} from '../StigmergyHeatmapPanel';

describe('StigmergyHeatmapPanel', () => {
  const mockApiZonesData = {
    count: 3,
    half_life_hours: 4.0,
    window_hours: 16,
    zones: [
      {
        zone_id: 'crates/tylluan-kernel/transport',
        subsystem: 'kernel',
        description: 'Sovereign MCP transport handlers, SSE event loop, HTTP router & rate limiter',
        heat: 1.85,
        half_life_hours: 4.0,
        active_agents: ['deep', 'claude-code'],
        total_traces: 42,
        last_touched_at: Math.floor(Date.now() / 1000) - 120,
        contention_risk: 'high',
        neighbor_zones: ['crates/tylluan-kernel/router', 'crates/tylluan-link/p2p'],
        traces: [
          {
            trace_id: 't-101',
            agent_id: 'deep',
            trace_type: 'write',
            weight: 1.0,
            touched_at: Math.floor(Date.now() / 1000) - 120,
            note: 'Noise XK transport handshake refactor',
          },
        ],
      },
      {
        zone_id: 'dashboard/src/components',
        subsystem: 'dashboard',
        description: 'React dashboard UI, consolidated tab suites, metric primitives, and observability panels',
        heat: 1.62,
        half_life_hours: 4.0,
        active_agents: ['antigravity', 'claude-code'],
        total_traces: 36,
        last_touched_at: Math.floor(Date.now() / 1000) - 60,
        contention_risk: 'low',
        neighbor_zones: ['dashboard/src/hooks', 'packages/tylluan-ui-core'],
        traces: [
          {
            trace_id: 't-102',
            agent_id: 'antigravity',
            trace_type: 'write',
            weight: 1.0,
            touched_at: Math.floor(Date.now() / 1000) - 60,
            note: 'StigmergyHeatmapPanel live kernel connection',
          },
        ],
      },
      {
        zone_id: 'guilds/core',
        subsystem: 'guilds',
        description: 'Python ecosystem tools, vision moondream, check_coloquio, and worker coordinators',
        heat: 0.38,
        half_life_hours: 4.0,
        active_agents: ['deep'],
        total_traces: 8,
        last_touched_at: Math.floor(Date.now() / 1000) - 14400,
        contention_risk: 'low',
        neighbor_zones: ['guilds/vision'],
        traces: [],
      },
    ],
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders header banner and KPI metrics with reference data', () => {
    render(<StigmergyHeatmapPanel />);

    expect(screen.getByText(/Stigmergic Heatmap & Workprints/i)).toBeInTheDocument();
    expect(screen.getByText(/Tracked Work Zones/i)).toBeInTheDocument();
    expect(screen.getByText(/Hot \/ Active Focus/i)).toBeInTheDocument();
    expect(screen.getByText(/Steady \/ Warm Zones/i)).toBeInTheDocument();
    expect(screen.getByText(/Quiescent \/ Cold/i)).toBeInTheDocument();

    // Verify default zones exist
    expect(screen.getByText('crates/tylluan-kernel/transport')).toBeInTheDocument();
    expect(screen.getByText('dashboard/src/components')).toBeInTheDocument();
  });

  it('filters zones by subsystem select dropdown', () => {
    render(<StigmergyHeatmapPanel />);

    const subsystemSelect = screen.getByDisplayValue('All Subsystems');
    fireEvent.change(subsystemSelect, { target: { value: 'dashboard' } });

    expect(screen.getByText('dashboard/src/components')).toBeInTheDocument();
    expect(screen.queryByText('crates/tylluan-kernel/transport')).not.toBeInTheDocument();
  });

  it('filters zones by heat category dropdown', () => {
    render(<StigmergyHeatmapPanel />);

    const categorySelect = screen.getByDisplayValue('All Heat Levels');
    fireEvent.change(categorySelect, { target: { value: 'blazing' } });

    expect(screen.getByText('crates/tylluan-kernel/transport')).toBeInTheDocument();
    expect(screen.getByText('dashboard/src/components')).toBeInTheDocument();
    expect(screen.queryByText('guilds/core')).not.toBeInTheDocument();
  });

  it('filters zones by text search query', () => {
    render(<StigmergyHeatmapPanel />);

    const searchInput = screen.getByPlaceholderText(/Search by zone path/i);
    fireEvent.change(searchInput, { target: { value: 'adr' } });

    expect(screen.getByText('docs/reference/adr')).toBeInTheDocument();
    expect(screen.queryByText('crates/tylluan-kernel/transport')).not.toBeInTheDocument();
  });

  it('opens and closes zone inspector modal on card click', () => {
    render(<StigmergyHeatmapPanel />);

    const card = screen.getByText('crates/tylluan-kernel/transport');
    fireEvent.click(card);

    // Modal should be open
    expect(screen.getByText(/Recent Workprint Trail \(ADR-015\)/i)).toBeInTheDocument();
    expect(screen.getByText(/Noise XK transport handshake refactor/i)).toBeInTheDocument();
    expect(screen.getByText(/1-Hop Diffusion Neighbors/i)).toBeInTheDocument();

    // Close modal
    const closeBtn = screen.getByRole('button', { name: /close modal/i });
    fireEvent.click(closeBtn);

    expect(screen.queryByText(/Recent Workprint Trail \(ADR-015\)/i)).not.toBeInTheDocument();
  });

  it('fetches and displays live zones from bridge when available', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue(mockApiZonesData),
    };

    render(<StigmergyHeatmapPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(mockBridge.fetchRaw).toHaveBeenCalledWith('/api/v1/stigmergy/zones');
    });

    // Live badge should appear
    expect(screen.getByText(/Live Kernel Stream/i)).toBeInTheDocument();
    expect(screen.getByText('dashboard/src/components')).toBeInTheDocument();

    // Click zone card to open inspector modal and verify workprint trace note
    fireEvent.click(screen.getByText('dashboard/src/components'));
    expect(screen.getByText('StigmergyHeatmapPanel live kernel connection')).toBeInTheDocument();
  });

  it('computes correct heat categories via getHeatCategory helper', () => {
    expect(getHeatCategory(2.0)).toBe('blazing');
    expect(getHeatCategory(1.5)).toBe('blazing');
    expect(getHeatCategory(1.2)).toBe('hot');
    expect(getHeatCategory(1.0)).toBe('hot');
    expect(getHeatCategory(0.8)).toBe('warm');
    expect(getHeatCategory(0.5)).toBe('warm');
    expect(getHeatCategory(0.3)).toBe('cool');
    expect(getHeatCategory(0.2)).toBe('cool');
    expect(getHeatCategory(0.1)).toBe('cold');
    expect(getHeatCategory(0.0)).toBe('cold');
  });

  it('formats time elapsed correctly via formatTimeAgo helper', () => {
    const now = Math.floor(Date.now() / 1000);
    expect(formatTimeAgo(0)).toBe('never');
    expect(formatTimeAgo(now - 10)).toBe('10s ago');
    expect(formatTimeAgo(now - 180)).toBe('3m ago');
    expect(formatTimeAgo(now - 7200)).toBe('2h ago');
    expect(formatTimeAgo(now - 172800)).toBe('2d ago');
  });
});

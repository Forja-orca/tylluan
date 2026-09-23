import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import {
  SchedulerConfusionPanel,
  computeDerivedMetrics,
  SchedulerConfusionData,
} from '../SchedulerConfusionPanel';

describe('SchedulerConfusionPanel', () => {
  const mockApiConfusionData: SchedulerConfusionData = {
    total: 100,
    agrees: 80,
    differ: 20,
    differ_with_cascade_fired: 8,
    by_guild: {
      bash: { Agrees: 50, Differ: 5 },
      browser: { Agrees: 10, Differ: 10 },
      silva: { Agrees: 20, Differ: 5 },
    },
    note: 'observation-only tallies; cutover is a separate Tech Lead decision',
  };

  const emptyConfusionData: SchedulerConfusionData = {
    total: 0,
    agrees: 0,
    differ: 0,
    differ_with_cascade_fired: 0,
    by_guild: {},
    note: 'store not created yet',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders header banner and metric cards with reference data', () => {
    render(<SchedulerConfusionPanel />);

    expect(screen.getByText(/Cognitive Scheduler Confusion Matrix/i)).toBeInTheDocument();
    expect(screen.getByText(/Decisions Observed/i)).toBeInTheDocument();
    expect(screen.getByText(/Concordance Rate/i)).toBeInTheDocument();
    expect(screen.getByText(/Confusion Rate/i)).toBeInTheDocument();
    expect(screen.getByText(/Cascade Fired on Differ/i)).toBeInTheDocument();
  });

  it('fetches and displays live confusion data from bridge', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue(mockApiConfusionData),
    };

    render(<SchedulerConfusionPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(mockBridge.fetchRaw).toHaveBeenCalledWith('/api/v1/scheduler/confusion');
    });

    expect(screen.getByText(/Live Kernel Stream/i)).toBeInTheDocument();
    expect(screen.getByText('80.0%')).toBeInTheDocument(); // Concordance
    expect(screen.getByText('20.0%')).toBeInTheDocument(); // Differ rate
    expect(screen.getByText('40.0%')).toBeInTheDocument(); // Cascade fired rate (8/20 = 40%)
    expect(screen.getByText('bash')).toBeInTheDocument();
    expect(screen.getByText('browser')).toBeInTheDocument();
  });

  it('handles empty / degraded store with informative empty state banner', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue(emptyConfusionData),
    };

    render(<SchedulerConfusionPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(mockBridge.fetchRaw).toHaveBeenCalledWith('/api/v1/scheduler/confusion');
    });

    expect(screen.getByText(/No Scheduler Confusion Observations Recorded Yet/i)).toBeInTheDocument();
    expect(screen.getByText(/store not created yet/i)).toBeInTheDocument();
  });

  it('computes derived metrics accurately via computeDerivedMetrics helper', () => {
    const metrics = computeDerivedMetrics(mockApiConfusionData);

    expect(metrics.total).toBe(100);
    expect(metrics.agrees).toBe(80);
    expect(metrics.differ).toBe(20);
    expect(metrics.differ_with_cascade_fired).toBe(8);
    expect(metrics.agreement_rate).toBeCloseTo(80.0);
    expect(metrics.disagreement_rate).toBeCloseTo(20.0);
    expect(metrics.cascade_fired_rate_on_differ).toBeCloseTo(40.0);

    const bashGuild = metrics.guildMetrics.find(g => g.guild === 'bash');
    expect(bashGuild).toBeDefined();
    expect(bashGuild?.total).toBe(55);
    expect(bashGuild?.agrees).toBe(50);
    expect(bashGuild?.differ).toBe(5);
    expect(bashGuild?.agreement_rate).toBeCloseTo((50 / 55) * 100);
    expect(bashGuild?.disagreement_rate).toBeCloseTo((5 / 55) * 100);

    // Empty dataset handling
    const emptyMetrics = computeDerivedMetrics(emptyConfusionData);
    expect(emptyMetrics.total).toBe(0);
    expect(emptyMetrics.agreement_rate).toBe(100);
    expect(emptyMetrics.disagreement_rate).toBe(0);
    expect(emptyMetrics.cascade_fired_rate_on_differ).toBe(0);
    expect(emptyMetrics.guildMetrics).toEqual([]);
  });

  it('filters guilds by text search input', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue(mockApiConfusionData),
    };

    render(<SchedulerConfusionPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('bash')).toBeInTheDocument();
    });

    const searchInput = screen.getByPlaceholderText(/Search guild.../i);
    fireEvent.change(searchInput, { target: { value: 'browser' } });

    expect(screen.getByText('browser')).toBeInTheDocument();
    expect(screen.queryByText('bash')).not.toBeInTheDocument();
  });

  it('sorts guilds by different criteria via dropdown', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue(mockApiConfusionData),
    };

    render(<SchedulerConfusionPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('browser')).toBeInTheDocument();
    });

    const sortSelect = screen.getByDisplayValue(/Highest Disagreement/i);
    fireEvent.change(sortSelect, { target: { value: 'volume' } });

    // Both guilds still render
    expect(screen.getByText('bash')).toBeInTheDocument();
    expect(screen.getByText('browser')).toBeInTheDocument();
  });

  it('opens and closes guild inspector modal on guild card click', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue(mockApiConfusionData),
    };

    render(<SchedulerConfusionPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('bash')).toBeInTheDocument();
    });

    const guildCard = screen.getByText('bash');
    fireEvent.click(guildCard);

    // Modal opens
    expect(screen.getByText(/Guild: bash/i)).toBeInTheDocument();
    expect(screen.getByText(/Observation Insights/i)).toBeInTheDocument();

    // Close modal via close button
    const closeBtn = screen.getByRole('button', { name: /close modal/i });
    fireEvent.click(closeBtn);

    expect(screen.queryByText(/Guild: bash/i)).not.toBeInTheDocument();
  });
});

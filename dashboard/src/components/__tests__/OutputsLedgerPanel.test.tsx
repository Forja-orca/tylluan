import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import { OutputsLedgerPanel } from '../OutputsLedgerPanel';

describe('OutputsLedgerPanel', () => {
  const mockRuns = [
    {
      guild: 'comfy',
      run_id: 'run-12345678-abcd',
      created_at: 1789999000,
      requested_by: 'jose',
      tool: 'generate_image',
      files_count: 2,
      total_bytes: 2048000,
      delivery_status: 'ok',
    },
    {
      guild: 'data_tools',
      run_id: 'run-87654321-efgh',
      created_at: 1789998000,
      requested_by: 'claude-code',
      tool: 'export_csv',
      files_count: 1,
      total_bytes: 512,
      delivery_status: 'partial',
    },
  ];

  const mockManifest = {
    schema_version: 1,
    guild: 'comfy',
    tool: 'generate_image',
    requested_by: 'jose',
    run_id: 'run-12345678-abcd',
    created_at: 1789999000,
    call_success: true,
    delivery_status: 'ok',
    files: [
      {
        path: 'data/outputs/comfy/output_01.png',
        bytes: 1024000,
        sha256: 'a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0',
      },
      {
        path: 'data/outputs/comfy/output_02.png',
        bytes: 1024000,
        sha256: 'b2c3d4e5f6a10718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0',
      },
    ],
    claimed_outputs_verified: 2,
    window_scan_started_unix: 1789998995,
  };

  let mockBridge: any;
  let mockNotify: any;

  beforeEach(() => {
    mockNotify = vi.fn();
    mockBridge = {
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url.startsWith('/api/v1/outputs/run-12345678-abcd/manifest')) {
          return Promise.resolve(mockManifest);
        }
        if (url.startsWith('/api/v1/outputs')) {
          return Promise.resolve({ runs: mockRuns });
        }
        return Promise.reject(new Error(`Unhandled URL: ${url}`));
      }),
    };
  });

  it('renders header, stats, and run items from bridge', async () => {
    render(<OutputsLedgerPanel bridge={mockBridge} notify={mockNotify} />);

    expect(screen.getByText('Outputs Ledger')).toBeDefined();
    expect(screen.getByText('bwc-d0fb0812')).toBeDefined();

    await waitFor(() => {
      expect(screen.getAllByText('comfy').length).toBeGreaterThan(0);
      expect(screen.getAllByText('data_tools').length).toBeGreaterThan(0);
    });

    // Check stats
    expect(screen.getByText('Indexed Runs')).toBeDefined();
    expect(screen.getByText('Total Artifacts')).toBeDefined();
    expect(screen.getByText('Aggregated Size')).toBeDefined();
  });

  it('renders empty state when no runs exist', async () => {
    mockBridge.fetchRaw.mockResolvedValueOnce({ runs: [] });
    render(<OutputsLedgerPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getByText('No output runs found')).toBeDefined();
    });
  });

  it('opens manifest inspector modal and displays files and sha256 checksums', async () => {
    render(<OutputsLedgerPanel bridge={mockBridge} notify={mockNotify} />);

    await waitFor(() => {
      expect(screen.getAllByText('comfy').length).toBeGreaterThan(0);
    });

    const inspectButtons = screen.getAllByText('Inspect');
    fireEvent.click(inspectButtons[0]);

    await waitFor(() => {
      expect(screen.getByText('Manifest Inspector')).toBeDefined();
      expect(screen.getByText('data/outputs/comfy/output_01.png')).toBeDefined();
      expect(screen.getByText('data/outputs/comfy/output_02.png')).toBeDefined();
    });
  });
});


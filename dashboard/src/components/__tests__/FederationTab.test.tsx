import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import React from 'react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { FederationTab } from '../FederationTab';

vi.mock('../../hooks/usePolling', () => ({
  usePolling: () => undefined,
}));

describe('FederationTab — W1 Mesh & Federation Read-Only Wiring', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders live Mesh peers and Federation nodes on 200 OK', async () => {
    const fetchRawMock = vi.fn().mockImplementation((url: string) => {
      if (url === '/api/v1/mesh/peers') {
        return Promise.resolve({
          enabled: true,
          peer_count: 2,
          buckets: 4,
          peers: [
            {
              node_id: 'peer-alpha-1234567890abcdef',
              addr: '192.168.1.50:47004',
              capabilities: ['inference', 'storage'],
              last_seen: Date.now() - 5000,
            },
          ],
        });
      }
      if (url === '/api/v1/federation/nodes') {
        return Promise.resolve({
          source: 'local',
          count: 1,
          nodes: [
            {
              id: 'silva-mem-test-999',
              node_type: 'episodic',
              content: 'Autonomous cognitive memory node in SilvaDB',
              weight: 0.8524,
              shareable: true,
              topic_key: null,
            },
          ],
        });
      }
      if (url === '/api/v1/federation/peers') {
        return Promise.resolve([]);
      }
      if (url === '/api/v1/federation/sharing') {
        return Promise.resolve({
          enabled: true,
          shareable_nodes_count: 1,
          min_weight: 0.5,
          min_activity_hours: 24,
          node_types: ['episodic', 'semantic'],
        });
      }
      return Promise.resolve({});
    });

    const mockBridge = {
      fetchRaw: fetchRawMock,
      listPeers: vi.fn().mockResolvedValue([]),
      listFederationPeers: vi.fn().mockResolvedValue([]),
      getFederationStatus: vi.fn().mockResolvedValue({ enabled: true, peers_count: 1 }),
    } as unknown as NexusBridge;

    render(<FederationTab bridge={mockBridge} notify={vi.fn()} />);

    // Verify Mesh DHT Peer Discovery header and bucket stats
    await waitFor(() => {
      expect(screen.getByText(/Mesh DHT Peer Discovery/i)).toBeDefined();
      expect(screen.getByText(/\(buckets: 4, total: 2\)/i)).toBeDefined();
    });

    // Verify Mesh peer row rendered
    expect(screen.getByText('peer-alpha-1234567890abcdef')).toBeDefined();
    expect(screen.getByText('192.168.1.50:47004')).toBeDefined();
    expect(screen.getByText('inference')).toBeDefined();
    expect(screen.getByText('storage')).toBeDefined();

    // Verify Federation nodes header, source and content
    expect(screen.getByText(/SilvaDB Shareable Nodes Control \(1\)/i)).toBeDefined();
    expect(screen.getByText(/\(source: local\)/i)).toBeDefined();
    expect(screen.getByText('Autonomous cognitive memory node in SilvaDB')).toBeDefined();
    expect(screen.getByText('0.852')).toBeDefined();
    expect(screen.getByText('Federated / Shareable')).toBeDefined();
  });

  it('renders clear operator clearance states (HTTP 403) with accessible retry buttons', async () => {
    const fetchRawMock = vi.fn().mockImplementation((url: string) => {
      if (url === '/api/v1/mesh/peers') {
        return Promise.reject(new Error('HTTP 403 Forbidden'));
      }
      if (url === '/api/v1/federation/nodes') {
        return Promise.reject(new Error('HTTP 403 Forbidden: Operator credential required'));
      }
      if (url === '/api/v1/federation/peers') return Promise.resolve([]);
      if (url === '/api/v1/federation/sharing') return Promise.resolve(null);
      return Promise.resolve({});
    });

    const mockBridge = {
      fetchRaw: fetchRawMock,
      listPeers: vi.fn().mockResolvedValue([]),
      listFederationPeers: vi.fn().mockResolvedValue([]),
      getFederationStatus: vi.fn().mockResolvedValue({ enabled: false, peers_count: 0 }),
    } as unknown as NexusBridge;

    render(<FederationTab bridge={mockBridge} notify={vi.fn()} />);

    await waitFor(() => {
      // Both Mesh and Federation nodes sections should render the operator clearance message
      const clearanceAlerts = screen.getAllByText(/Requiere credencial de operador \(HTTP 403\)/i);
      expect(clearanceAlerts.length).toBeGreaterThanOrEqual(2);
    });

    // Check specific error messages
    expect(screen.getByText(/Token maestro requerido para inspeccionar DHT routing table/i)).toBeDefined();
    expect(screen.getByText(/Token maestro requerido para consultar nodos de federación/i)).toBeDefined();

    // Verify retry buttons exist and trigger refetch
    const retryButtons = screen.getAllByRole('button', { name: /Reintentar/i });
    expect(retryButtons.length).toBeGreaterThanOrEqual(2);

    fireEvent.click(retryButtons[0]);
    expect(fetchRawMock).toHaveBeenCalledWith(
      '/api/v1/mesh/peers',
      expect.objectContaining({ signal: expect.any(AbortSignal) })
    );
  });

  it('renders empty states when mesh peers and federation nodes are empty', async () => {
    const fetchRawMock = vi.fn().mockImplementation((url: string) => {
      if (url === '/api/v1/mesh/peers') {
        return Promise.resolve({
          enabled: true,
          peer_count: 0,
          buckets: 0,
          peers: [],
        });
      }
      if (url === '/api/v1/federation/nodes') {
        return Promise.resolve({
          source: 'local',
          count: 0,
          nodes: [],
        });
      }
      if (url === '/api/v1/federation/peers') return Promise.resolve([]);
      if (url === '/api/v1/federation/sharing') return Promise.resolve(null);
      return Promise.resolve({});
    });

    const mockBridge = {
      fetchRaw: fetchRawMock,
      listPeers: vi.fn().mockResolvedValue([]),
      listFederationPeers: vi.fn().mockResolvedValue([]),
      getFederationStatus: vi.fn().mockResolvedValue({ enabled: false, peers_count: 0 }),
    } as unknown as NexusBridge;

    render(<FederationTab bridge={mockBridge} notify={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/No active DHT mesh peers discovered in local routing table/i)).toBeDefined();
      expect(screen.getByText(/No nodes found in SilvaDB/i)).toBeDefined();
    });

    // Verify retry button in SilvaDB empty state
    const retryNodesBtn = screen.getByRole('button', { name: /^Reintentar$/i });
    expect(retryNodesBtn).toBeDefined();
    fireEvent.click(retryNodesBtn);
    expect(fetchRawMock).toHaveBeenCalledWith(
      '/api/v1/federation/nodes',
      expect.objectContaining({ signal: expect.any(AbortSignal) })
    );
  });

  it('passes AbortSignal to mesh and nodes queries and aborts signal on cancel', async () => {
    let capturedMeshSignal: AbortSignal | undefined;
    const fetchRawMock = vi.fn().mockImplementation((url: string, init?: RequestInit) => {
      if (url === '/api/v1/mesh/peers') {
        capturedMeshSignal = init?.signal as AbortSignal;
        return new Promise(() => {}); // pending
      }
      return Promise.resolve({});
    });

    const mockBridge = {
      fetchRaw: fetchRawMock,
      listPeers: vi.fn().mockResolvedValue([]),
      listFederationPeers: vi.fn().mockImplementation(() => new Promise(() => {})),
      getFederationStatus: vi.fn().mockResolvedValue({ enabled: false, peers_count: 0 }),
    } as unknown as NexusBridge;

    render(<FederationTab bridge={mockBridge} notify={vi.fn()} />);

    await waitFor(() => {
      expect(capturedMeshSignal).toBeDefined();
    });
    expect(capturedMeshSignal?.aborted).toBe(false);

    // Click Cancel button for mesh query
    const cancelBtn = screen.getByRole('button', { name: /^Cancel$/i });
    expect(cancelBtn).toBeDefined();
    fireEvent.click(cancelBtn);

    expect(capturedMeshSignal?.aborted).toBe(true);
    await waitFor(() => {
      expect(screen.getByText(/Consulta Mesh cancelada por el usuario/i)).toBeDefined();
    });
  });
});

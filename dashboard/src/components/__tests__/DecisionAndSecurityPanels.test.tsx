import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { DecisionServicePanel } from '../DecisionServicePanel';
import { ContradictionsPanel } from '../ContradictionsPanel';
import AuditTrailPanel from '../AuditTrailPanel';

describe('DecisionServicePanel (D1)', () => {
  it('renders epistemic warning banner, per-contract agreement summary, and shadow rows', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue({
        rows: [
          {
            id: 1,
            ts: '2026-10-18T05:00:00Z',
            contract: 'recall_gate',
            request_hash: 'a1b2c3d4e5f60123456789',
            rules_value: 'retrieve',
            model_value: 'retrieve',
            provider: 'http_json',
            latency_ms: 14,
            agreed: 1,
            mode: 'shadow',
            model_identity: 'laya-onnx',
          },
          {
            id: 2,
            ts: '2026-10-18T05:01:00Z',
            contract: 'conflict_verdict',
            request_hash: 'f6e5d4c3b2a19876543210',
            rules_value: 'compatible',
            model_value: 'contradiction',
            provider: 'http_json',
            latency_ms: 21,
            agreed: 0,
            mode: 'shadow',
            model_identity: 'laya-onnx',
          },
        ],
        summary: [
          { contract: 'recall_gate', n: 5, n_agreed: 4, n_disagreed: 0, n_null: 1 },
          { contract: 'conflict_verdict', n: 3, n_agreed: 1, n_disagreed: 2, n_null: 0 },
        ],
      }),
    } as unknown as NexusBridge;

    render(<DecisionServicePanel bridge={mockBridge} />);

    expect(
      screen.getByText(/Sin verdad humana: acuerdo con reglas no es precisión/i)
    ).toBeDefined();

    await waitFor(() => {
      expect(screen.getAllByText('recall_gate').length).toBeGreaterThan(0);
      expect(screen.getByText('AGREED')).toBeDefined();
      expect(screen.getByText('DISAGREED')).toBeDefined();
    });

    expect(screen.getByText('n=5')).toBeDefined();
    expect(screen.getByText('n=3')).toBeDefined();
  });

  it('shows clear operator clearance state when /api/v1/decide/shadow returns 403', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockRejectedValue(new Error('HTTP 403 Forbidden')),
    } as unknown as NexusBridge;

    render(<DecisionServicePanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(
        screen.getByText(/Requiere credencial de operador \(HTTP 403\)/i)
      ).toBeDefined();
    });
    expect(screen.getAllByRole('button', { name: /Reintentar/i }).length).toBeGreaterThan(0);
  });

  it('renders empty state with Reintentar button when shadow evaluations are empty', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      rows: [],
      summary: [],
      total: 0,
      limit: 50,
    });
    const mockBridge = {
      fetchRaw: fetchMock,
    } as unknown as NexusBridge;

    render(<DecisionServicePanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(
        screen.getByText(/No hay evaluaciones en sombra registradas aún/i)
      ).toBeDefined();
    });

    const retryBtn = screen.getByRole('button', { name: /Reintentar consulta de evaluaciones en sombra/i });
    expect(retryBtn).toBeDefined();
    fireEvent.click(retryBtn);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('gracefully handles 404 when /api/v1/decide/shadow is not mounted yet', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockRejectedValue(new Error('HTTP 404 Not Found')),
    } as unknown as NexusBridge;

    render(<DecisionServicePanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(
        screen.getByText(/GET \/api\/v1\/decide\/shadow no está montado aún en el kernel/i)
      ).toBeDefined();
    });
    expect(screen.getAllByText('write_relevance').length).toBeGreaterThan(0);
  });

  it('shows filter-specific empty state and reset button when total > 0 but 0 filtered rows match', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue({
        rows: [
          {
            id: 2116,
            ts: '2026-10-18T05:00:00Z',
            contract: '',
            request_hash: 'e4a909d1ea2338379ceb44ec2c811e95',
            rules_value: null,
            model_value: 'false',
            provider: 'systemone-http-v1',
            latency_ms: 51,
            agreed: null,
            mode: 'off',
            model_identity: 'decima-small',
          },
        ],
        summary: [
          { contract: '', n: 1, n_agreed: 0, n_disagreed: 0, n_null: 1 },
        ],
        total: 2116,
        limit: 50,
      }),
    } as unknown as NexusBridge;

    render(<DecisionServicePanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText(/Showing 1 of 1 rows \(total: 2116\)/i)).toBeDefined();
      expect(screen.getAllByText('(unspecified)').length).toBeGreaterThanOrEqual(1);
    });

    // Click recall_gate contract card to filter by a contract with 0 rows
    const recallCard = screen.getByRole('button', { name: /recall_gate/i });
    fireEvent.click(recallCard);

    // Header updates to 0 rows
    expect(screen.getByText(/Showing 0 of 1 rows \(total: 2116\)/i)).toBeDefined();

    // Table body shows clear filter empty state with total count, NOT the global empty-store message
    expect(
      screen.getByText(/No hay evaluaciones que coincidan con el contrato/i)
    ).toBeDefined();
    expect(
      screen.getByText(/2116 evaluaciones registradas en total/i)
    ).toBeDefined();
    expect(
      screen.queryByText(/No hay evaluaciones en sombra registradas aún/i)
    ).toBeNull();

    // Click button to reset filter
    const resetBtn = screen.getByRole('button', { name: /Mostrar todas las evaluaciones/i });
    expect(resetBtn).toBeDefined();
    fireEvent.click(resetBtn);

    // Rows restored
    await waitFor(() => {
      expect(screen.getByText(/Showing 1 of 1 rows \(total: 2116\)/i)).toBeDefined();
    });
  });
});

describe('ContradictionsPanel (D2)', () => {
  it('renders ~91% FP calibration warning and lists conflicted nodes from /api/v1/silva/conflicted (excluding deprecated_nodes)', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockResolvedValue({
        nodes: [
          {
            id: 'node-conf-1',
            node_type: 'Semantic',
            content_excerpt: 'Conflicted claim under structural audit (excerpt <= 300 chars)',
            conflicted_since: '2026-10-18T05:00:00Z',
          },
          {
            id: 'node-conf-2',
            node_type: 'Episodic',
            content_excerpt: 'Second conflicted node in SilvaDB',
            conflicted_since: null,
          },
        ],
        total_conflicted: 368,
        limit: 50,
      }),
    } as unknown as NexusBridge;

    render(<ContradictionsPanel bridge={mockBridge} />);

    expect(screen.getByText(/la detección actual es estructural; sin contradicciones reales en la muestra etiquetada a mano por Buffy/i)).toBeDefined();

    await waitFor(() => {
      expect(screen.getByText('node-conf-1')).toBeDefined();
      expect(screen.getByText('node-conf-2')).toBeDefined();
    });

    expect(screen.getByText(/Total conflicted: 368/i)).toBeDefined();

    const searchInput = screen.getByPlaceholderText(/Search by node ID, content excerpt, or node_type/i);
    fireEvent.change(searchInput, { target: { value: 'node-conf-2' } });

    await waitFor(() => {
      expect(screen.queryByText('node-conf-1')).toBeNull();
      expect(screen.getByText('node-conf-2')).toBeDefined();
    });
  });

  it('shows clear operator clearance state when /api/v1/silva/conflicted returns 403', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockRejectedValue(new Error('HTTP 403 Forbidden')),
    } as unknown as NexusBridge;

    render(<ContradictionsPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(
        screen.getAllByText(/Requiere credencial de operador \(HTTP 403\)/i).length
      ).toBeGreaterThan(0);
    });
    expect(screen.getAllByRole('button', { name: /Reintentar/i }).length).toBeGreaterThan(0);
  });

  it('renders empty state with Reintentar button when no conflicted nodes exist', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      nodes: [],
      total_conflicted: 0,
      limit: 50,
    });
    const mockBridge = {
      fetchRaw: fetchMock,
    } as unknown as NexusBridge;

    render(<ContradictionsPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText(/No se encontraron nodos en conflicto/i)).toBeDefined();
    });

    const retryBtn = screen.getByRole('button', { name: /Reintentar consulta de nodos en conflicto/i });
    expect(retryBtn).toBeDefined();
    fireEvent.click(retryBtn);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('shows honest pending state when GET /api/v1/silva/conflicted returns 404', async () => {
    const mockBridge = {
      fetchRaw: vi.fn().mockRejectedValue(new Error('HTTP 404 Not Found')),
    } as unknown as NexusBridge;

    render(<ContradictionsPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(
        screen.getByText(/GET \/api\/v1\/silva\/conflicted no está montado aún en el kernel/i)
      ).toBeDefined();
    });
  });
});

describe('AuditTrailPanel (D3)', () => {
  it('renders HMAC chain verification, dispatch latency percentiles, and security doctor status', async () => {
    const mockBridge = {
      getAuditTrail: vi.fn().mockResolvedValue({
        entries: [
          {
            agent_id: 'claude-code',
            guild: 'memory',
            intent_preview: 'recall architecture',
            allowed: true,
            timestamp: new Date().toISOString(),
          },
        ],
        total: 1,
      }),
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url === '/api/v1/audit/verify') {
          return Promise.resolve({
            ok: true,
            status: 'clean',
            valid_count: 128,
            tampered_count: 0,
            message: 'All 128 audit log entries passed HMAC verification.',
          });
        }
        if (url.startsWith('/api/v1/audit/latency')) {
          return Promise.resolve({
            available: true,
            window_minutes: 60,
            latency_ms: { p50: 12, p95: 45, p99: 88, max: 110, sampled: 42, zero_latency_excluded: 5 },
            error_rate: { total: 47, errors: 0, percent: 0 },
          });
        }
        if (url === '/api/v1/security/events') {
          return Promise.resolve({
            status: 'healthy',
            system: { cpu_usage_percent: 18.4, memory_percent: 42.1, warnings: [] },
            suggestions: [],
          });
        }
        if (url === '/api/v1/audit/logs') {
          return Promise.resolve({
            logs: [{ data: { message: 'INFO kernel audit verified' } }],
            count: 1,
          });
        }
        return Promise.resolve({});
      }),
    } as unknown as NexusBridge;

    render(<AuditTrailPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText(/All 128 audit log entries passed HMAC verification/i)).toBeDefined();
      expect(screen.getByText('CLEAN — VERIFIED')).toBeDefined();
      expect(screen.getByText('SAMPLED (42)')).toBeDefined();
      expect(screen.getByText('HEALTHY — NOMINAL')).toBeDefined();
      expect(screen.getByText('45 ms')).toBeDefined();
      expect(screen.getByText('@claude-code')).toBeDefined();
    });
  });

  it('renders tampered HMAC pill, 0-sample empty latency pill, and degraded security findings', async () => {
    const mockBridge = {
      getAuditTrail: vi.fn().mockResolvedValue({ entries: [], total: 0 }),
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url === '/api/v1/audit/verify') {
          return Promise.resolve({
            ok: true,
            status: 'tampered',
            valid_count: 10,
            tampered_count: 2,
            message: 'ALERT: 2 audit log entries failed HMAC verification (10 valid).',
          });
        }
        if (url.startsWith('/api/v1/audit/latency')) {
          return Promise.resolve({
            available: true,
            window_minutes: 60,
            latency_ms: { p50: null, p95: null, p99: null, max: null, sampled: 0, zero_latency_excluded: 3 },
            error_rate: { total: 3, errors: 0, percent: 0 },
          });
        }
        if (url === '/api/v1/security/events') {
          return Promise.resolve({
            status: 'degraded',
            config_valid: true,
            guilds: [{ name: 'browser', running: false, tools_count: 4, issues: ['Guild process exited'] }],
            system: { cpu_usage_percent: 88.2, memory_percent: 79.4, warnings: ['High CPU utilization'] },
            suggestions: ['Restart inactive guild'],
          });
        }
        return Promise.resolve({ logs: [] });
      }),
    } as unknown as NexusBridge;

    render(<AuditTrailPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('TAMPERED — BROKEN')).toBeDefined();
      expect(screen.getByText('EMPTY — 0 SAMPLES')).toBeDefined();
      expect(screen.getByText('DEGRADED — WARNING')).toBeDefined();
      expect(screen.getByText('[browser] Guild process exited')).toBeDefined();
    });
  });

  it('handles 403 Forbidden with operator clearance message and 403 pill', async () => {
    const mockBridge = {
      getAuditTrail: vi.fn().mockRejectedValue(new Error('HTTP 403 Forbidden')),
      fetchRaw: vi.fn().mockImplementation((url: string) => {
        if (url === '/api/v1/audit/verify') {
          return Promise.resolve({
            ok: false,
            error: 'HTTP 403 Forbidden: operator credentials required',
          });
        }
        if (url === '/api/v1/security/events') {
          return Promise.reject(new Error('HTTP 403 Forbidden'));
        }
        return Promise.resolve({});
      }),
    } as unknown as NexusBridge;

    render(<AuditTrailPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getAllByText(/Requiere credencial de operador \(HTTP 403\)/i).length).toBeGreaterThan(0);
      expect(screen.getAllByText('OPERADOR REQUERIDO (403)').length).toBeGreaterThan(0);
    });

    const retryBtns = screen.getAllByRole('button', { name: /Reintentar/i });
    expect(retryBtns.length).toBeGreaterThan(0);
  });

  it('renders table empty state with Reintentar button when audit trail entries are empty', async () => {
    const getAuditTrailMock = vi.fn().mockResolvedValue({ entries: [], total: 0 });
    const mockBridge = {
      getAuditTrail: getAuditTrailMock,
      fetchRaw: vi.fn().mockResolvedValue({}),
    } as unknown as NexusBridge;

    render(<AuditTrailPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText(/No audit records matching filters found/i)).toBeDefined();
    });

    const retryBtn = screen.getByRole('button', { name: /Reintentar/i });
    expect(retryBtn).toBeDefined();
    fireEvent.click(retryBtn);
    expect(getAuditTrailMock).toHaveBeenCalledTimes(2);
  });

  it('passes AbortSignal and aborts active query on Cancel in DecisionServicePanel', async () => {
    let capturedSignal: AbortSignal | undefined;
    const mockBridge = {
      fetchRaw: vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
        capturedSignal = init?.signal as AbortSignal;
        return new Promise(() => {}); // pending
      }),
    } as unknown as NexusBridge;

    render(<DecisionServicePanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(capturedSignal).toBeDefined();
    });
    expect(capturedSignal?.aborted).toBe(false);

    const cancelBtn = screen.getByRole('button', { name: /Cancel shadow log query/i });
    expect(cancelBtn).toBeDefined();
    fireEvent.click(cancelBtn);

    expect(capturedSignal?.aborted).toBe(true);
    await waitFor(() => {
      expect(screen.getByText(/Consulta cancelada por el usuario/i)).toBeDefined();
    });
  });

  it('passes AbortSignal and aborts active query on Cancel in ContradictionsPanel', async () => {
    let capturedSignal: AbortSignal | undefined;
    const mockBridge = {
      fetchRaw: vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
        capturedSignal = init?.signal as AbortSignal;
        return new Promise(() => {}); // pending
      }),
    } as unknown as NexusBridge;

    render(<ContradictionsPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(capturedSignal).toBeDefined();
    });
    expect(capturedSignal?.aborted).toBe(false);

    const cancelBtn = screen.getByRole('button', { name: /Cancel conflicted nodes query/i });
    expect(cancelBtn).toBeDefined();
    fireEvent.click(cancelBtn);

    expect(capturedSignal?.aborted).toBe(true);
    await waitFor(() => {
      expect(screen.getByText(/Consulta cancelada por el usuario/i)).toBeDefined();
    });
  });

  it('passes AbortSignal and aborts active queries on Cancel in AuditTrailPanel', async () => {
    let capturedSignal: AbortSignal | undefined;
    const mockBridge = {
      getAuditTrail: vi.fn().mockImplementation(() => new Promise(() => {})),
      fetchRaw: vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
        capturedSignal = init?.signal as AbortSignal;
        return new Promise(() => {}); // pending
      }),
    } as unknown as NexusBridge;

    render(<AuditTrailPanel bridge={mockBridge} />);

    await waitFor(() => {
      expect(capturedSignal).toBeDefined();
    });
    expect(capturedSignal?.aborted).toBe(false);

    const cancelBtn = screen.getByRole('button', { name: /Cancel audit trail query/i });
    expect(cancelBtn).toBeDefined();
    fireEvent.click(cancelBtn);

    expect(capturedSignal?.aborted).toBe(true);
    await waitFor(() => {
      expect(screen.getByText(/Consulta de auditoría cancelada por el usuario/i)).toBeDefined();
    });
  });

  it('scopes Escape key listener so it does not cross between co-mounted panels (T2)', async () => {
    let decisionSignal: AbortSignal | undefined;
    let contradictionsSignal: AbortSignal | undefined;

    const mockBridgeDecision = {
      fetchRaw: vi.fn().mockImplementation((url: string, init?: RequestInit) => {
        if (url.includes('/api/v1/decide/shadow')) {
          decisionSignal = init?.signal as AbortSignal;
          return new Promise(() => {}); // pending
        }
        return Promise.resolve({});
      }),
    } as unknown as NexusBridge;

    const mockBridgeContradictions = {
      fetchRaw: vi.fn().mockImplementation((url: string, init?: RequestInit) => {
        if (url.includes('/api/v1/silva/conflicted')) {
          contradictionsSignal = init?.signal as AbortSignal;
          return new Promise(() => {}); // pending
        }
        return Promise.resolve({});
      }),
    } as unknown as NexusBridge;

    render(
      <div>
        <DecisionServicePanel bridge={mockBridgeDecision} />
        <ContradictionsPanel bridge={mockBridgeContradictions} />
      </div>
    );

    await waitFor(() => {
      expect(decisionSignal).toBeDefined();
      expect(contradictionsSignal).toBeDefined();
    });

    expect(decisionSignal?.aborted).toBe(false);
    expect(contradictionsSignal?.aborted).toBe(false);

    // 1. Focus element inside DecisionServicePanel and press Escape
    const cancelDecisionBtn = screen.getByRole('button', { name: /Cancel shadow log query/i });
    cancelDecisionBtn.focus();
    fireEvent.keyDown(cancelDecisionBtn, { key: 'Escape' });

    // DecisionServicePanel must abort
    expect(decisionSignal?.aborted).toBe(true);

    // ContradictionsPanel must NOT abort (Escape must not leak across panels)
    expect(contradictionsSignal?.aborted).toBe(false);

    // 2. Now focus element inside ContradictionsPanel and press Escape
    const cancelContradictionsBtn = screen.getByRole('button', { name: /Cancel conflicted nodes query/i });
    cancelContradictionsBtn.focus();
    fireEvent.keyDown(cancelContradictionsBtn, { key: 'Escape' });

    // Now ContradictionsPanel must abort
    expect(contradictionsSignal?.aborted).toBe(true);
  });
});




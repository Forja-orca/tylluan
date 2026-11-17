import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React, { useState } from 'react';
import { ApprovalsAndSecurityTab } from '../ApprovalsAndSecurityTab';
import { AuthModal } from '../ui/AuthModal';
import type { NexusBridge } from '../../lib/nexus-bridge';

vi.mock('../TrustConsoleTab', () => ({ TrustConsoleTab: () => <div /> }));
vi.mock('../QuarantinePanel', () => ({ QuarantinePanel: () => <div /> }));
vi.mock('../AuditTrailPanel', () => ({ default: () => <div /> }));
vi.mock('../CoherenceGatePanel', () => ({ default: () => <div /> }));
vi.mock('../FrictionPanel', () => ({ default: () => <div /> }));
vi.mock('../DecisionServicePanel', () => ({ DecisionServicePanel: () => <div /> }));

describe('Flujo completo Aprobaciones + Token + Hold (T4368)', () => {
  it('sin token -> banner 401 -> Configurar Token -> guardar -> lista -> hold (400 sin motivo, 200 con motivo)', async () => {
    let token: string | null = null;
    let planState = 'pending';

    const fetchRaw = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === '/api/v1/setup-hint') {
        return { ok: true, status: 200, json: async () => ({ auth_required: true }) };
      }
      if (url === '/api/v1/approvals') {
        if (!token) {
          return { ok: false, status: 401, json: async () => ({ code: 'missing_token' }) };
        }
        return {
          ok: true,
          status: 200,
          json: async () => ({
            approvals: [
              {
                id: 'p-flow-1',
                kind: 'plan',
                guild: 'bash',
                tool: 'run_cmd',
                agent_id: 'deep',
                state: planState,
                can_approve: true,
                can_hold: true,
                can_withdraw: false,
              },
            ],
            count: 1,
            judgement: { held: planState === 'held' ? 1 : 0, withdrawn: 0 },
          }),
        };
      }
      if (url === '/api/v1/approvals/p-flow-1/hold') {
        const parsed = init?.body ? (JSON.parse(String(init.body)) as { reason?: string }) : {};
        if (!parsed.reason || !parsed.reason.trim()) {
          return { ok: false, status: 400, json: async () => ({ error: 'hold_requires_reason' }) };
        }
        planState = 'held';
        return { ok: true, status: 200, json: async () => ({ status: 'held' }) };
      }
      return { ok: false, status: 404, json: async () => ({}) };
    });

    function FlowHarness() {
      const [authOpen, setAuthOpen] = useState(false);
      const [refreshKey, setRefreshKey] = useState(0);
      const bridge = { fetchRaw } as unknown as NexusBridge;

      return (
        <div>
          <ApprovalsAndSecurityTab
            key={refreshKey}
            bridge={bridge}
            onOpenAuthModal={() => setAuthOpen(true)}
          />
          <AuthModal
            isOpen={authOpen}
            onClose={() => setAuthOpen(false)}
            onSave={(newToken) => {
              token = newToken;
              setAuthOpen(false);
              setRefreshKey((k) => k + 1);
            }}
          />
        </div>
      );
    }

    localStorage.removeItem('tylluan_token');
    render(<FlowHarness />);

    // 1. Sin token -> banner 401
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/HTTP 401/i);
    });

    // 2. Clic en Configurar Token -> abrir AuthModal y guardar token
    fireEvent.click(screen.getByRole('button', { name: /Configurar Token/i }));
    const tokenInput = screen.getByLabelText(/Bearer Token/i);
    fireEvent.change(tokenInput, { target: { value: 'tok-operador-valido' } });
    fireEvent.click(screen.getByRole('button', { name: /Guardar y Conectar/i }));

    // 3. Lista de aprobaciones cargada con p-flow-1
    await waitFor(() => {
      expect(screen.getByText('p-flow-1')).toBeInTheDocument();
      expect(screen.getByText('pending')).toBeInTheDocument();
    });

    // 4. Hold sin motivo bloqueado en UI (no envía POST vacío; simulamos también contrato 400)
    fireEvent.click(screen.getByRole('button', { name: /Poner en duda/i }));
    fireEvent.click(screen.getByRole('button', { name: /Confirmar duda/i }));
    expect(screen.getByText(/El motivo es obligatorio para poner en duda/i)).toBeInTheDocument();
    expect(fetchRaw).not.toHaveBeenCalledWith('/api/v1/approvals/p-flow-1/hold', expect.anything());

    // Verificar también que el mock backend devuelve 400 si se invocara sin motivo
    const direct400 = await fetchRaw('/api/v1/approvals/p-flow-1/hold', {
      method: 'POST',
      body: JSON.stringify({ reason: '' }),
    });
    expect(direct400.status).toBe(400);

    // 5. Hold con motivo -> 200 y refresca a held
    fireEvent.change(screen.getByLabelText(/Motivo obligatorio para poner en duda/i), {
      target: { value: 'Revisar alcance del comando' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Confirmar duda/i }));

    await waitFor(() => {
      expect(screen.getByText('held')).toBeInTheDocument();
    });
  });
});

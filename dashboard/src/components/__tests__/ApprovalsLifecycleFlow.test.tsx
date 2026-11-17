import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { ApprovalsAndSecurityTab } from '../ApprovalsAndSecurityTab';
import type { NexusBridge } from '../../lib/nexus-bridge';

vi.mock('../TrustConsoleTab', () => ({ TrustConsoleTab: () => <div /> }));
vi.mock('../QuarantinePanel', () => ({ QuarantinePanel: () => <div /> }));
vi.mock('../AuditTrailPanel', () => ({ default: () => <div /> }));
vi.mock('../CoherenceGatePanel', () => ({ default: () => <div /> }));
vi.mock('../FrictionPanel', () => ({ default: () => <div /> }));
vi.mock('../DecisionServicePanel', () => ({ DecisionServicePanel: () => <div /> }));

describe('Flujo de aprobaciones hold/withdraw con 401/403/409 (T4385)', () => {
  it('401 en GET /api/v1/approvals muestra aviso HTTP 401 y permite abrir Configurar Token', async () => {
    const onOpenAuthModal = vi.fn();
    const fetchRaw = vi.fn().mockImplementation(async (url: string) => {
      if (url === '/api/v1/setup-hint') {
        return { ok: true, status: 200, json: async () => ({ auth_required: true }) };
      }
      if (url === '/api/v1/approvals') {
        return { ok: false, status: 401, json: async () => ({ code: 'unauthorized' }) };
      }
      return { ok: false, status: 404, json: async () => ({}) };
    });

    const bridge = { fetchRaw } as unknown as NexusBridge;
    render(<ApprovalsAndSecurityTab bridge={bridge} onOpenAuthModal={onOpenAuthModal} />);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(/HTTP 401/i);

    fireEvent.click(screen.getByRole('button', { name: /Configurar Token/i }));
    expect(onOpenAuthModal).toHaveBeenCalledTimes(1);
  });

  it('403 al ejecutar hold muestra Permiso denegado (HTTP 403) y mantiene abierto el motivo', async () => {
    const fetchRaw = vi.fn().mockImplementation(async (url: string) => {
      if (url === '/api/v1/setup-hint') {
        return { ok: true, status: 200, json: async () => ({ auth_required: true }) };
      }
      if (url === '/api/v1/approvals') {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            approvals: [
              {
                id: 'ap-403-1',
                kind: 'plan',
                guild: 'bash',
                tool: 'exec',
                agent_id: 'deep',
                state: 'pending',
                can_approve: false,
                can_hold: true,
                can_withdraw: false,
              },
            ],
            count: 1,
            judgement: { held: 0, withdrawn: 0 },
          }),
        };
      }
      if (url === '/api/v1/approvals/ap-403-1/hold') {
        return { ok: false, status: 403, json: async () => ({ error: 'forbidden' }) };
      }
      return { ok: false, status: 404, json: async () => ({}) };
    });

    const bridge = { fetchRaw } as unknown as NexusBridge;
    render(<ApprovalsAndSecurityTab bridge={bridge} />);

    await screen.findByText('ap-403-1');

    fireEvent.click(screen.getByRole('button', { name: /Poner en duda/i }));
    const reasonInput = screen.getByLabelText(/Motivo obligatorio para poner en duda/i);
    fireEvent.change(reasonInput, { target: { value: 'Verificar permisos del agente' } });
    fireEvent.click(screen.getByRole('button', { name: /Confirmar duda/i }));

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(
        /Permiso denegado \(HTTP 403\) al ejecutar hold sobre ap-403-1/i,
      );
    });
    expect(screen.getByLabelText(/Motivo obligatorio para poner en duda/i)).toBeInTheDocument();
  });

  it('409 al ejecutar withdraw muestra Conflicto de estado (HTTP 409) y luego 200 actualiza retirados', async () => {
    let conflictDone = false;
    let withdrawn = false;

    const fetchRaw = vi.fn().mockImplementation(async (url: string) => {
      if (url === '/api/v1/setup-hint') {
        return { ok: true, status: 200, json: async () => ({ auth_required: true }) };
      }
      if (url === '/api/v1/approvals') {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            approvals: withdrawn
              ? []
              : [
                  {
                    id: 'ap-409-1',
                    kind: 'plan',
                    guild: 'git',
                    tool: 'push',
                    agent_id: 'deep',
                    state: 'pending',
                    can_approve: false,
                    can_hold: false,
                    can_withdraw: true,
                  },
                ],
            count: withdrawn ? 0 : 1,
            judgement: { held: 0, withdrawn: withdrawn ? 1 : 0 },
          }),
        };
      }
      if (url === '/api/v1/approvals/ap-409-1/withdraw') {
        if (!conflictDone) {
          conflictDone = true;
          return { ok: false, status: 409, json: async () => ({ error: 'state_conflict' }) };
        }
        withdrawn = true;
        return { ok: true, status: 200, json: async () => ({ status: 'withdrawn' }) };
      }
      return { ok: false, status: 404, json: async () => ({}) };
    });

    const bridge = { fetchRaw } as unknown as NexusBridge;
    render(<ApprovalsAndSecurityTab bridge={bridge} />);

    await screen.findByText('ap-409-1');

    // Primer intento -> 409
    fireEvent.click(screen.getByRole('button', { name: /Retirar/i }));
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(
        /Conflicto de estado \(HTTP 409\) al ejecutar withdraw sobre ap-409-1/i,
      );
    });

    // Segundo intento -> 200 y refresca contador retirados: 1
    fireEvent.click(screen.getByRole('button', { name: /Retirar/i }));
    await waitFor(() => {
      expect(screen.getByText(/retirados:\s*1/i)).toBeInTheDocument();
    });
  });
});

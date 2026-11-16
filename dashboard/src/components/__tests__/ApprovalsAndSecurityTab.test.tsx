import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { ApprovalsAndSecurityTab } from '../ApprovalsAndSecurityTab';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { useAppStore } from '../../stores/useAppStore';

vi.mock('../TrustConsoleTab', () => ({
  TrustConsoleTab: () => <div data-testid="mock-trust-console" />,
}));
vi.mock('../QuarantinePanel', () => ({
  QuarantinePanel: () => <div data-testid="mock-quarantine-panel" />,
}));
vi.mock('../AuditTrailPanel', () => ({
  default: () => <div data-testid="mock-audit-trail" />,
}));
vi.mock('../CoherenceGatePanel', () => ({
  default: () => <div data-testid="mock-coherence-gate" />,
}));
vi.mock('../FrictionPanel', () => ({
  default: () => <div data-testid="mock-friction-panel" />,
}));
vi.mock('../DecisionServicePanel', () => ({
  DecisionServicePanel: () => <div data-testid="mock-decision-service" />,
}));

describe('ApprovalsAndSecurityTab', () => {
  it('muestra estado vacío cuando no hay aprobaciones pendientes', () => {
    render(<ApprovalsAndSecurityTab bridge={null} approvals={[]} />);

    expect(screen.getByText(/Revisa acciones que piden tu permiso/i)).toBeInTheDocument();
    expect(screen.queryByText(/HITL|ASI06/i)).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /Pendiente de tu aprobación/i })).toBeInTheDocument();
    expect(screen.getByText(/Sin acciones pendientes de aprobación en este momento/i)).toBeInTheDocument();
    expect(screen.getByText(/Qué es:/i)).toBeInTheDocument();
    expect(screen.getByText(/Cómo aparece uno:/i)).toBeInTheDocument();
    expect(screen.getByText(/Qué hacer:/i)).toBeInTheDocument();
  });

  it('renderiza la lista de approvals recibidos y permite ir a Coloquio', () => {
    useAppStore.getState().setActiveTab('approvals');
    render(
      <ApprovalsAndSecurityTab
        bridge={null}
        approvals={[
          { id: 'apr-42', tool: 'filesystem', guild: 'builders', status: 'pending' },
          { id: 'apr-43', tool: 'terminal', status: 'blocked' },
        ]}
      />
    );

    const list = screen.getByRole('list', { name: /Lista de acciones pendientes de aprobación/i });
    expect(list).toBeInTheDocument();
    expect(screen.getByText('filesystem')).toBeInTheDocument();
    expect(screen.getByText(/builders/)).toBeInTheDocument();
    expect(screen.getByText('apr-42')).toBeInTheDocument();
    expect(screen.getByText('pending')).toBeInTheDocument();
    expect(screen.getByText('terminal')).toBeInTheDocument();
    expect(screen.getByText('apr-43')).toBeInTheDocument();
    expect(screen.getByText('blocked')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Revisar planes en Coloquio/i }));
    expect(useAppStore.getState().activeTab).toBe('coloquio');
  });

  it('consulta GET /api/v1/approvals, muestra lista unificada y botones según can_approve/can_hold/can_withdraw', async () => {
    const fetchRaw = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        approvals: [
          {
            id: 'g-1',
            kind: 'grant',
            guild: 'filesystem',
            tool_name: 'write_file',
            agent_id: 'deep',
            summary: '1 arg(s), hash A1B2C3D4',
            state: 'pending',
            can_approve: true,
            can_hold: false,
            can_withdraw: false,
          },
          {
            id: 'p-1',
            kind: 'plan',
            guild: 'bash',
            tool: 'run_cmd',
            agent_id: 'antigravity',
            summary: '2 arg(s), hash E5F60708',
            state: 'held',
            can_approve: false,
            can_hold: true,
            can_withdraw: true,
          },
        ],
        count: 2,
        judgement: { held: 1, withdrawn: 0 },
      }),
    });
    const bridge = { fetchRaw } as unknown as NexusBridge;

    render(<ApprovalsAndSecurityTab bridge={bridge} />);

    await waitFor(() => {
      expect(fetchRaw).toHaveBeenCalledWith('/api/v1/approvals');
      expect(screen.getByText('write_file')).toBeInTheDocument();
      expect(screen.getByText('run_cmd')).toBeInTheDocument();
    });

    expect(screen.getByText(/en duda: 1 · retirados: 0/i)).toBeInTheDocument();

    const grantItem = screen.getByText('g-1').closest('li') as HTMLElement;
    expect(within(grantItem).getByRole('button', { name: /^Aprobar$/i })).toBeInTheDocument();
    expect(within(grantItem).queryByRole('button', { name: /Poner en duda/i })).not.toBeInTheDocument();
    expect(within(grantItem).queryByRole('button', { name: /^Retirar$/i })).not.toBeInTheDocument();

    const planItem = screen.getByText('p-1').closest('li') as HTMLElement;
    expect(within(planItem).queryByRole('button', { name: /^Aprobar$/i })).not.toBeInTheDocument();
    expect(within(planItem).getByRole('button', { name: /Poner en duda/i })).toBeInTheDocument();
    expect(within(planItem).getByRole('button', { name: /^Retirar$/i })).toBeInTheDocument();
  });

  it('exige motivo obligatorio en Poner en duda (hold) antes de enviar al bridge', async () => {
    const fetchRaw = vi.fn().mockImplementation((url: string) => {
      if (url === '/api/v1/approvals') {
        return Promise.resolve({
          ok: true,
          status: 200,
          json: async () => ({
            approvals: [
              {
                id: 'p-9',
                kind: 'plan',
                guild: 'bash',
                tool: 'run_cmd',
                agent_id: 'deep',
                state: 'pending',
                can_approve: true,
                can_hold: true,
                can_withdraw: false,
              },
            ],
            count: 1,
            judgement: { held: 0, withdrawn: 0 },
          }),
        });
      }
      return Promise.resolve({ ok: true, status: 200, json: async () => ({}) });
    });
    const bridge = { fetchRaw } as unknown as NexusBridge;

    render(<ApprovalsAndSecurityTab bridge={bridge} />);

    await waitFor(() => expect(screen.getByText('p-9')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Poner en duda/i }));
    fireEvent.click(screen.getByRole('button', { name: /Confirmar duda/i }));

    expect(screen.getByText(/El motivo es obligatorio para poner en duda/i)).toBeInTheDocument();
    expect(fetchRaw).not.toHaveBeenCalledWith('/api/v1/approvals/p-9/hold', expect.anything());

    const reasonInput = screen.getByLabelText(/Motivo obligatorio para poner en duda/i);
    fireEvent.change(reasonInput, { target: { value: 'Falta verificación previa' } });
    fireEvent.click(screen.getByRole('button', { name: /Confirmar duda/i }));

    await waitFor(() => {
      expect(fetchRaw).toHaveBeenCalledWith('/api/v1/approvals/p-9/hold', expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ reason: 'Falta verificación previa' }),
      }));
    });
  });

  it('si /api/v1/approvals devuelve 401 muestra aviso sin exponer token y abre el modal de token', async () => {
    const fetchRaw = vi.fn().mockResolvedValue({
      ok: false,
      status: 401,
      json: async () => ({ code: 'missing_token' }),
    });
    const onOpenAuthModal = vi.fn();
    const bridge = { fetchRaw } as unknown as NexusBridge;

    render(<ApprovalsAndSecurityTab bridge={bridge} onOpenAuthModal={onOpenAuthModal} />);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/HTTP 401/i);
    });
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Bearer\s+[A-Za-z0-9_-]{8,}/);

    const tokenBtn = screen.getByRole('button', { name: /Configurar Token/i });
    fireEvent.click(tokenBtn);
    expect(onOpenAuthModal).toHaveBeenCalledTimes(1);
  });

  it.each([
    { action: 'approve' as const, label: /^Aprobar$/i, status: 403, expected: /HTTP 403/i },
    { action: 'approve' as const, label: /^Aprobar$/i, status: 409, expected: /HTTP 409/i },
    { action: 'hold' as const, label: /Poner en duda/i, status: 403, expected: /HTTP 403/i },
    { action: 'hold' as const, label: /Poner en duda/i, status: 409, expected: /HTTP 409/i },
    { action: 'withdraw' as const, label: /^Retirar$/i, status: 403, expected: /HTTP 403/i },
    { action: 'withdraw' as const, label: /^Retirar$/i, status: 409, expected: /HTTP 409/i },
  ])('mantiene la fila, muestra mensaje claro y no expone token ante $status en $action', async ({ action, label, status, expected }) => {
    const fetchRaw = vi.fn().mockImplementation((url: string) => {
      if (url === '/api/v1/approvals') {
        return Promise.resolve({
          ok: true,
          status: 200,
          json: async () => ({
            approvals: [
              {
                id: 'p-err-1',
                kind: 'plan',
                guild: 'bash',
                tool: 'run_cmd',
                agent_id: 'deep',
                state: 'pending',
                can_approve: true,
                can_hold: true,
                can_withdraw: true,
              },
            ],
            count: 1,
            judgement: { held: 0, withdrawn: 0 },
          }),
        });
      }
      return Promise.resolve({ ok: false, status, json: async () => ({}) });
    });
    const bridge = { fetchRaw } as unknown as NexusBridge;

    render(<ApprovalsAndSecurityTab bridge={bridge} />);
    await waitFor(() => expect(screen.getByText('p-err-1')).toBeInTheDocument());

    if (action === 'hold') {
      fireEvent.click(screen.getByRole('button', { name: label }));
      fireEvent.change(screen.getByLabelText(/Motivo obligatorio para poner en duda/i), {
        target: { value: 'Revisión pendiente' },
      });
      fireEvent.click(screen.getByRole('button', { name: /Confirmar duda/i }));
    } else {
      fireEvent.click(screen.getByRole('button', { name: label }));
    }

    await waitFor(() => {
      const statusMsg = screen.getByRole('status');
      expect(statusMsg).toHaveTextContent(expected);
      expect(statusMsg).not.toHaveTextContent(/Bearer\s+[A-Za-z0-9_-]{8,}/);
    });
    expect(screen.getByText('p-err-1')).toBeInTheDocument();
  });
});

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { ApprovalsAndSecurityTab } from '../ApprovalsAndSecurityTab';
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
});

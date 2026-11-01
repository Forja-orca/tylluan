import { fireEvent, render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { ColoquioHandoffBadge } from '../coloquio/ColoquioHandoffBadge';
import { ColoquioHitlCard } from '../coloquio/ColoquioHitlCard';
import { ColoquioThinkingBlock } from '../coloquio/ColoquioThinkingBlock';
import { ColoquioToolCard } from '../coloquio/ColoquioToolCard';

describe('Coloquio AG-UI Cards — Keyboard & Focus Accessibility (AG-25 P4)', () => {
  it('ColoquioHandoffBadge exposes role="status" and accessible label', () => {
    render(
      <ColoquioHandoffBadge
        handoff={{
          kind: 'delivery',
          label: 'ENTREGADO',
          taskId: 'AG-24',
          fromAgent: 'antigravity',
          toAgents: ['claude-code', 'jose'],
          deliveredTag: true,
        }}
      />
    );

    const statusRegion = screen.getByRole('status', { name: /estado de coordinación: entregado/i });
    expect(statusRegion).toBeInTheDocument();
    expect(screen.getByText('AG-24')).toBeInTheDocument();
    expect(screen.getByText('@claude-code')).toBeInTheDocument();
    expect(screen.getByText('T-Delivered')).toBeInTheDocument();
  });

  it('ColoquioThinkingBlock toggles via click/Enter and collapses with focus restored on Escape', () => {
    render(<ColoquioThinkingBlock thoughtContent={'Paso 1: auditar tokens\nPaso 2: medir contraste'} />);

    const toggleBtn = screen.getByRole('button', { name: /razonamiento/i });
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');
    expect(toggleBtn.className).toContain('focus-visible:ring-2');

    // Open via click (native <button> handles Enter/Space identically)
    fireEvent.click(toggleBtn);
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(/Paso 1: auditar tokens/i)).toBeInTheDocument();

    // Press Escape to collapse and verify focus returns to toggle button
    fireEvent.keyDown(toggleBtn, { key: 'Escape' });
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');
    expect(document.activeElement).toBe(toggleBtn);
  });

  it('ColoquioToolCard toggles details and collapses on Escape with focus ring classes', () => {
    render(
      <ColoquioToolCard
        toolCall={{
          id: 'tc-1',
          tool: 'post_to_channel',
          guild: 'coloquio',
          intent: 'post to general',
          performed: "guild='coloquio' tool='post_to_channel' (ok)",
          changed: "Posted to 'general'",
          status: 'passed',
          rawJson: '{"status":"passed"}',
        }}
      />
    );

    const toggleBtn = screen.getByRole('button', { name: /coloquio · post_to_channel/i });
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');
    expect(toggleBtn.className).toContain('focus-visible:ring-2');

    fireEvent.click(toggleBtn);
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(/post to general/i)).toBeInTheDocument();

    fireEvent.keyDown(toggleBtn, { key: 'Escape' });
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');
    expect(document.activeElement).toBe(toggleBtn);
  });

  it('ColoquioHitlCard exposes focusable action buttons and prevents duplicate approval clicks', async () => {
    let resolveApprove: (() => void) | undefined;
    const onApprove = vi.fn().mockImplementation(
      () =>
        new Promise<void>(resolve => {
          resolveApprove = resolve;
        })
    );
    const onVisualize = vi.fn();

    render(
      <ColoquioHitlCard
        planTurn={2413}
        isApproved={false}
        tasks={['1. Tokens CSS', '2. Tarjetas Coloquio']}
        onVisualize={onVisualize}
        onApprove={onApprove}
      />
    );

    const region = screen.getByRole('region', { name: /human-in-the-loop plan #2413/i });
    expect(region).toBeInTheDocument();

    const vizBtn = screen.getByRole('button', { name: /visualize on whiteboard/i });
    const approveBtn = screen.getByRole('button', { name: /approve & start/i });
    expect(vizBtn.className).toContain('focus-visible:ring-2');
    expect(approveBtn.className).toContain('focus-visible:ring-2');

    // First click starts approval and disables button immediately
    fireEvent.click(approveBtn);
    fireEvent.click(approveBtn); // Second rapid click must be ignored
    expect(onApprove).toHaveBeenCalledTimes(1);

    if (resolveApprove) resolveApprove();
  });
});

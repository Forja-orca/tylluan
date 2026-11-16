import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { MemoryConsolidated } from '../MemoryConsolidated';
import { KnowledgeGraphTab } from '../KnowledgeGraphTab';

vi.mock('../NodesTab', () => ({
  NodesTab: () => <div data-testid="mock-nodes-tab" />,
}));

vi.mock('../ContradictionsTab', () => ({
  ContradictionsTab: () => <div data-testid="mock-contradictions-tab" />,
}));

vi.mock('../RadialSilvaGraph', () => ({
  RadialSilvaGraph: () => <div data-testid="mock-radial-graph" />,
}));

vi.mock('../IngestPanel', () => ({
  IngestPanel: () => <div data-testid="mock-ingest-panel" />,
}));

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({ sysStatus: null }),
}));

describe('Memoria — Responsivo 375px (T4307)', () => {
  it('envuelve las pestañas principales de MemoryConsolidated con flex-wrap', () => {
    render(
      <MemoryConsolidated
        bridge={null}
        memoryStats={{ document_count: 0, disk_usage_bytes: 0, node_count: 0, edge_count: 0 }}
        notify={vi.fn()}
      />
    );
    const tablist = screen.getByRole('tablist', { name: /Vistas de memoria SilvaDB/i });
    expect(tablist.className).toContain('flex-wrap');
    expect(tablist.className).not.toContain('overflow-x-auto');
  });

  it('envuelve el selector Graph/List de KnowledgeGraphTab con flex-wrap', () => {
    render(<KnowledgeGraphTab bridge={null} notify={vi.fn()} />);
    const modeTabs = screen.getByRole('tablist', { name: /Knowledge View Modes/i });
    expect(modeTabs.className).toContain('flex-wrap');
    expect(modeTabs.parentElement?.className).toContain('flex-wrap');
  });
});

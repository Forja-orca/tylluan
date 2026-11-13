import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { RadialSilvaGraph } from '../RadialSilvaGraph';
import type { NexusBridge } from '../../../lib/nexus-bridge';

describe('RadialSilvaGraph (AG-30b — Consumo de Cómputo Mínimo)', () => {
  let mockBridge: NexusBridge;

  beforeEach(() => {
    vi.clearAllMocks();
    mockBridge = {
      fetchRaw: vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        expect(url).toContain('/api/v1/silva/graph?limit=40');
        expect(options?.signal).toBeDefined();

        // Generar 50 nodos para probar límite estricto de 40 vecinos
        const nodes = Array.from({ length: 50 }, (_, i) => ({
          id: `node-${i}`,
          label: `Recuerdo ${i}`,
          content: `Contenido del recuerdo ${i}`,
          lifecycle_state: i % 2 === 0 ? 'active' : 'consolidated',
          stigmergy_heat: i * 0.1,
          type: 'memory',
        }));

        const links = [
          { source: 'node-0', target: 'node-1', weight: 1.0 },
          { source: 'node-0', target: 'node-2', weight: 0.8 },
          { source: 'node-1', target: 'node-2', weight: 0.5 },
        ];

        return Promise.resolve({ nodes, links });
      }),
    } as unknown as NexusBridge;
  });

  it('(1) Proyecta disposición radial sin bucles ni simulación física', async () => {
    const { container } = render(<RadialSilvaGraph bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('Constelación Radial SilvaDB (AG-30b)')).toBeDefined();
    });

    // Debe renderizar SVG puro con role="img"
    const svg = container.querySelector('svg[role="img"]');
    expect(svg).toBeDefined();
    expect(svg?.getAttribute('role')).toBe('img');
  });

  it('(2) Limita estrictamente a un máximo de 40 vecinos proyectados', async () => {
    render(<RadialSilvaGraph bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('40 vecinos proyectados (máx 40)')).toBeDefined();
    });

    // Círculos de nodos vecinos: exactamente 40
    const neighborGroups = screen.getAllByRole('button', { name: /Nodo vecino/i });
    expect(neighborGroups.length).toBe(40);
  });

  it('(3) Realiza una sola petición cancelable con AbortController a /api/v1/silva/graph', async () => {
    render(<RadialSilvaGraph bridge={mockBridge} />);

    await waitFor(() => {
      expect(mockBridge.fetchRaw).toHaveBeenCalledTimes(1);
    });

    const calls = vi.mocked(mockBridge.fetchRaw).mock.calls;
    expect(calls[0][0]).toBe('/api/v1/silva/graph?limit=40');
    expect(calls[0][1]?.signal instanceof AbortSignal).toBe(true);
  });

  it('(4) Permite seleccionar nuevo nodo central y actualizar el foco', async () => {
    const onSelectMock = vi.fn();
    render(<RadialSilvaGraph bridge={mockBridge} onSelectNode={onSelectMock} />);

    await waitFor(() => {
      expect(screen.getByText('40 vecinos proyectados (máx 40)')).toBeDefined();
    });

    const targetNeighbor = screen.getByRole('button', { name: /Nodo vecino Recuerdo 1,/i });
    fireEvent.click(targetNeighbor);

    expect(onSelectMock).toHaveBeenCalledWith('node-1');
  });

  it('(5) Accesibilidad por teclado: tecla Enter activa el nodo', async () => {
    const onSelectMock = vi.fn();
    render(<RadialSilvaGraph bridge={mockBridge} onSelectNode={onSelectMock} />);

    await waitFor(() => {
      expect(screen.getByText('40 vecinos proyectados (máx 40)')).toBeDefined();
    });

    const targetNeighbor = screen.getByRole('button', { name: /Nodo vecino Recuerdo 2,/i });
    fireEvent.keyDown(targetNeighbor, { key: 'Enter' });

    expect(onSelectMock).toHaveBeenCalledWith('node-2');
  });

  it('(6) Respeta motion-reduce en clases CSS y transiciones', async () => {
    const { container } = render(<RadialSilvaGraph bridge={mockBridge} />);

    await waitFor(() => {
      expect(screen.getByText('Constelación Radial SilvaDB (AG-30b)')).toBeDefined();
    });

    const motionReducedElements = container.querySelectorAll('.motion-reduce\\:transition-none');
    expect(motionReducedElements.length).toBeGreaterThan(0);
  });
});

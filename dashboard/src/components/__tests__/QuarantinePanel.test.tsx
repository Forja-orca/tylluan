import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import axe from 'axe-core';
import { QuarantinePanel } from '../QuarantinePanel';
import type { NexusBridge, QuarantineNode, QuarantineResponse } from '../../lib/nexus-bridge';

// Mock canvas getContext to avoid JSDOM stderr noise
(HTMLCanvasElement.prototype as unknown as Record<string, unknown>).getContext = vi.fn().mockReturnValue({
  measureText: () => ({ width: 10 }),
  fillRect: () => {},
  clearRect: () => {},
  getImageData: () => ({ data: new Array(4) }),
  putImageData: () => {},
  createImageData: () => [],
  setTransform: () => {},
  drawImage: () => {},
  save: () => {},
  fillText: () => {},
  restore: () => {},
  beginPath: () => {},
  moveTo: () => {},
  lineTo: () => {},
  closePath: () => {},
  stroke: () => {},
  translate: () => {},
  scale: () => {},
  rotate: () => {},
  arc: () => {},
  fill: () => {},
});

const mockQuarantineNodes: QuarantineNode[] = [
  {
    id: 'quar-node-1',
    content_preview: 'Suspicious payload injected into prompt memory',
    quarantine_reason: 'prompt_injection_asi06',
    provenance: 'mesh_sync_peer_alpha',
    agent_id: 'deep',
    created_at: 1776672000,
  },
  {
    id: 'quar-node-2',
    content_preview: 'Exfiltrated secret key reference detected in memory candidate',
    quarantine_reason: 'secret_leak_prevention',
    provenance: 'coloquio_ingest',
    agent_id: null,
    created_at: 1776673000,
  },
];

function createMockBridge(customImpl?: {
  getQuarantine?: (limit?: number, offset?: number) => Promise<QuarantineResponse>;
  releaseQuarantine?: (id: string) => Promise<{ released: boolean; id: string }>;
  fetchRaw?: (url: string, opts?: Record<string, unknown>) => Promise<unknown>;
}): NexusBridge {
  return {
    getQuarantine: customImpl?.getQuarantine || vi.fn().mockResolvedValue({ total: 0, nodes: [] }),
    releaseQuarantine: customImpl?.releaseQuarantine || vi.fn().mockResolvedValue({ released: true, id: 'test' }),
    fetchRaw: customImpl?.fetchRaw || vi.fn().mockResolvedValue({ total: 0, nodes: [] }),
  } as unknown as NexusBridge;
}

describe('QuarantinePanel (LOTE AE — ASI06 Judge & D-12 Contract)', () => {
  describe('Estado 1: Vacío', () => {
    it('muestra el estado "sin nodos en cuarentena" cuando no hay memorias aisladas', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockResolvedValue({ total: 0, nodes: [] }),
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText('sin nodos en cuarentena')).toBeInTheDocument());
      expect(screen.getByText(/El juez de ASI06 no tiene memorias aisladas actualmente/)).toBeInTheDocument();
    });
  });

  describe('Estado 2: Cargando', () => {
    it('muestra el spinner y texto de carga mientras la promesa está pendiente', () => {
      const bridge = createMockBridge({
        getQuarantine: () => new Promise(() => {}), // nunca resuelve
      });

      render(<QuarantinePanel bridge={bridge} />);
      expect(screen.getByText('Cargando nodos en cuarentena...')).toBeInTheDocument();
    });
  });

  describe('Estado 3: 403 Requiere Operador', () => {
    it('muestra "requiere operador (HTTP 403)" cuando el endpoint deniega acceso', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockRejectedValue(new Error('HTTP 403 Forbidden: operator clearance required')),
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText(/requiere operador \(HTTP 403\)/i)).toBeInTheDocument());
      expect(screen.getByText(/exige rol de operador/i)).toBeInTheDocument();
    });
  });

  describe('Estado 4: Endpoint no disponible (404 / backend no montado aún)', () => {
    it('muestra "endpoint no disponible" sin romper cuando el kernel retorna 404', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockRejectedValue(new Error('404 Not Found')),
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText('endpoint no disponible')).toBeInTheDocument());
      expect(screen.getByText(/esperando D-12 en el backend/i)).toBeInTheDocument();
    });
  });

  describe('Estado 5: Error general', () => {
    it('muestra banner de error y botón de reintentar ante fallos de red', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockRejectedValue(new Error('Network connection reset')),
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText('Error de consulta')).toBeInTheDocument());
      expect(screen.getByText(/Network connection reset/)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /reintentar/i })).toBeInTheDocument();
    });
  });

  describe('Estado 6: Con Nodos (Lista/Tabla de Cuarentena)', () => {
    it('muestra los nodos con sus campos exactos del contrato D-12', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockResolvedValue({
          total: 2,
          nodes: mockQuarantineNodes,
        }),
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText(/quar-node-1/)).toBeInTheDocument());

      // Verificamos preview de contenido
      expect(screen.getByText(/Suspicious payload injected into prompt memory/)).toBeInTheDocument();
      expect(screen.getByText(/Exfiltrated secret key reference/)).toBeInTheDocument();

      // Motivos
      expect(screen.getByText('prompt_injection_asi06')).toBeInTheDocument();
      expect(screen.getByText('secret_leak_prevention')).toBeInTheDocument();

      // Provenance
      expect(screen.getByText('mesh_sync_peer_alpha')).toBeInTheDocument();
      expect(screen.getByText('coloquio_ingest')).toBeInTheDocument();

      // Agente
      expect(screen.getByText('@deep')).toBeInTheDocument();

      // Contador total en el encabezado
      expect(screen.getByText('2')).toBeInTheDocument();
    });
  });

  describe('Flujo de Confirmación y Liberación', () => {
    it('abre diálogo de confirmación antes de liberar y cancela al presionar Cancelar', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockResolvedValue({
          total: 1,
          nodes: [mockQuarantineNodes[0]],
        }),
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText(/quar-node-1/)).toBeInTheDocument());

      const releaseBtn = screen.getByRole('button', { name: /liberar nodo quar-node-1/i });
      fireEvent.click(releaseBtn);

      // El diálogo modal de confirmación debe aparecer
      expect(screen.getByRole('dialog')).toBeInTheDocument();
      expect(screen.getByText(/Confirmar Liberación de Nodo/i)).toBeInTheDocument();

      // Cancelar
      const cancelBtn = screen.getByRole('button', { name: /cancelar/i });
      fireEvent.click(cancelBtn);

      // El diálogo debe cerrarse y el nodo sigue presente
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(screen.getByText(/quar-node-1/)).toBeInTheDocument();
    });

    it('libera el nodo tras confirmar en el modal e invoca bridge.releaseQuarantine', async () => {
      const releaseQuarantine = vi.fn().mockResolvedValue({ released: true, id: 'quar-node-1' });
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockResolvedValue({
          total: 1,
          nodes: [mockQuarantineNodes[0]],
        }),
        releaseQuarantine,
      });

      render(<QuarantinePanel bridge={bridge} />);
      await waitFor(() => expect(screen.getByText(/quar-node-1/)).toBeInTheDocument());

      // Abrir confirmación
      fireEvent.click(screen.getByRole('button', { name: /liberar nodo quar-node-1/i }));

      // Confirmar liberación
      const confirmBtn = screen.getByRole('button', { name: /confirmar liberación/i });
      fireEvent.click(confirmBtn);

      await waitFor(() => {
        expect(releaseQuarantine).toHaveBeenCalledWith('quar-node-1');
      });

      // El nodo debe removerse de la lista y mostrar feedback de éxito
      await waitFor(() => {
        expect(screen.getByText(/liberado exitosamente/i)).toBeInTheDocument();
      });
      expect(screen.queryByText('Suspicious payload injected into prompt memory')).toBeNull();
    });
  });

  describe('Soporte Temas y Accesibilidad (axe-core)', () => {
    it('renderiza sin violaciones críticas/serias en modo oscuro y claro', async () => {
      const bridge = createMockBridge({
        getQuarantine: vi.fn().mockResolvedValue({
          total: 2,
          nodes: mockQuarantineNodes,
        }),
      });

      // Modo oscuro
      const { container: darkContainer, unmount: u1 } = render(
        <div className="dark bg-slate-950 text-slate-100">
          <QuarantinePanel bridge={bridge} />
        </div>
      );
      await waitFor(() => expect(screen.getByText(/quar-node-1/)).toBeInTheDocument());
      const darkAxe = await axe.run(darkContainer);
      const darkCrit = darkAxe.violations.filter(v => v.impact === 'critical' || v.impact === 'serious');
      expect(darkCrit).toEqual([]);
      u1();

      // Modo claro
      const { container: lightContainer, unmount: u2 } = render(
        <div className="light bg-white text-slate-900">
          <QuarantinePanel bridge={bridge} />
        </div>
      );
      await waitFor(() => expect(screen.getByText(/quar-node-1/)).toBeInTheDocument());
      const lightAxe = await axe.run(lightContainer);
      const lightCrit = lightAxe.violations.filter(v => v.impact === 'critical' || v.impact === 'serious');
      expect(lightCrit).toEqual([]);
      u2();
    });
  });
});

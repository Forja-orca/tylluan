import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { VisualControlRoom, SubruntimeRingsView, AgentHeatmapView } from '../VisualControlRoom';
import type { SubruntimeStatus } from '../VisualControlRoom';

describe('VisualControlRoom — AG-17 Sala de Control Visual', () => {
  beforeEach(() => {
    // Mock HTMLCanvasElement.getContext for JSDOM
    HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
      clearRect: vi.fn(),
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      arc: vi.fn(),
      fill: vi.fn(),
      fillText: vi.fn(),
    });
  });

  const mockSubruntimes: SubruntimeStatus[] = [
    { id: 'Memoria', name: 'Memoria', percent: 100, state: 'running', details: '11051 nodos', tasksCount: 1, runsTotal: 5, errorsTotal: 0 },
    { id: 'Inferencia', name: 'Inferencia', percent: 100, state: 'running', details: 'Embeddings OK', tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
    { id: 'Procesos', name: 'Procesos y Guilds', percent: 75, state: 'running', details: '14 activos', tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
    { id: 'Decision', name: 'Decisión', percent: 90, state: 'running', details: 'Score 90%', tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
    { id: 'Coordinacion', name: 'Coordinación', percent: 80, state: 'running', details: '80 turnos', tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
    { id: 'Red', name: 'Red y Enlace', percent: 0, state: 'idle', details: 'Mesh local', tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
  ];

  it('1. Renderiza los 6 anillos de subruntimes ADR-025 con datos reales y porcentajes', () => {
    render(<SubruntimeRingsView subruntimes={mockSubruntimes} />);

    expect(screen.getByTestId('ring-Memoria')).toBeInTheDocument();
    expect(screen.getByTestId('ring-Inferencia')).toBeInTheDocument();
    expect(screen.getByTestId('ring-Procesos')).toBeInTheDocument();
    expect(screen.getByTestId('ring-Decision')).toBeInTheDocument();
    expect(screen.getByTestId('ring-Coordinacion')).toBeInTheDocument();
    expect(screen.getByTestId('ring-Red')).toBeInTheDocument();

    expect(screen.getAllByText('100%').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('75%')).toBeInTheDocument();
    expect(screen.getByText('11051 nodos')).toBeInTheDocument();
  });

  it('2. Renderiza estado explícito "Sin Dato" cuando un subruntime carece de métricas', () => {
    const nodataSubruntimes: SubruntimeStatus[] = [
      { id: 'Red', name: 'Red', percent: 0, state: 'nodata', details: 'Sin telemetría', tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
    ];
    render(<SubruntimeRingsView subruntimes={nodataSubruntimes} />);
    expect(screen.getByText(/sin dato/i)).toBeInTheDocument();
  });

  it('3. Permite alternar entre las 3 vistas en la barra de control', () => {
    const mockBridge = {
      getRaw: vi.fn().mockResolvedValue({}),
    } as any;

    render(<VisualControlRoom bridge={mockBridge} online={true} />);

    expect(screen.getByText(/Sala de Control Visual/i)).toBeInTheDocument();
    const btnFlow = screen.getByRole('button', { name: /2\. Mapa de Flujo/i });
    const btnHeatmap = screen.getByRole('button', { name: /3\. Mapa de Calor/i });

    fireEvent.click(btnFlow);
    expect(screen.getByText(/Aristas con grosor y velocidad proporcionales/i)).toBeInTheDocument();

    fireEvent.click(btnHeatmap);
    expect(screen.getByText(/Agente \\ Hora/i)).toBeInTheDocument();
  });

  it('4. Renderiza la matriz de calor de actividad agentes x horas correctamente', () => {
    const mockMessages = [
      { author_id: 'claude-code', created_at: 1793271585 },
      { author_id: 'claude-code', created_at: 1793272512 },
      { author_id: 'antigravity', created_at: 1793275468 },
    ];

    render(<AgentHeatmapView messages={mockMessages} />);

    expect(screen.getByText('@claude-code')).toBeInTheDocument();
    expect(screen.getByText('@antigravity')).toBeInTheDocument();
    expect(screen.getByText(/Total mensajes analizados: 3/i)).toBeInTheDocument();
  });
});

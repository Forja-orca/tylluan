import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import axe from 'axe-core';
import { RecallHealthCard, type RecallTelemetryData } from '../RecallHealthCard';

const EXPECTED_BUCKETS = [
  '<=5ms',
  '<=10ms',
  '<=25ms',
  '<=50ms',
  '<=100ms',
  '<=250ms',
  '<=500ms',
  '<=1000ms',
  '<=2500ms',
  '>2500ms',
];

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

const mockTelemetryWithData: RecallTelemetryData = {
  calls: 2504,
  empty_calls: 210,
  empty_ratio: 0.084,
  mean_latency_ms: 41.3,
  latency_histogram: {
    '<=5ms': 12,
    '<=10ms': 80,
    '<=25ms': 450,
    '<=50ms': 920,
    '<=100ms': 680,
    '<=250ms': 250,
    '<=500ms': 80,
    '<=1000ms': 25,
    '<=2500ms': 4,
    '>2500ms': 3,
  },
};

const mockTelemetryWithNulls: RecallTelemetryData = {
  calls: 0,
  empty_calls: 0,
  empty_ratio: null,
  mean_latency_ms: null,
  latency_histogram: {
    '<=5ms': 0,
    '<=10ms': 0,
    '<=25ms': 0,
    '<=50ms': 0,
    '<=100ms': 0,
    '<=250ms': 0,
    '<=500ms': 0,
    '<=1000ms': 0,
    '<=2500ms': 0,
    '>2500ms': 0,
  },
};

describe('RecallHealthCard (LOTE AD — D-5 Telemetry)', () => {
  describe('Estado 1: Ausente (kernel sin telemetría de recall)', () => {
    it('muestra el banner "kernel sin telemetría de recall (actualiza)" cuando isAbsent es true', () => {
      render(<RecallHealthCard isAbsent={true} />);
      expect(screen.getByText('kernel sin telemetría de recall (actualiza)')).toBeInTheDocument();
      expect(screen.getByText(/El kernel en ejecución no expone contadores de telemetría/)).toBeInTheDocument();
    });

    it('muestra el banner de ausente cuando recall es undefined', () => {
      render(<RecallHealthCard recall={undefined} />);
      expect(screen.getByText('kernel sin telemetría de recall (actualiza)')).toBeInTheDocument();
    });
  });

  describe('Estado 2: Con nulls (0 llamadas, métricas calculadas null)', () => {
    it('muestra "sin datos" para empty_ratio y mean_latency (NUNCA 0% ni 0 ms)', () => {
      render(<RecallHealthCard recall={mockTelemetryWithNulls} isAbsent={false} />);

      // Debe mostrar "Sin llamadas"
      expect(screen.getByText('Sin llamadas')).toBeInTheDocument();

      // Calls y Empty calls deben ser 0
      expect(screen.getAllByText('0')).toHaveLength(2);

      // Empty ratio y latencia media deben ser "sin datos", NUNCA 0%
      const sinDatosElements = screen.getAllByText('sin datos');
      expect(sinDatosElements.length).toBeGreaterThanOrEqual(2);

      // Verificamos que NO exista "0%" o "0.0%"
      expect(screen.queryByText('0%')).toBeNull();
      expect(screen.queryByText('0.0%')).toBeNull();
      expect(screen.queryByText('0 ms')).toBeNull();
      expect(screen.queryByText('0.0 ms')).toBeNull();

      // Indicador de histograma sin llamadas
      expect(screen.getByText(/Sin datos de latencia acumulados hasta la primera llamada/)).toBeInTheDocument();
    });
  });

  describe('Estado 3: Con datos (llamadas registradas e histograma completo)', () => {
    it('muestra métricas numéricas formateadas y los 10 cubos de latencia', () => {
      render(<RecallHealthCard recall={mockTelemetryWithData} isAbsent={false} />);

      // Total calls
      expect(screen.getAllByText(/2[,.]?504/).length).toBeGreaterThanOrEqual(1);

      // Empty calls
      expect(screen.getByText('210')).toBeInTheDocument();

      // Empty ratio porcentual (0.084 -> 8.4%)
      expect(screen.getByText('8.4%')).toBeInTheDocument();

      // Latencia media
      expect(screen.getByText('41.3 ms')).toBeInTheDocument();

      // Verificación de los 10 cubos de latencia
      for (const bucket of EXPECTED_BUCKETS) {
        expect(screen.getByText(bucket)).toBeInTheDocument();
      }

      // Conteo de llamadas sumadas en histograma
      expect(screen.getByText('2504 llamadas sumadas')).toBeInTheDocument();
    });
  });

  describe('Soporte Temas Claro y Oscuro', () => {
    it('se renderiza correctamente en modo oscuro', () => {
      const { container } = render(
        <div className="dark bg-slate-950 text-slate-100">
          <RecallHealthCard recall={mockTelemetryWithData} isAbsent={false} />
        </div>
      );
      expect(container.querySelector('.dark\\:bg-slate-900')).toBeInTheDocument();
    });

    it('se renderiza correctamente en modo claro', () => {
      const { container } = render(
        <div className="light bg-white text-slate-900">
          <RecallHealthCard recall={mockTelemetryWithData} isAbsent={false} />
        </div>
      );
      expect(container.querySelector('.bg-white')).toBeInTheDocument();
    });
  });

  describe('Accesibilidad Estructural (axe-core)', () => {
    it('cumple WCAG 2.1 AA sin violaciones críticas ni serias en los 3 estados', async () => {
      // Estado ausente
      const { container: c1, unmount: u1 } = render(
        <div className="dark bg-slate-950 text-slate-100">
          <RecallHealthCard isAbsent={true} />
        </div>
      );
      const res1 = await axe.run(c1);
      const crit1 = res1.violations.filter(v => v.impact === 'critical' || v.impact === 'serious');
      expect(crit1).toEqual([]);
      u1();

      // Estado con nulls
      const { container: c2, unmount: u2 } = render(
        <div className="dark bg-slate-950 text-slate-100">
          <RecallHealthCard recall={mockTelemetryWithNulls} isAbsent={false} />
        </div>
      );
      const res2 = await axe.run(c2);
      const crit2 = res2.violations.filter(v => v.impact === 'critical' || v.impact === 'serious');
      expect(crit2).toEqual([]);
      u2();

      // Estado con datos
      const { container: c3, unmount: u3 } = render(
        <div className="dark bg-slate-950 text-slate-100">
          <RecallHealthCard recall={mockTelemetryWithData} isAbsent={false} />
        </div>
      );
      const res3 = await axe.run(c3);
      const crit3 = res3.violations.filter(v => v.impact === 'critical' || v.impact === 'serious');
      expect(crit3).toEqual([]);
      u3();
    });
  });
});

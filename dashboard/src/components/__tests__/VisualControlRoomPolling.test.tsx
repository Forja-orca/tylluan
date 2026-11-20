/**
 * D232 (T4813) — el sondeo de la Sala de Control no puede correr con la pestaña oculta.
 *
 * Antes: 5 endpoints cada 6 s, siempre, mirase el usuario o no. Ahora: 3 endpoints cada
 * 15 s y **cero** peticiones con `document.hidden`. Al volver a la pestaña, un refresco.
 *
 * Este test es el objetivo de mutación: si alguien quita el `if (document.hidden) return`
 * de `fetchData`, el primer caso se pone ROJO (el sondeo vuelve a correr oculto).
 */
import { render, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { VisualControlRoom } from '../VisualControlRoom';
import type { NexusBridge } from '@tylluan/ui-core';

function bridgeFalso() {
  const fetchMock = vi.fn().mockResolvedValue({});
  const bridge = {
    fetch: fetchMock,
    getRaw: vi.fn().mockResolvedValue({}),
  } as unknown as NexusBridge;
  return { bridge, fetchMock };
}

function ocultarPestana(oculta: boolean) {
  Object.defineProperty(document, 'hidden', {
    configurable: true,
    get: () => oculta,
  });
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => (oculta ? 'hidden' : 'visible'),
  });
}

describe('VisualControlRoom — sondeo con la pestaña oculta (D232)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    ocultarPestana(false);
    HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
      clearRect: vi.fn(), beginPath: vi.fn(), moveTo: vi.fn(), lineTo: vi.fn(),
      stroke: vi.fn(), arc: vi.fn(), fill: vi.fn(), fillText: vi.fn(),
    }) as unknown as typeof HTMLCanvasElement.prototype.getContext;
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('no pide nada si document.hidden es true (aunque pasen minutos)', async () => {
    ocultarPestana(true);
    const { bridge, fetchMock } = bridgeFalso();
    render(<VisualControlRoom bridge={bridge} online={true} />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5 * 60 * 1000);
    });

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('pide al volver a la pestaña y no espera al siguiente ciclo', async () => {
    ocultarPestana(true);
    const { bridge, fetchMock } = bridgeFalso();
    render(<VisualControlRoom bridge={bridge} online={true} />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60000);
    });
    expect(fetchMock).not.toHaveBeenCalled();

    ocultarPestana(false);
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'));
    });

    expect(fetchMock).toHaveBeenCalled();
    // y solo los tres endpoints del componente: health/detailed y system/status NO
    const rutas = fetchMock.mock.calls.map((c) => String(c[0]));
    expect(rutas).not.toContain('/api/v1/health/detailed');
    expect(rutas).not.toContain('/api/v1/system/status');
    expect(rutas).toContain('/api/v1/runtime/tasks');
  });

  it('con la pestaña visible sondea cada 15 s, no cada 6', async () => {
    ocultarPestana(false);
    const { bridge, fetchMock } = bridgeFalso();
    render(<VisualControlRoom bridge={bridge} online={true} />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    const primeraTanda = fetchMock.mock.calls.length;
    expect(primeraTanda).toBeGreaterThan(0);

    // a los 6 s (el intervalo viejo) no debe haber tanda nueva
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });
    expect(fetchMock.mock.calls.length).toBe(primeraTanda);

    // a los 15 s sí
    await act(async () => {
      await vi.advanceTimersByTimeAsync(9000);
    });
    expect(fetchMock.mock.calls.length).toBeGreaterThan(primeraTanda);
  });
});

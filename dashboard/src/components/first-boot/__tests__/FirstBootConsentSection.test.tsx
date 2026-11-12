import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { FirstBootConsentSection } from '../FirstBootConsentSection';
import { TEST_FIRST_BOOT_MANIFEST, puedeDescargar } from '../first-boot-manifest';
import type { NexusBridge } from '../../../lib/nexus-bridge';

describe('FirstBootConsentSection — Consentimiento de Descarga de Modelos (T3898 / T3903 / T4009)', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  it('renderiza el estado del sistema en primer arranque limpio con SilvaDB activa y aviso de modelo ausente', () => {
    render(
      <FirstBootConsentSection
        bridge={null}
        sysStatus={{ embeddings_loaded: false, silva_healthy: true }}
        notify={vi.fn()}
      />
    );

    expect(screen.getByText(/Primer Arranque: Activación de Búsqueda Semántica Vectorial/i)).toBeInTheDocument();
    expect(screen.getByText(/Memoria Soberana \(SilvaDB\)/i)).toBeInTheDocument();
    expect(screen.getByText(/Activa · Búsqueda léxica BM25, índice FTS5 y grafo de conocimiento PPR/i)).toBeInTheDocument();
    expect(screen.getByText(/Sin modelo descargado · Operando en modo BM25 puro/i)).toBeInTheDocument();
    expect(screen.getByText(/BM25 \+ Grafo Activo/i)).toBeInTheDocument();
  });

  it('T4009(a): muestra el modelo configurado, arquitectura y licencia cuando se inyecta initialManifest por prop (sin flags de simulación en prod)', () => {
    const candidate = TEST_FIRST_BOOT_MANIFEST.candidates[0];

    render(
      <FirstBootConsentSection
        bridge={null}
        sysStatus={{ embeddings_loaded: false }}
        notify={vi.fn()}
        initialManifest={TEST_FIRST_BOOT_MANIFEST}
      />
    );

    expect(screen.getByText((content) => content.includes(candidate.name))).toBeInTheDocument();
    expect(screen.getByText(candidate.architecture)).toBeInTheDocument();
    expect(screen.getByText(candidate.license)).toBeInTheDocument();
  });

  it('T3813(b) / T3995: en modo producción sin datos del backend NO inventa mxbai-embed-large ni rutas hardcodeadas (renderiza —)', () => {
    render(
      <FirstBootConsentSection
        bridge={null}
        sysStatus={{ embeddings_loaded: false }}
        notify={vi.fn()}
      />
    );

    expect(screen.queryByText(/mxbai-embed-large/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/models\/fastembed/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Ruta local: —/i)).toBeInTheDocument();
    expect(screen.getByText(/📦 Modelo Configurado: —/i)).toBeInTheDocument();
  });

  it('T3813(b) / T3995: en modo producción puebla modelo y ruta local estrictamente desde GET /api/v1/models', async () => {
    const mockBridge: NexusBridge = {
      fetchRaw: vi.fn().mockImplementation((path: string) => {
        if (path === '/api/v1/models') {
          return Promise.resolve({
            active: { embedding: 'mxbai-embed-large', vector_dimensions: 1024 },
            models_dir: 'E:\\tylluan\\models',
            detected_local_models: [],
          });
        }
        return Promise.resolve({});
      }),
    } as unknown as NexusBridge;

    render(
      <FirstBootConsentSection
        bridge={mockBridge}
        sysStatus={{ embeddings_loaded: false }}
        notify={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/📦 Modelo Configurado: mxbai-embed-large/i)).toBeInTheDocument();
      expect(screen.getByText(/Ruta local: E:\\tylluan\\models/i)).toBeInTheDocument();
    });
  });

  it('permite continuar en modo BM25 offline y persiste la elección en localStorage', () => {
    const onContinueOffline = vi.fn();
    const notify = vi.fn();

    render(
      <FirstBootConsentSection
        bridge={null}
        sysStatus={{ embeddings_loaded: false }}
        notify={notify}
        onContinueOffline={onContinueOffline}
      />
    );

    const offlineBtn = screen.getByRole('button', { name: /Continuar con Búsqueda Léxica \(BM25\)/i });
    expect(offlineBtn).toBeInTheDocument();

    fireEvent.click(offlineBtn);

    expect(localStorage.getItem('tylluan_first_boot_choice')).toBe('bm25_offline');
    expect(onContinueOffline).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith(
      expect.stringContaining('Modo BM25 offline activado'),
      'info'
    );
  });

  it('incluye el comando de terminal alternativo (tylluan download-models) y respeta DIRECTIVA_MODELOS (el chat es del operador)', () => {
    render(
      <FirstBootConsentSection
        bridge={null}
        sysStatus={{ embeddings_loaded: false }}
        notify={vi.fn()}
      />
    );

    expect(screen.getByText(/O descarga desde terminal:/i)).toBeInTheDocument();
    expect(screen.getByText(/tylluan download-models/i)).toBeInTheDocument();
    expect(screen.getByText(/El chat es del operador/i)).toBeInTheDocument();
    expect(TEST_FIRST_BOOT_MANIFEST.candidates.every((c) => ['embedding', 'reranker', 'vision'].includes(c.role))).toBe(true);
  });

  it('T4009(a): muestra barra de progreso real y permite cancelar cuando el backend responde status="downloading"', async () => {
    const notify = vi.fn();
    const mockBridge: NexusBridge = {
      fetchRaw: vi.fn().mockImplementation((path: string) => {
        if (path === '/api/v1/inference/models/download') {
          return Promise.resolve({
            status: 'downloading',
            progress_percent: 45,
            bytes_transferred_mb: 300,
          });
        }
        return Promise.resolve({
          active: { embedding: null },
          models_dir: 'E:\\tylluan\\models',
        });
      }),
    } as unknown as NexusBridge;

    render(
      <FirstBootConsentSection
        bridge={mockBridge}
        sysStatus={{ embeddings_loaded: false }}
        notify={notify}
        initialManifest={TEST_FIRST_BOOT_MANIFEST}
      />
    );

    const checkbox = screen.getByRole('checkbox');
    fireEvent.click(checkbox);

    const downloadBtn = screen.getByRole('button', { name: /Solicitar Descarga de Modelo/i });
    fireEvent.click(downloadBtn);

    const progressbar = await screen.findByRole('progressbar');
    expect(progressbar).toBeInTheDocument();
    expect(progressbar).toHaveAttribute('aria-valuenow', '45');

    const cancelBtn = screen.getByRole('button', { name: /Cancelar Descarga/i });
    fireEvent.click(cancelBtn);

    expect(notify).toHaveBeenCalledWith(
      expect.stringContaining('Descarga cancelada por el operador'),
      'info'
    );
  });

  describe('Pruebas de Mutación de Consentimiento y Honestidad Radical (TL T3903 / T3930 / T4009)', () => {
    it('T4009(b)/(c) mutación de consentimiento: NADA descarga al montar ni sin marcar explícitamente la casilla de consentimiento (caza M1 y M3 por separado)', async () => {
      // Predicado puro de guarda (caza mutaciones en puedeDescargar)
      expect(puedeDescargar(false, 'idle')).toBe(false);
      expect(puedeDescargar(true, 'idle')).toBe(true);
      expect(puedeDescargar(true, 'downloading')).toBe(false);
      expect(puedeDescargar(true, 'completed')).toBe(false);

      const notify = vi.fn();
      const fetchRawMock = vi.fn().mockImplementation((path: string) => {
        if (path === '/api/v1/models') {
          return Promise.resolve({
            active: { embedding: 'mxbai-embed-large', vector_dimensions: 1024 },
            models_dir: 'E:\\tylluan\\models',
          });
        }
        return Promise.resolve({ status: 'ready' });
      });

      const mockBridge: NexusBridge = {
        fetchRaw: fetchRawMock,
      } as unknown as NexusBridge;

      render(
        <FirstBootConsentSection
          bridge={mockBridge}
          sysStatus={{ embeddings_loaded: false }}
          notify={notify}
        />
      );

      // 1. Al montar solo se consulta GET /api/v1/models; NUNCA se invoca /api/v1/inference/models/download
      await waitFor(() => {
        expect(fetchRawMock).toHaveBeenCalledWith('/api/v1/models');
      });
      expect(fetchRawMock).not.toHaveBeenCalledWith(
        '/api/v1/inference/models/download',
        expect.anything()
      );

      // 2. Sin marcar el checkbox de consentimiento, el botón está deshabilitado (caza mutación M3 en disabled)
      const checkbox = screen.getByRole('checkbox') as HTMLInputElement;
      expect(checkbox.checked).toBe(false);

      const downloadBtn = screen.getByRole('button', { name: /Solicitar Descarga de Modelo/i }) as HTMLButtonElement;
      expect(downloadBtn.disabled).toBe(true);
      expect(downloadBtn).toBeDisabled();

      // 2b. Incluso si se invoca directamente el handler onClick puenteando disabled, la guarda de handleStartDownload bloquea el POST (caza mutación M1 en solitario)
      const reactPropsKey = Object.keys(downloadBtn).find((k) => k.startsWith('__reactProps$'));
      expect(reactPropsKey).toBeDefined();
      const reactProps = (downloadBtn as unknown as Record<string, { onClick?: () => Promise<void> }>)[reactPropsKey!];
      await act(async () => {
        await reactProps.onClick?.();
      });
      expect(notify).toHaveBeenCalledWith(
        expect.stringContaining('Se requiere consentimiento explícito del operador'),
        'error'
      );
      expect(fetchRawMock).not.toHaveBeenCalledWith(
        '/api/v1/inference/models/download',
        expect.anything()
      );

      // 3. Solo tras marcar el consentimiento Y hacer clic en el botón se envía el POST con consent_granted: true
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(true);
      expect(downloadBtn.disabled).toBe(false);
      expect(downloadBtn).not.toBeDisabled();

      fireEvent.click(downloadBtn);
      await waitFor(() => {
        expect(fetchRawMock).toHaveBeenCalledWith(
          '/api/v1/inference/models/download',
          expect.objectContaining({
            method: 'POST',
            body: JSON.stringify({
              model_id: 'mxbai-embed-large',
              consent_granted: true,
            }),
          })
        );
      });
    });

    it('muestra estado de error honesto y NUNCA marca completado si el endpoint devuelve 404 o falla', async () => {
      const onModelReady = vi.fn();
      const notify = vi.fn();

      const mockBridge: NexusBridge = {
        fetchRaw: vi.fn().mockImplementation((path: string) => {
          if (path === '/api/v1/inference/models/download') {
            return Promise.reject(new Error('HTTP 404 Not Found: endpoint no disponible'));
          }
          return Promise.resolve({});
        }),
      } as unknown as NexusBridge;

      render(
        <FirstBootConsentSection
          bridge={mockBridge}
          sysStatus={{ embeddings_loaded: false }}
          notify={notify}
          onModelReady={onModelReady}
        />
      );

      fireEvent.click(screen.getByRole('checkbox'));
      const downloadBtn = screen.getByRole('button', { name: /Solicitar Descarga de Modelo/i });
      fireEvent.click(downloadBtn);

      await waitFor(() => {
        expect(screen.getByText(/Descarga remota no completada/i)).toBeInTheDocument();
      });

      expect(onModelReady).not.toHaveBeenCalled();
      expect(screen.queryByText(/¡Modelo descargado y disponible en el kernel!/i)).not.toBeInTheDocument();
      expect(screen.getByText(/La descarga asistida desde la interfaz aún no está disponible/i)).toBeInTheDocument();
      expect(notify).toHaveBeenCalledWith(
        expect.stringContaining('tylluan download-models'),
        'error'
      );
    });

    it('mutación T3930: respuesta "downloading" sin que aparezca el modelo en active.embedding => onModelReady NO se llama', async () => {
      const onModelReady = vi.fn();
      const notify = vi.fn();

      const mockBridge: NexusBridge = {
        fetchRaw: vi.fn().mockImplementation((path: string) => {
          if (path === '/api/v1/inference/models/download') {
            return Promise.resolve({ status: 'downloading' });
          }
          if (path === '/api/v1/models') {
            return Promise.resolve({
              active: { embedding: null },
              models_dir: 'models/fastembed',
              detected_local_models: [],
            });
          }
          return Promise.resolve({});
        }),
      } as unknown as NexusBridge;

      render(
        <FirstBootConsentSection
          bridge={mockBridge}
          sysStatus={{ embeddings_loaded: false }}
          notify={notify}
          onModelReady={onModelReady}
        />
      );

      fireEvent.click(screen.getByRole('checkbox'));
      const downloadBtn = screen.getByRole('button', { name: /Solicitar Descarga de Modelo/i });
      fireEvent.click(downloadBtn);

      await waitFor(() => {
        expect(notify).toHaveBeenCalledWith(
          expect.stringContaining('Descarga en curso en segundo plano'),
          'info'
        );
      });

      expect(onModelReady).not.toHaveBeenCalled();
      expect(screen.queryByText(/¡Modelo descargado y disponible en el kernel!/i)).not.toBeInTheDocument();
    });

    it('T3930: respuesta "downloading" transiciona a completed y llama a onModelReady SOLO cuando /api/v1/models reporta active.embedding', async () => {
      vi.useFakeTimers();
      try {
        const onModelReady = vi.fn();
        const notify = vi.fn();
        let modelsCallCount = 0;

        const mockBridge: NexusBridge = {
          fetchRaw: vi.fn().mockImplementation((path: string) => {
            if (path === '/api/v1/inference/models/download') {
              return Promise.resolve({ status: 'downloading' });
            }
            if (path === '/api/v1/models') {
              modelsCallCount++;
              return Promise.resolve({
                active: { embedding: modelsCallCount >= 2 ? 'mxbai-embed-large' : null },
                models_dir: 'E:\\tylluan\\models',
                detected_local_models: [],
              });
            }
            return Promise.resolve({});
          }),
        } as unknown as NexusBridge;

        render(
          <FirstBootConsentSection
            bridge={mockBridge}
            sysStatus={{ embeddings_loaded: false }}
            notify={notify}
            onModelReady={onModelReady}
          />
        );

        fireEvent.click(screen.getByRole('checkbox'));
        const downloadBtn = screen.getByRole('button', { name: /Solicitar Descarga de Modelo/i });
        fireEvent.click(downloadBtn);

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });
        expect(onModelReady).not.toHaveBeenCalled();

        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(onModelReady).toHaveBeenCalledWith('mxbai-embed-large');
        expect(screen.getByText(/¡Modelo descargado y disponible en el kernel!/i)).toBeInTheDocument();
      } finally {
        vi.useRealTimers();
      }
    });
  });
});

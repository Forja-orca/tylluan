import React from 'react';
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { GeneralSettingsPanel } from '../GeneralSettingsPanel';
import { InferenceSettingsPanel } from '../InferenceSettingsPanel';
import { SecuritySettingsPanel } from '../SecuritySettingsPanel';
import { ConnectorsTab } from '../../ConnectorsTab';
import { MaintenanceTab } from '../../MaintenanceTab';
import type { NexusBridge } from '../../../lib/nexus-bridge';

// Mock canvas getContext to avoid JSDOM stderr noise
(HTMLCanvasElement.prototype as unknown as Record<string, unknown>).getContext = vi.fn().mockReturnValue({
  fillRect: vi.fn(),
  clearRect: vi.fn(),
  getImageData: vi.fn(),
  putImageData: vi.fn(),
  createImageData: vi.fn(),
  setTransform: vi.fn(),
  drawImage: vi.fn(),
  save: vi.fn(),
  fillText: vi.fn(),
  restore: vi.fn(),
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  closePath: vi.fn(),
  stroke: vi.fn(),
  translate: vi.fn(),
  scale: vi.fn(),
  rotate: vi.fn(),
  arc: vi.fn(),
  fill: vi.fn(),
  measureText: vi.fn().mockReturnValue({ width: 0 }),
  transform: vi.fn(),
  rect: vi.fn(),
  clip: vi.fn(),
  bezierCurveTo: vi.fn(),
  createLinearGradient: vi.fn().mockReturnValue({ addColorStop: vi.fn() }),
});

class MockResizeObserver {
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
}
(globalThis as unknown as Record<string, unknown>).ResizeObserver = MockResizeObserver;

const mockUseNexusState = {
  bridge: null as unknown as NexusBridge,
  sessions: [] as unknown[],
  loading: false,
  refreshData: vi.fn(),
  sysStatus: {
    system: { cpu_usage: 15, memory_percent: 42, used_memory_mb: 4096, total_memory_mb: 16384 },
  },
};

vi.mock('../../../hooks/useNexus', () => ({
  useNexus: () => mockUseNexusState,
}));

vi.mock('@tylluan/ui-core', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useNexus: () => mockUseNexusState,
  };
});

describe('Ajustes del Sistema — Paneles Unificados A2 (Consumo Mínimo)', () => {
  let mockBridge: { fetchRaw: ReturnType<typeof vi.fn> };
  let mockNotify: Mock<(msg: string, type?: 'error' | 'info') => void>;

  beforeEach(() => {
    mockNotify = vi.fn<(msg: string, type?: 'error' | 'info') => void>();
    mockBridge = {
      fetchRaw: vi.fn<(url: string, init?: RequestInit) => Promise<unknown>>(),
    };
  });

  describe('GeneralSettingsPanel', () => {
    it('realiza una sola carga al montar (0 sondeos) y renderiza identidad y campos protegidos', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            nexus: { profile: 'portable', host: '127.0.0.1', port: 47004 },
            dev_mode: false,
            low_memory_mode: false,
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({
            fields: [
              { key: 'low_memory_mode', editable_via_api: true, requires_restart: false, type: 'boolean' },
              { key: 'host', editable_via_api: false, requires_restart: true, type: 'string', reason: 'solo operador' },
            ],
          });
        }
        if (url === '/health') {
          return Promise.resolve({
            status: 'ok',
            version: '0.17.2',
            commit: 'e62ddb46',
            uptime_secs: 3600,
          });
        }
        return Promise.resolve({});
      });

      render(<GeneralSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByText(/Estado del Daemon Soberano/i)).toBeInTheDocument();
        // Verificamos que se muestre el commit cargado de GET /health
        expect(screen.getByText(/e62ddb46/i)).toBeInTheDocument();
      });

      // Verificamos presencia de campos solo lectura con su razón
      expect(screen.getByText(/127.0.0.1/i)).toBeInTheDocument();
      expect(screen.getByText(/47004/i)).toBeInTheDocument();

      // Cambiar toggle de low_memory_mode activa el botón guardar
      const toggle = screen.getByRole('checkbox', { name: /alternar modo de bajo consumo/i });
      expect(toggle).not.toBeChecked();
      fireEvent.click(toggle);
      expect(toggle).toBeChecked();

      const saveBtn = screen.getByRole('button', { name: /guardar ajustes/i });
      expect(saveBtn).not.toBeDisabled();

      // Simular guardado seguro (POST /validate -> POST /edit con ETag)
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config/validate') {
          return Promise.resolve({ ok: true, dry_run: true, etag: 'etag-abc-123', diff: [] });
        }
        if (url === '/api/v1/config/edit') {
          return Promise.resolve({ ok: true, status: 'applied', backup: 'backup.toml' });
        }
        return Promise.resolve({});
      });

      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/config/validate',
          expect.objectContaining({ method: 'POST' })
        );
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/config/edit',
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('"etag":"etag-abc-123"'),
          })
        );
      });
    });
  });

  describe('InferenceSettingsPanel', () => {
    it('pantalla única de inferencia (SPEC §4): lista densa de proveedores, modelos por uso y drawer lateral Añadir proveedor', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            inference: { device: 'cpu', max_threads: 4, primary_model: 'qwen2.5-1.5b' },
            memory: { embedding_model: 'mxbai-embed-large' },
            silva: { reranker_enabled: false },
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({
            fields: [
              { key: 'inference.device', options: ['cpu', 'directml', 'cuda', 'coreml'], editable_via_api: true, requires_restart: true },
              { key: 'inference.max_threads', min: 1, max: 256, editable_via_api: true, requires_restart: true },
            ],
          });
        }
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: {
              embedding: 'mxbai-embed-large',
              vector_dimensions: 1024,
              inference: 'qwen2.5-1.5b',
              vision: 'models/smolvlm2-256m',
            },
            detected_local_models: [
              { id: 'qwen2.5-1.5b', name: 'Local Qwen 1.5B', size_mb: 2944, role: 'chat', chat_capable: true, installed: true },
              { id: 'smollm2-360m', name: 'Local SmolLM2', size_mb: 720, role: 'chat', chat_capable: true, installed: true },
              { id: 'decima', name: 'decima', size_mb: 500, role: 'decision', chat_capable: false, installed: true },
              { id: 'smolvlm2-256m', name: 'Local SmolVLM2', path: 'models/smolvlm2-256m', size_mb: 256, role: 'vision', chat_capable: false, installed: true },
            ],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({
            valid: true,
            points: {
              decision: { provider: 'local_ephemeral', model: 'decima', cloud_enabled: false },
              grammar_judge: { provider: 'local_ephemeral', model: 'smollm2', cloud_enabled: false },
              sentinel: { provider: 'local_ephemeral', model: 'qwen3guard', cloud_enabled: false },
            },
          });
        }
        if (url === '/api/v1/inference/providers/discover') {
          return Promise.resolve({
            scanned: 3,
            any_detected: true,
            providers: [
              { id: 'lm-studio', label: 'LM Studio', base_url: 'http://127.0.0.1:1234', detected: true, latency_ms: 12, models: ['qwen2.5-coder-7b'] },
              { id: 'llama-cpp', label: 'llama.cpp del usuario', base_url: 'http://127.0.0.1:9000', detected: false },
              { id: 'ollama', label: 'Ollama', base_url: 'http://127.0.0.1:11434', detected: false },
            ],
            configured_external: [],
          });
        }
        if (url === '/api/v1/external-providers') {
          return Promise.resolve([
            {
              name: 'openrouter',
              type: 'openai_compatible',
              base_url: 'https://openrouter.ai/api/v1',
              api_key_env: 'OPENROUTER_API_KEY',
              models: ['gpt-4o-mini', 'claude-3-haiku'],
              api_key_set: true,
            },
          ]);
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByText(/Proveedores de inferencia/i)).toBeInTheDocument();
        expect(screen.getByText(/Modelos por uso/i)).toBeInTheDocument();
        expect(screen.getByText(/Ajustes Avanzados & Motor de Inferencia/i)).toBeInTheDocument();
        expect(screen.getByTestId('provider-row-tylluan')).toBeInTheDocument();
        expect(screen.getByTestId('provider-row-llamacpp')).toBeInTheDocument();
        expect(screen.getByTestId('provider-row-lmstudio')).toBeInTheDocument();
        expect(screen.getByTestId('provider-row-ollama')).toBeInTheDocument();
      });

      // Verificamos los tres botones de la fila llama.cpp
      expect(screen.getByRole('button', { name: /Arrancar llama\.cpp/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Abrir UI web de llama\.cpp/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Abrir carpeta models/i })).toBeInTheDocument();

      // Verificamos las 3 filas de Modelos por uso (SPEC §3 y §4)
      expect(screen.getByText(/^Decision$/i)).toBeInTheDocument();
      expect(screen.getByText(/^Embeddings$/i)).toBeInTheDocument();
      expect(screen.getByText(/^Vision$/i)).toBeInTheDocument();

      // Invariante de arquitectura: 1024 dimensiones
      expect(screen.getAllByTitle(/1024 dimensiones/i).length).toBeGreaterThanOrEqual(1);

      // Verificamos proveedor externo y prueba de latencia (ping)
      expect(screen.getAllByText(/openrouter/i).length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText(/OPENROUTER_API_KEY/i)).toBeInTheDocument();

      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url.startsWith('/api/v1/external-providers/openrouter/test')) {
          return Promise.resolve({
            ok: true,
            status: 'online',
            provider: 'openrouter',
            type: 'openai_compatible',
            base_url: 'https://openrouter.ai/api/v1',
            http_status: 200,
            latency_ms: 142,
          });
        }
        if (url.startsWith('/api/v1/inference/points/decision')) {
          return Promise.resolve({ ok: true });
        }
        return Promise.resolve({});
      });

      const pingBtn = screen.getByRole('button', { name: /comprobar openrouter/i });
      fireEvent.click(pingBtn);

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/external-providers/openrouter/test',
          expect.objectContaining({ method: 'POST' })
        );
        expect(screen.getByText(/142 ms/i)).toBeInTheDocument();
      });

      // Guardar punto de decisión
      const saveDecisionBtn = screen.getByRole('button', { name: /guardar modelo de decisión/i });
      fireEvent.click(saveDecisionBtn);

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/inference/points/decision',
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('"persist":true'),
          })
        );
      });

      // Botón Añadir proveedor habilitado tras la entrega de D-157 en main (SPEC §6)
      const addBtn = screen.getByTestId('add-provider-btn');
      expect(addBtn).not.toBeDisabled();
      expect(addBtn).toHaveTextContent(/añadir proveedor/i);
    });

    it('control de llama.cpp: los 3 botones (Arrancar, Abrir Web UI, Abrir carpeta models) operan correctamente (T3259)', async () => {
      let isRunning = false;
      mockBridge.fetchRaw.mockImplementation((url: string, _opts?: { method?: string }) => {
        if (url === '/api/v1/inference/llama/status') {
          return Promise.resolve({
            running: isRunning,
            port: 9000,
            models_dir: 'E:\\tylluan\\models',
            guild: 'llama_backend',
            guild_running: isRunning,
          });
        }
        if (url === '/api/v1/inference/llama/start') {
          isRunning = true;
          return Promise.resolve({ running: true, port: 9000 });
        }
        if (url === '/api/v1/inference/llama/stop') {
          isRunning = false;
          return Promise.resolve({ running: false, port: 9000 });
        }
        return Promise.resolve({});
      });

      const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null);

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} kernelSupportsAddProvider={true} />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Arrancar llama\.cpp/i })).toBeInTheDocument();
      });

      // 1. Click en 'Arrancar llama.cpp'
      fireEvent.click(screen.getByRole('button', { name: /Arrancar llama\.cpp/i }));
      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith('/api/v1/inference/llama/start', { method: 'POST' });
        expect(screen.getByRole('button', { name: /Detener llama\.cpp/i })).toBeInTheDocument();
      });

      // 2. Click en 'Abrir UI web de llama.cpp'
      fireEvent.click(screen.getByRole('button', { name: /Abrir UI web de llama\.cpp/i }));
      expect(openSpy).toHaveBeenCalledWith('http://127.0.0.1:9000', '_blank', 'noopener,noreferrer');

      // 3. Click en 'Abrir carpeta models'
      const openFolderBtn = screen.getByRole('button', { name: /Abrir carpeta models/i });
      expect(openFolderBtn).toBeInTheDocument();
      fireEvent.click(openFolderBtn);

      openSpy.mockRestore();
    });

    it('muestra el estado en memoria de embeddings y reranker como «cargado / liberado por inactividad» según /api/v1/dashboard/summary (T3806)', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            inference: { device: 'cpu', max_threads: 4 },
            memory: { embedding_model: 'mxbai-embed-large' },
            silva: { reranker_enabled: true },
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({ fields: [] });
        }
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: {
              embedding: 'mxbai-embed-large',
              vector_dimensions: 1024,
              vision: 'models/smolvlm2-256m',
            },
            detected_local_models: [
              { id: 'decima', name: 'decima', role: 'decision', chat_capable: false, installed: true },
              { id: 'smolvlm2-256m', name: 'smolvlm2-256m', role: 'vision', chat_capable: false, installed: true },
            ],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({ points: {} });
        }
        if (url === '/api/v1/external-providers') {
          return Promise.resolve([]);
        }
        if (url === '/api/v1/inference/providers/discover') {
          return Promise.resolve({ providers: [] });
        }
        if (url === '/api/v1/dashboard/summary') {
          return Promise.resolve({
            interoception: {
              capabilities: {
                embeddings_loaded: true,
                reranker_loaded: false,
                dense_stage_ready: true,
                embedding_model: 'mxbai-embed-large-v2-onnx',
                reranker_model: 'jina-reranker-v1-turbo-en',
              },
            },
          });
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        // Embeddings: cargado
        const embSummary = screen.getByTestId('embedding-summary-pill');
        expect(embSummary).toHaveTextContent(/cargado/i);
        const embBadge = screen.getByTestId('embeddings-loaded-badge');
        expect(embBadge).toHaveTextContent(/cargado/i);

        // Reranker: liberado por inactividad
        const rerankSummary = screen.getByTestId('reranker-summary-pill');
        expect(rerankSummary).toHaveTextContent(/liberado por inactividad/i);
        const rerankBadge = screen.getByTestId('reranker-loaded-badge');
        expect(rerankBadge).toHaveTextContent(/liberado por inactividad/i);
      });
    });

    it('sin summary (o sin capabilities) muestra «desconocido» neutro y «—» sin inventar modelos (T3813)', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            inference: { device: 'cpu', max_threads: 4 },
            memory: {},
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({ fields: [] });
        }
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: {
              embedding: undefined,
              vector_dimensions: 1024,
              vision: undefined,
            },
            detected_local_models: [],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({ points: {} });
        }
        if (url === '/api/v1/external-providers') {
          return Promise.resolve([]);
        }
        if (url === '/api/v1/inference/providers/discover') {
          return Promise.resolve({ providers: [] });
        }
        if (url === '/api/v1/dashboard/summary') {
          // Sin campo interoception ni capabilities (T3813 punto a y c)
          return Promise.resolve({});
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        // Pill de SilvaDB: desconectado y estado desconocido (sin inventar mxbai)
        const embSummary = screen.getByTestId('embedding-summary-pill');
        expect(embSummary).toHaveTextContent(/SilvaDB: desconectado/i);
        expect(embSummary).toHaveTextContent(/\(desconocido\)/i);

        // Pill de Reranker: estado desconocido
        const rerankSummary = screen.getByTestId('reranker-summary-pill');
        expect(rerankSummary).toHaveTextContent(/\(desconocido\)/i);

        // Badges en lista de modelos por uso: desconocido
        const embBadge = screen.getByTestId('embeddings-loaded-badge');
        expect(embBadge).toHaveTextContent(/desconocido/i);

        const rerankBadge = screen.getByTestId('reranker-loaded-badge');
        expect(rerankBadge).toHaveTextContent(/desconocido/i);

        // Input de modelo de reranker muestra '—' sin inventar jina
        const rerankInput = screen.getByLabelText(/Modelo para Reranker/i) as HTMLInputElement;
        expect(rerankInput.value).toBe('—');
      });
    });

    it('inventario de modelos vacío con hint y models_dir muestra la ruta absoluta, el texto del hint y botón de copiar ruta (D-162 / T3828)', async () => {
      // Mock de navigator.clipboard
      const writeTextMock = vi.fn().mockResolvedValue(undefined);
      Object.assign(navigator, {
        clipboard: {
          writeText: writeTextMock,
        },
      });

      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            nexus: { port: 47004 },
            inference: { device: 'cpu', max_threads: 4 },
            memory: {},
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({ fields: [] });
        }
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: {
              embedding: 'mxbai-embed-large',
              vector_dimensions: 1024,
              vision: undefined,
            },
            models_dir: 'E:\\tylluan\\models',
            models_dir_from_env: false,
            hint: 'La carpeta de modelos E:\\tylluan\\models existe pero no tiene ningún modelo: copia tus modelos ahí. Para chatear, copia además un modelo de chat en esa carpeta.',
            detected_local_models: [],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({ points: {} });
        }
        if (url === '/api/v1/external-providers') {
          return Promise.resolve([]);
        }
        if (url === '/api/v1/inference/providers/discover') {
          return Promise.resolve({ providers: [] });
        }
        if (url === '/api/v1/dashboard/summary') {
          return Promise.resolve({});
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        // La caja de hint de inventario vacío debe ser visible
        const hintBox = screen.getByTestId('empty-models-hint-box');
        expect(hintBox).toBeInTheDocument();

        // Muestra el texto exacto del hint devuelto por el kernel
        expect(screen.getByTestId('empty-models-hint-text')).toHaveTextContent(
          /La carpeta de modelos E:\\tylluan\\models existe pero no tiene ningún modelo/i
        );
        expect(screen.getByTestId('empty-models-hint-text')).toHaveTextContent(
          /Para chatear, copia además un modelo de chat en esa carpeta/i
        );

        // Muestra la ruta absoluta models_dir
        const pathEl = screen.getByTestId('models-dir-path');
        expect(pathEl).toHaveTextContent('E:\\tylluan\\models');

        // Botón copiar ruta
        const copyBtn = screen.getByTestId('copy-models-dir-btn');
        expect(copyBtn).toHaveTextContent(/Copiar ruta/i);

        // Al hacer click, llama a navigator.clipboard.writeText con la ruta absoluta
        fireEvent.click(copyBtn);
        expect(writeTextMock).toHaveBeenCalledWith('E:\\tylluan\\models');
      });
    });

    it('inventario con modelos detectados no muestra el hint de inventario vacío (T3828)', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            nexus: { port: 47004 },
            inference: { device: 'cpu', max_threads: 4 },
            memory: {},
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({ fields: [] });
        }
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: {
              embedding: 'mxbai-embed-large',
              vector_dimensions: 1024,
              vision: 'models/smolvlm2-256m',
            },
            models_dir: 'E:\\tylluan\\models',
            models_dir_from_env: false,
            hint: null,
            detected_local_models: [
              { id: 'qwen2.5-1.5b', name: 'Local Qwen 1.5B', size_mb: 2944, role: 'chat', chat_capable: true, installed: true },
            ],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({ points: {} });
        }
        if (url === '/api/v1/external-providers') {
          return Promise.resolve([]);
        }
        if (url === '/api/v1/inference/providers/discover') {
          return Promise.resolve({ providers: [] });
        }
        if (url === '/api/v1/dashboard/summary') {
          return Promise.resolve({});
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        // Con modelos presentes, no se debe renderizar el hint de inventario vacío
        expect(screen.queryByTestId('empty-models-hint-box')).not.toBeInTheDocument();
      });
    });

    it('inventario vacío en kernel viejo (sin hint ni models_dir) no inventa rutas ni hints (T3828)', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            nexus: { port: 47004 },
            inference: { device: 'cpu', max_threads: 4 },
            memory: {},
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({ fields: [] });
        }
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: {
              embedding: 'mxbai-embed-large',
              vector_dimensions: 1024,
            },
            // Sin campos models_dir, models_dir_from_env ni hint (kernel pre-D162)
            detected_local_models: [],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({ points: {} });
        }
        if (url === '/api/v1/external-providers') {
          return Promise.resolve([]);
        }
        if (url === '/api/v1/inference/providers/discover') {
          return Promise.resolve({ providers: [] });
        }
        if (url === '/api/v1/dashboard/summary') {
          return Promise.resolve({});
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        // En kernel viejo no se inventa la caja de hint ni rutas falsas
        expect(screen.queryByTestId('empty-models-hint-box')).not.toBeInTheDocument();
        expect(screen.queryByTestId('models-dir-path')).not.toBeInTheDocument();
        expect(screen.queryByTestId('copy-models-dir-btn')).not.toBeInTheDocument();
      });
    });

    it('T4064(a) mutación: sin modelo activo en /api/v1/models la fila Tylluan NO dice «Activo y disponible»', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') return Promise.resolve({ inference: { device: 'cpu', max_threads: 4 } });
        if (url === '/api/v1/config/schema') return Promise.resolve({ fields: [] });
        if (url === '/api/v1/models') {
          return Promise.resolve({ active: {}, detected_local_models: [], models_dir: 'E:\\tylluan\\models' });
        }
        if (url === '/api/v1/inference/points') return Promise.resolve({ points: {} });
        if (url === '/api/v1/external-providers') return Promise.resolve([]);
        if (url === '/api/v1/inference/providers/discover') return Promise.resolve({ providers: [] });
        if (url === '/api/v1/dashboard/summary') return Promise.resolve({});
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByTestId('provider-row-tylluan')).toBeInTheDocument();
      });

      // Sin modelo activo el estado real es neutro (antes: literal fijo «Activo y disponible»)
      expect(screen.queryByText(/Activo y disponible/i)).not.toBeInTheDocument();
      expect(screen.getAllByText(/sin modelo activo/i).length).toBeGreaterThan(0);
    });

    it('T4064(a) con modelo activo y embeddings cargados la fila Tylluan muestra el estado real «Activo y disponible»', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') return Promise.resolve({ inference: { device: 'cpu', max_threads: 4 } });
        if (url === '/api/v1/config/schema') return Promise.resolve({ fields: [] });
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: { embedding: 'mxbai-embed-large', vector_dimensions: 1024 },
            detected_local_models: [],
            models_dir: 'E:\\tylluan\\models',
          });
        }
        if (url === '/api/v1/inference/points') return Promise.resolve({ points: {} });
        if (url === '/api/v1/external-providers') return Promise.resolve([]);
        if (url === '/api/v1/inference/providers/discover') return Promise.resolve({ providers: [] });
        if (url === '/api/v1/dashboard/summary') {
          return Promise.resolve({ interoception: { capabilities: { embeddings_loaded: true } } });
        }
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getAllByText(/Activo y disponible/i).length).toBeGreaterThan(0);
      });
    });

    it('T4064(b) mutación: cambiar el modelo de Visión persiste en /api/v1/config/inference-llama', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') return Promise.resolve({ inference: { device: 'cpu', max_threads: 4 } });
        if (url === '/api/v1/config/schema') return Promise.resolve({ fields: [] });
        if (url === '/api/v1/models') {
          return Promise.resolve({
            active: { embedding: 'mxbai-embed-large', vector_dimensions: 1024, vision: 'E:\\models\\smolvlm2-256m' },
            detected_local_models: [
              { id: 'smolvlm2-256m', name: 'SmolVLM2 256M', path: 'E:\\models\\smolvlm2-256m', role: 'vision' },
              { id: 'vlm-b', name: 'VLM B', path: 'E:\\models\\vlm-b', role: 'vision' },
            ],
            models_dir: 'E:\\tylluan\\models',
          });
        }
        if (url === '/api/v1/inference/points') return Promise.resolve({ points: {} });
        if (url === '/api/v1/external-providers') return Promise.resolve([]);
        if (url === '/api/v1/inference/providers/discover') return Promise.resolve({ providers: [] });
        if (url === '/api/v1/dashboard/summary') return Promise.resolve({});
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      const visionSelect = await screen.findByLabelText('Modelo para Visión');
      fireEvent.change(visionSelect, { target: { value: 'E:\\models\\vlm-b' } });

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/config/inference-llama',
          expect.objectContaining({ method: 'POST' })
        );
      });

      const llamada = mockBridge.fetchRaw.mock.calls.find(c => c[0] === '/api/v1/config/inference-llama');
      expect(llamada).toBeDefined();
      const init = llamada?.[1] as RequestInit;
      expect(String(init.body)).toContain('vision_model');
      expect(String(init.body)).toContain('vlm-b');
    });
  });

  describe('SecuritySettingsPanel', () => {
    it('muestra estado no-alarmista de cadena de auditoría (D-95), matriz ACL (D-96) y aviso de inmutabilidad', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/audit/verify') {
          return Promise.resolve({
            ok: true,
            status: 'broken',
            valid_count: 8928,
            hash_inconsistent_count: 0,
            link_break_count: 25,
            link_break_ids: [2577, 2615],
            message: 'Chain has 25 problem row(s)',
          });
        }
        if (url === '/api/v1/config') {
          return Promise.resolve({
            security: {
              acl: {
                default_role: 'admin',
                roles: {
                  admin: ['*'],
                  reader: ['knowledge', 'coloquio'],
                  writer: ['knowledge', 'filesystem'],
                },
                tokens_count: 2,
                token_agent_bindings_count: 1,
              },
              encrypt_at_rest: false,
              sentinel: { enabled: true },
              sandbox: { enabled: false },
            },
          });
        }
        return Promise.resolve({});
      });

      render(<SecuritySettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByText(/Cadena de Auditoría Criptográfica/i)).toBeInTheDocument();
        // Insignia no alarmista en ámbar suave: RE-SINCRONIZADA
        expect(screen.getByText(/RE-SINCRONIZADA/i)).toBeInTheDocument();
      });

      expect(screen.getByText(/saltos de secuencia/i)).toBeInTheDocument();

      // Matriz ACL
      expect(screen.getAllByText(/admin/i).length).toBeGreaterThan(0);
      expect(screen.getByText(/reader/i)).toBeInTheDocument();
      expect(screen.getByText(/writer/i)).toBeInTheDocument();
      expect(screen.getByText(/Tokens Bearer Configurados/i)).toBeInTheDocument();
      expect(screen.getByText('2')).toBeInTheDocument();

      // Aviso de inmutabilidad
      expect(screen.getByText(/Seguridad Fail-Closed e Inmutabilidad por Interfaz/i)).toBeInTheDocument();
    });
  });

  describe('ConnectorsTab (A3)', () => {
    it('renderiza servidores MCP externos con variables redactadas y maneja tool_collisions presentes', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/mcp/external') {
          return Promise.resolve([
            {
              name: 'gitkraken',
              active: true,
              running: true,
              command: 'gk',
              args: ['mcp', '--scheme=tylluan'],
              env: { API_KEY: '***', SECRET_SOURCE: '***' },
              tools: [{ name: 'git_status' }],
            },
          ]);
        }
        if (url === '/api/v1/setup-hint') {
          return Promise.resolve({});
        }
        return Promise.resolve({});
      });

      const mockGetCapabilities = vi.fn().mockResolvedValue({
        status: 'ok',
        version: '0.17.2',
        sovereign_contract: { tools: [] },
        guilds: [],
        all_guild_tools: [],
        mcp: { prompts: [], resources: [] },
        sessions: [],
        tool_collisions: [
          {
            name: 'analyze_code',
            kept_server: 'codebase_memory',
            shadowed_servers: ['gitkraken'],
          },
        ],
      });

      const fullBridge = {
        ...mockBridge,
        getCapabilities: mockGetCapabilities,
        getBaseUrl: () => 'http://localhost:47004',
        sessions: [],
        loading: false,
        refreshData: vi.fn(),
      };

      mockUseNexusState.bridge = fullBridge as unknown as NexusBridge;

      render(<ConnectorsTab notify={mockNotify} />);

      // Switch to external MCP tab
      await waitFor(() => {
        expect(screen.getByRole('tab', { name: /servidores mcp externos/i })).toBeInTheDocument();
      });

      const extTab = screen.getByRole('tab', { name: /servidores mcp externos/i });
      fireEvent.click(extTab);

      // Verify tool collisions banner
      expect(screen.getByText(/Colisiones de Herramientas MCP Detectadas/i)).toBeInTheDocument();
      expect(screen.getByText('analyze_code')).toBeInTheDocument();
      expect(screen.getByText('codebase_memory')).toBeInTheDocument();
      expect(screen.getAllByText('gitkraken').length).toBeGreaterThanOrEqual(1);

      // Verify external server details and masked env
      expect(screen.getByText(/API_KEY/i)).toBeInTheDocument();
      expect(screen.getAllByText(/\*\*\*/i).length).toBeGreaterThan(0);
      expect(screen.getByText(/Protegido A0/i)).toBeInTheDocument();
    });

    it('degrada limpiamente cuando tool_collisions no está presente en capabilities', async () => {
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/mcp/external') {
          return Promise.resolve([]);
        }
        return Promise.resolve({});
      });

      const mockGetCapabilities = vi.fn().mockResolvedValue({
        status: 'ok',
        version: '0.17.2',
        sovereign_contract: { tools: [] },
        guilds: [],
        all_guild_tools: [],
        mcp: { prompts: [], resources: [] },
        sessions: [],
        // tool_collisions is omitted intentionally
      });

      const fullBridge = {
        ...mockBridge,
        getCapabilities: mockGetCapabilities,
        getBaseUrl: () => 'http://localhost:47004',
        sessions: [],
        loading: false,
        refreshData: vi.fn(),
      };

      mockUseNexusState.bridge = fullBridge as unknown as NexusBridge;

      render(<ConnectorsTab notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByRole('tab', { name: /servidores mcp externos/i })).toBeInTheDocument();
      });

      // No collision banner should be present and no crash
      expect(screen.queryByText(/Colisiones de Herramientas MCP Detectadas/i)).not.toBeInTheDocument();
    });
  });

  describe('MaintenanceTab (A3)', () => {
    it('muestra métricas de storage, no contiene botones prohibidos de VACUUM/ONNX y permite editar parámetros', async () => {
      const fullBridge = {
        ...mockBridge,
        maintenance_status: vi.fn().mockResolvedValue({
          status: 'ok',
          brain_size_bytes: 1048576,
          brain_size_human: '1.00 MB',
          last_export: '2026-11-03 10:00:00',
          storage_mode: 'SQLite WAL',
          node_count: 42,
          edge_count: 84,
          orphan_node_count: 0,
        }),
        probe: vi.fn().mockResolvedValue({
          kernel_version: '0.17.2',
          port: 47004,
          detected_dialect: 'standard',
        }),
        getMetricsHistory: vi.fn().mockResolvedValue({
          snapshots: [{ timestamp: 1, cpu: 12, mem: 45 }],
        }),
      };

      mockUseNexusState.bridge = fullBridge as unknown as NexusBridge;

      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            memory: {
              background_concurrency: 4,
              background_max_wait_secs: 60,
              reindex_node_delay_ms: 200,
              reindex_nodes_per_tick: 100,
              reindex_tick_secs: 600,
            },
            logging: {
              max_file_size_mb: 50,
              max_files: 5,
            },
          });
        }
        if (url === '/api/v1/config/schema') {
          return Promise.resolve({
            fields: [
              { key: 'memory.background_concurrency', section: 'maintenance', editable_via_api: true },
            ],
          });
        }
        return Promise.resolve({});
      });

      render(<MaintenanceTab bridge={fullBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByText(/Mantenimiento del Sustrato Soberano/i)).toBeInTheDocument();
        // Verificamos que se muestren las métricas de memoria cargadas asíncronamente
        expect(screen.getByText('1.00 MB')).toBeInTheDocument();
        expect(screen.getByText('42')).toBeInTheDocument();
      });

      // Verificamos que VACUUM y ONNX no tengan botones ejecutables activos (Turn #2562)
      expect(screen.queryByRole('button', { name: /ejecutar vacuum/i })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /purgar caché onnx/i })).not.toBeInTheDocument();
      expect(screen.getByText(/Compactación SQLite \(VACUUM\)/i)).toBeInTheDocument();
      expect(screen.getByText(/No disponible por API/i)).toBeInTheDocument();

      // Cambiamos parámetro editable
      await waitFor(() => {
        expect(screen.getByLabelText(/hilos de fondo/i)).toHaveValue(4);
      });
      const concurrencyInput = screen.getByLabelText(/hilos de fondo/i);
      fireEvent.change(concurrencyInput, { target: { value: '8' } });
      expect(concurrencyInput).toHaveValue(8);

      const saveBtn = screen.getByRole('button', { name: /guardar parámetros/i });
      expect(saveBtn).not.toBeDisabled();

      // Simular guardado seguro
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config/validate') {
          return Promise.resolve({ ok: true, dry_run: true, etag: 'etag-maint-456', diff: [] });
        }
        if (url === '/api/v1/config/edit') {
          return Promise.resolve({ ok: true, status: 'applied', backup: 'backup_maint.toml' });
        }
        return Promise.resolve({});
      });

      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/config/validate',
          expect.objectContaining({ method: 'POST' })
        );
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/config/edit',
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('"etag":"etag-maint-456"'),
          })
        );
      });
    });

    it('lee requires_restart del backend y avisa "Guardado, aplica al reiniciar" con banner accesible (H2 / AG-23)', async () => {
      const fullBridge = {
        ...mockBridge,
        maintenance_status: vi.fn().mockResolvedValue({
          status: 'ok',
          brain_size_bytes: 1048576,
          brain_size_human: '1.00 MB',
          last_export: '2026-11-03 10:00:00',
          storage_mode: 'SQLite WAL',
          node_count: 42,
          edge_count: 84,
          orphan_node_count: 0,
        }),
        probe: vi.fn().mockResolvedValue({
          kernel_version: '0.17.2',
          port: 47004,
          detected_dialect: 'standard',
        }),
        getMetricsHistory: vi.fn().mockResolvedValue({
          snapshots: [{ timestamp: 1, cpu: 12, mem: 45 }],
        }),
      };

      mockUseNexusState.bridge = fullBridge as unknown as NexusBridge;

      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config') {
          return Promise.resolve({
            memory: {
              background_concurrency: 2,
              background_max_wait_secs: 60,
              reindex_node_delay_ms: 200,
              reindex_nodes_per_tick: 100,
              reindex_tick_secs: 600,
            },
            logging: {
              max_file_size_mb: 50,
              max_files: 5,
            },
          });
        }
        return Promise.resolve({});
      });

      render(<MaintenanceTab bridge={fullBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByLabelText(/hilos de fondo/i)).toHaveValue(2);
      });

      const concurrencyInput = screen.getByLabelText(/hilos de fondo/i);
      fireEvent.change(concurrencyInput, { target: { value: '6' } });
      expect(concurrencyInput).toHaveValue(6);

      // Simular respuesta del backend donde requires_restart = true
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url === '/api/v1/config/validate') {
          return Promise.resolve({
            ok: true,
            dry_run: true,
            etag: 'etag-restart-789',
            requires_restart: true,
            diff: [{ key: 'memory.background_concurrency', old: 2, new: 6 }],
          });
        }
        if (url === '/api/v1/config/edit') {
          return Promise.resolve({
            ok: true,
            status: 'applied',
            requires_restart: true,
            backup: 'backup_restart.toml',
          });
        }
        return Promise.resolve({});
      });

      const saveBtn = screen.getByRole('button', { name: /guardar parámetros/i });
      fireEvent.click(saveBtn);

      await waitFor(() => {
        // Verifica el aviso textual exigido por el TL: 'Guardado, aplica al reiniciar'
        expect(screen.getByText(/Guardado, aplica al reiniciar/i)).toBeInTheDocument();
        // Verifica que se notifique al usuario
        expect(mockNotify).toHaveBeenCalledWith('Guardado, aplica al reiniciar', 'info');
      });

      // Verifica el banner de alerta accesible con role="status" y aria-live="polite"
      const banners = screen.getAllByRole('status');
      const restartBanner = banners.find((b) => b.textContent?.includes('Reinicio del Kernel Requerido'));
      expect(restartBanner).toBeDefined();
      expect(restartBanner).toHaveAttribute('aria-live', 'polite');
      expect(restartBanner).toHaveTextContent(/Reinicio del Kernel Requerido/i);

      // El botón 'Entendido' para cerrar el banner tiene focus-visible
      const dismissBtn = screen.getByRole('button', { name: /entendido/i });
      expect(dismissBtn.className).toContain('focus-visible:ring-2');
      fireEvent.click(dismissBtn);

      // El banner se cierra tras confirmación
      expect(screen.queryByText(/Reinicio del Kernel Requerido/i)).not.toBeInTheDocument();
    });
  });
});

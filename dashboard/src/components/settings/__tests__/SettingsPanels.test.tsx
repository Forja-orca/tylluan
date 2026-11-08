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
    it('absorbe Model Hub: renderiza Capa 1 (Motor/Hardware), Capa 2 (Inventario en disco), Capa 3 (Puntos cognitivos T-DSH-116), Capa 4 (Proveedores externos con ping de latencia) y Capa 5 (Soberanía)', async () => {
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
            active: { embedding: 'mxbai-embed-large', vector_dimensions: 1024, inference: 'qwen2.5-1.5b' },
            detected_local_models: [
              { id: 'qwen2.5-1.5b', name: 'Local Qwen 1.5B', size_mb: 2944, model_type: 'generative' },
              { id: 'smollm2-360m', name: 'Local SmolLM2', size_mb: 720, model_type: 'generative' },
            ],
          });
        }
        if (url === '/api/v1/inference/points') {
          return Promise.resolve({
            valid: true,
            points: {
              chat: { provider: 'local_ephemeral', model: 'qwen2.5-1.5b', cloud_enabled: false },
              decision: { provider: 'local_ephemeral', model: 'qwen2.5-1.5b', cloud_enabled: false },
              grammar_judge: { provider: 'local_ephemeral', model: 'smollm2', cloud_enabled: false },
              sentinel: { provider: 'local_ephemeral', model: 'qwen3guard', cloud_enabled: false },
            },
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
        expect(screen.getByText(/Capa 1 — Motor de Inferencia y Aceleración Local/i)).toBeInTheDocument();
        expect(screen.getByText(/Capa 2 — Inventario Físico de Modelos en Disco/i)).toBeInTheDocument();
        expect(screen.getByText(/Capa 3 — Matriz de Enrutamiento Cognitivo/i)).toBeInTheDocument();
        expect(screen.getByText(/Capa 4 — Proveedores Externos y Telemetría de Latencia/i)).toBeInTheDocument();
        expect(screen.getByText(/Capa 5 — Política de Resiliencia y Fallback Soberano/i)).toBeInTheDocument();
      });

      // Verificamos los 4 puntos canónicos T-DSH-116
      expect(screen.getByText(/Chat y Diálogo/i)).toBeInTheDocument();
      expect(screen.getByText(/Decisión y Consenso/i)).toBeInTheDocument();
      expect(screen.getByText(/Juez Gramatical/i)).toBeInTheDocument();
      expect(screen.getByText(/Centinela de Coherencia/i)).toBeInTheDocument();

      // Invariante de arquitectura
      expect(screen.getByText(/1024 dimensiones/i)).toBeInTheDocument();

      // Capa 2: Verificamos inventario en disco
      expect(screen.getAllByText(/Local Qwen 1.5B/i).length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByText(/2944 MB/i).length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText(/Modelo Primario Activo/i)).toBeInTheDocument();
      expect(screen.getAllByText(/Local SmolLM2/i).length).toBeGreaterThanOrEqual(1);

      // Capa 4: Verificamos proveedor externo y prueba de latencia (ping)
      expect(screen.getByRole('heading', { name: /openrouter/i })).toBeInTheDocument();
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
        if (url.startsWith('/api/v1/inference/points/chat')) {
          return Promise.resolve({ ok: true });
        }
        return Promise.resolve({});
      });

      const pingBtn = screen.getByRole('button', { name: /comprobar latencia/i });
      fireEvent.click(pingBtn);

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/external-providers/openrouter/test',
          expect.objectContaining({ method: 'POST' })
        );
        expect(screen.getByText(/142 ms/i)).toBeInTheDocument();
      });

      // Guardar un punto cognitivo invoca POST /api/v1/inference/points/{point} con persist: true
      const saveChatBtn = screen.getByRole('button', { name: /guardar chat/i });
      fireEvent.click(saveChatBtn);

      await waitFor(() => {
        expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
          '/api/v1/inference/points/chat',
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('"persist":true'),
          })
        );
      });
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

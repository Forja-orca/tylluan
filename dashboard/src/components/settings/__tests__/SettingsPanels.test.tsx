import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { GeneralSettingsPanel } from '../GeneralSettingsPanel';
import { InferenceSettingsPanel } from '../InferenceSettingsPanel';
import { SecuritySettingsPanel } from '../SecuritySettingsPanel';
import type { NexusBridge } from '../../../lib/nexus-bridge';

describe('Ajustes del Sistema — Paneles Unificados A2 (Consumo Mínimo)', () => {
  let mockBridge: { fetchRaw: ReturnType<typeof vi.fn> };
  let mockNotify: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockNotify = vi.fn();
    mockBridge = {
      fetchRaw: vi.fn(),
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
      });

      // Verificamos que se muestre el commit cargado de GET /health
      expect(screen.getByText(/e62ddb46/i)).toBeInTheDocument();

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
    it('absorbe Model Hub: renderiza Capa 1 (Motor/Hardware), Capa 2 (Puntos cognitivos T-DSH-116) y Capa 3 (Soberanía)', async () => {
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
            active: { embedding: 'mxbai-embed-large', vector_dimensions: 1024 },
            detected_local_models: [{ id: 'qwen2.5-1.5b', name: 'Local Qwen 1.5B', size_mb: 2944 }],
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
        return Promise.resolve({});
      });

      render(<InferenceSettingsPanel bridge={mockBridge as unknown as NexusBridge} notify={mockNotify} />);

      await waitFor(() => {
        expect(screen.getByText(/Capa 1 — Motor de Inferencia y Aceleración Local/i)).toBeInTheDocument();
        expect(screen.getByText(/Capa 2 — Matriz de Enrutamiento Cognitivo/i)).toBeInTheDocument();
        expect(screen.getByText(/Capa 3 — Política de Resiliencia y Fallback Soberano/i)).toBeInTheDocument();
      });

      // Verificamos los 4 puntos canónicos T-DSH-116
      expect(screen.getByText(/Chat y Diálogo/i)).toBeInTheDocument();
      expect(screen.getByText(/Decisión y Consenso/i)).toBeInTheDocument();
      expect(screen.getByText(/Juez Gramatical/i)).toBeInTheDocument();
      expect(screen.getByText(/Centinela de Coherencia/i)).toBeInTheDocument();

      // Invariante de arquitectura
      expect(screen.getByText(/1024 dimensiones/i)).toBeInTheDocument();

      // Guardar un punto cognitivo invoca POST /api/v1/inference/points/{point} con persist: true
      mockBridge.fetchRaw.mockImplementation((url: string) => {
        if (url.startsWith('/api/v1/inference/points/chat')) {
          return Promise.resolve({ ok: true });
        }
        return Promise.resolve({});
      });

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
      });

      // Insignia no alarmista en ámbar suave: RE-SINCRONIZADA
      expect(screen.getByText(/RE-SINCRONIZADA/i)).toBeInTheDocument();
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
});

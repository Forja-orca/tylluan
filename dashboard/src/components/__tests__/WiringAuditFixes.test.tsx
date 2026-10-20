import React from 'react';
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { deleteNode } from '../../lib/api/memory';
import { WelcomeEmptyState } from '../WelcomeEmptyState';

// Mock useNexus hook for WelcomeEmptyState tests
vi.mock('../../hooks/useNexus', () => ({
  useNexus: vi.fn(() => ({
    agentProfiles: [{ agent_id: 'claude-code', first_seen: '2026-07-19', total_calls: 10, competencies: {} }],
    bridge: {
      fetchRaw: vi.fn().mockResolvedValue({
        mcp_clients: {
          claude_desktop: {
            config: {
              mcpServers: {
                tylluan: { url: 'http://127.0.0.1:47004/api/v1/events' }
              }
            }
          }
        }
      })
    },
    sessions: [
      {
        id: 'sess-123',
        client_name: 'Claude Desktop',
        agent_id: 'claude-code',
        tool_count: 5,
        last_intent: null,
        last_guild: null,
        last_active_unix: 1776673000,
        created_unix: 1776670000,
      }
    ]
  }))
}));

describe('LOTE AF — Auditoría de Cableado y Correcciones Frontend', () => {
  describe('Corrección 1: deleteNode path routing (routes.rs:49)', () => {
    it('invoca /api/v1/silva/nodes/{nodeId} con método DELETE (plural, coincidente con Axum)', async () => {
      const mockFetch = vi.fn().mockResolvedValue({ success: true });
      const client = {
        fetch: mockFetch,
        fetchRaw: vi.fn(),
      };

      await deleteNode(client, 'node-xyz-789');

      expect(mockFetch).toHaveBeenCalledTimes(1);
      expect(mockFetch).toHaveBeenCalledWith('/api/v1/silva/nodes/node-xyz-789', {
        method: 'DELETE',
      });
      // Verifica que NO se llama a la ruta rota singular /node/
      expect(mockFetch).not.toHaveBeenCalledWith('/api/v1/silva/node/node-xyz-789', expect.anything());
    });
  });

  describe('Corrección 2: WelcomeEmptyState MCP Sessions Detection (Categoría c)', () => {
    it('detecta sesiones activas de MCP desde useNexus cuando props.sessions está vacío', () => {
      render(
        <WelcomeEmptyState
          bridge={null}
          sysStatus={{ silva_healthy: true, embeddings_loaded: true }}
          sessions={[]}
          memoryStats={null}
          notify={vi.fn()}
          onRefresh={vi.fn()}
        />
      );

      // El paso de MCP debe marcarse como completado con line-through
      const mcpTask = screen.getByText('Connect MCP Client');
      expect(mcpTask).toBeInTheDocument();
      expect(mcpTask.className).toContain('line-through');
    });
  });
});

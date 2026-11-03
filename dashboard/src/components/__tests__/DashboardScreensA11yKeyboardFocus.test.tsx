import { fireEvent, render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { VisualControlRoom } from '../VisualControlRoom';
import { MemoryConsolidated } from '../MemoryConsolidated';
import { ModelsTab } from '../ModelsTab';
import { SettingsTab } from '../SettingsTab';
import { IngestPanel } from '../IngestPanel';

vi.mock('../../hooks/useNexus', () => ({
  useNexus: () => ({
    bridge: {
      fetchRaw: vi.fn().mockResolvedValue({}),
      getCapabilities: vi.fn().mockResolvedValue({
        sovereign_contract: { tools: [] },
        guilds: [],
        all_guild_tools: [],
        mcp: { prompts: [], resources: [] },
        sessions: [],
      }),
      revokeSession: vi.fn().mockResolvedValue({}),
      getBaseUrl: () => 'http://127.0.0.1:47004',
    },
    sessions: [],
    loading: false,
    refreshData: vi.fn(),
    events: [],
    sysStatus: {},
  }),
}));

vi.mock('../hooks/useNexus', () => ({
  useNexus: () => ({
    bridge: {
      fetchRaw: vi.fn().mockResolvedValue({}),
      getCapabilities: vi.fn().mockResolvedValue({
        sovereign_contract: { tools: [] },
        guilds: [],
        all_guild_tools: [],
        mcp: { prompts: [], resources: [] },
        sessions: [],
      }),
      revokeSession: vi.fn().mockResolvedValue({}),
      getBaseUrl: () => 'http://127.0.0.1:47004',
    },
    sessions: [],
    loading: false,
    refreshData: vi.fn(),
    events: [],
    sysStatus: {},
  }),
}));

vi.mock('../../hooks/usePolling', () => ({
  usePolling: () => undefined,
}));

vi.mock('../hooks/usePolling', () => ({
  usePolling: () => undefined,
}));

vi.mock('../ModelsLocalInference', () => ({
  ModelsLocalInference: () => <div data-testid="mock-models-local-inference">Inferencia Local Content</div>,
}));

vi.mock('../ModelsRoleAssignment', () => ({
  ModelsRoleAssignment: () => <div data-testid="mock-models-role-assignment">Asignacion Roles Content</div>,
}));

vi.mock('../ModelsExternalProviders', () => ({
  ModelsExternalProviders: () => <div data-testid="mock-models-external-providers">Proveedores Externos Content</div>,
}));

vi.mock('../ModelsEmbeddingsDisk', () => ({
  ModelsEmbeddingsDisk: () => <div data-testid="mock-models-embeddings-disk">Embeddings Content</div>,
}));

describe('Dashboard Screens — Keyboard & Focus Accessibility (AG-27)', () => {
  describe('VisualControlRoom Tablist Keyboard Navigation', () => {
    it('supports ArrowRight, ArrowLeft, Home, and End across view tabs', () => {
      render(<VisualControlRoom bridge={null} online={false} />);

      const tablist = screen.getByRole('tablist', { name: /vistas de la sala de control visual/i });
      expect(tablist).toBeInTheDocument();

      const ringsTab = screen.getByRole('tab', { name: /1\. anillos de estado/i });
      const flowTab = screen.getByRole('tab', { name: /2\. mapa de flujo/i });
      const heatmapTab = screen.getByRole('tab', { name: /3\. mapa de calor/i });

      expect(ringsTab).toHaveAttribute('aria-selected', 'true');
      expect(ringsTab).toHaveAttribute('tabIndex', '0');
      expect(flowTab).toHaveAttribute('aria-selected', 'false');
      expect(flowTab).toHaveAttribute('tabIndex', '-1');

      // ArrowRight: rings -> flow
      ringsTab.focus();
      fireEvent.keyDown(ringsTab, { key: 'ArrowRight' });
      expect(flowTab).toHaveAttribute('aria-selected', 'true');
      expect(flowTab).toHaveAttribute('tabIndex', '0');
      expect(document.activeElement).toBe(flowTab);

      // ArrowRight: flow -> heatmap
      fireEvent.keyDown(flowTab, { key: 'ArrowRight' });
      expect(heatmapTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(heatmapTab);

      // ArrowLeft: heatmap -> flow
      fireEvent.keyDown(heatmapTab, { key: 'ArrowLeft' });
      expect(flowTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(flowTab);

      // Home: jump to first tab (rings)
      fireEvent.keyDown(flowTab, { key: 'Home' });
      expect(ringsTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(ringsTab);

      // End: jump to last tab (heatmap)
      fireEvent.keyDown(ringsTab, { key: 'End' });
      expect(heatmapTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(heatmapTab);
    });
  });

  describe('MemoryConsolidated Tablist Keyboard Navigation', () => {
    it('supports ArrowRight, ArrowLeft, Home, and End across SilvaDB memory tabs', () => {
      render(
        <MemoryConsolidated
          bridge={null}
          notify={vi.fn()}
          memoryStats={null}
        />
      );

      const tablist = screen.getByRole('tablist', { name: /vistas de memoria silvadb/i });
      expect(tablist).toBeInTheDocument();

      const graphTab = screen.getByRole('tab', { name: /grafo de conocimiento/i });
      const nodesTab = screen.getByRole('tab', { name: /nodos de la flota/i });
      const contradictionsTab = screen.getByRole('tab', { name: /contradicciones cognitivas/i });

      expect(graphTab).toHaveAttribute('aria-selected', 'true');
      expect(graphTab).toHaveAttribute('tabIndex', '0');

      // ArrowRight: graph -> nodes
      graphTab.focus();
      fireEvent.keyDown(graphTab, { key: 'ArrowRight' });
      expect(nodesTab).toHaveAttribute('aria-selected', 'true');
      expect(nodesTab).toHaveAttribute('tabIndex', '0');
      expect(document.activeElement).toBe(nodesTab);

      // ArrowRight: nodes -> contradictions
      fireEvent.keyDown(nodesTab, { key: 'ArrowRight' });
      expect(contradictionsTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(contradictionsTab);

      // Home: jump back to graph
      fireEvent.keyDown(contradictionsTab, { key: 'Home' });
      expect(graphTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(graphTab);
    });
  });

  describe('ModelsTab Keyboard Navigation & Alertdialog Focus', () => {
    it('supports Arrow key navigation between model section tabs', () => {
      render(<ModelsTab bridge={null} />);

      const tablist = screen.getByRole('tablist', { name: /secciones de modelos/i });
      expect(tablist).toBeInTheDocument();

      const inferenceTab = screen.getByRole('tab', { name: /inferencia local/i });
      const rolesTab = screen.getByRole('tab', { name: /asignación de roles/i });
      const externalTab = screen.getByRole('tab', { name: /proveedores externos/i });
      const embeddingsTab = screen.getByRole('tab', { name: /embeddings y disco/i });

      expect(inferenceTab).toHaveAttribute('aria-selected', 'true');
      expect(inferenceTab).toHaveAttribute('tabIndex', '0');

      // ArrowRight: inference -> roles
      inferenceTab.focus();
      fireEvent.keyDown(inferenceTab, { key: 'ArrowRight' });
      expect(rolesTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(rolesTab);

      // ArrowRight: roles -> external
      fireEvent.keyDown(rolesTab, { key: 'ArrowRight' });
      expect(externalTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(externalTab);

      // End: jump to embeddings
      fireEvent.keyDown(externalTab, { key: 'End' });
      expect(embeddingsTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(embeddingsTab);
    });
  });

  describe('SettingsTab Tablist Navigation', () => {
    it('supports Arrow navigation and focus rings across the 5 settings tabs', () => {
      const mockOpenAuth = vi.fn();
      render(
        <SettingsTab
          bridge={null}
          notify={vi.fn()}
          onOpenAuthModal={mockOpenAuth}
        />
      );

      const tablist = screen.getByRole('tablist', { name: /secciones de ajustes/i });
      expect(tablist).toBeInTheDocument();

      const generalTab = screen.getByRole('tab', { name: /^general$/i });
      const inferenceTab = screen.getByRole('tab', { name: /^inferencia$/i });
      const securityTab = screen.getByRole('tab', { name: /^seguridad$/i });
      const connectorsTab = screen.getByRole('tab', { name: /^conectores$/i });
      const maintenanceTab = screen.getByRole('tab', { name: /^mantenimiento$/i });

      expect(generalTab).toHaveAttribute('aria-selected', 'true');

      // ArrowRight switches to inference
      generalTab.focus();
      fireEvent.keyDown(generalTab, { key: 'ArrowRight' });
      expect(inferenceTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(inferenceTab);

      // ArrowRight switches to security
      fireEvent.keyDown(inferenceTab, { key: 'ArrowRight' });
      expect(securityTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(securityTab);

      // End switches to maintenance
      fireEvent.keyDown(securityTab, { key: 'End' });
      expect(maintenanceTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(maintenanceTab);

      // Home switches to general
      fireEvent.keyDown(maintenanceTab, { key: 'Home' });
      expect(generalTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(generalTab);

      // Configurar Token button has focus-visible ring
      const tokenBtn = screen.getByRole('button', { name: /configurar token/i });
      expect(tokenBtn.className).toContain('focus-visible:ring-2');
    });
  });

  describe('IngestPanel Collapsible, Modes & Keyboard Upload', () => {
    it('collapses on Escape key returning focus to toggle button', () => {
      render(
        <IngestPanel
          bridge={null}
          notify={vi.fn()}
        />
      );

      const toggleBtn = screen.getByRole('button', { name: /ingesta de conocimiento en silvadb/i });
      expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');

      // Open panel
      fireEvent.click(toggleBtn);
      expect(toggleBtn).toHaveAttribute('aria-expanded', 'true');
      expect(screen.getByRole('tablist', { name: /modo de ingesta/i })).toBeInTheDocument();

      // Press Escape inside panel
      const textarea = screen.getByPlaceholderText(/pegue texto, hechos o conocimiento/i);
      textarea.focus();
      fireEvent.keyDown(textarea, { key: 'Escape' });

      // Panel collapses and focus returns to toggle button
      expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');
      expect(document.activeElement).toBe(toggleBtn);
    });

    it('supports arrow navigation across Ingest modes (Texto, Enlace URL, Archivo)', () => {
      render(
        <IngestPanel
          bridge={null}
          notify={vi.fn()}
        />
      );

      const toggleBtn = screen.getByRole('button', { name: /ingesta de conocimiento en silvadb/i });
      fireEvent.click(toggleBtn);

      const textTab = screen.getByRole('tab', { name: /texto/i });
      const urlTab = screen.getByRole('tab', { name: /enlace url/i });
      const fileTab = screen.getByRole('tab', { name: /archivo/i });

      expect(textTab).toHaveAttribute('aria-selected', 'true');

      // ArrowRight: text -> url
      textTab.focus();
      fireEvent.keyDown(textTab, { key: 'ArrowRight' });
      expect(urlTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(urlTab);

      // ArrowRight: url -> file
      fireEvent.keyDown(urlTab, { key: 'ArrowRight' });
      expect(fileTab).toHaveAttribute('aria-selected', 'true');
      expect(document.activeElement).toBe(fileTab);

      // File dropzone is keyboard-focusable with role="button" and tabindex="0"
      const dropzone = screen.getByRole('button', { name: /cargar archivo para ingesta/i });
      expect(dropzone).toHaveAttribute('tabIndex', '0');
      expect(dropzone.className).toContain('focus-visible:ring-2');
    });
  });
});

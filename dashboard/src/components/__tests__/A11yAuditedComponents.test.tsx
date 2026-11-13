import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { NexusBridge } from '../../lib/nexus-bridge';
import { GeneralSettingsPanel } from '../settings/GeneralSettingsPanel';
import { SecuritySettingsPanel } from '../settings/SecuritySettingsPanel';
import { EngineAdvancedAccordion } from '../settings/inference/EngineAdvancedAccordion';
import { ProviderSelectorCards } from '../settings/inference/ProviderSelectorCards';

describe('DASH-A11Y-AUDIT (P2): Componentes de Settings e Inferencia Auditados', () => {
  describe('Fallo 1: GeneralSettingsPanel — Switch de Bajo Consumo (Focus Visible)', () => {
    it('proporciona un indicador de foco visible (peer-focus-visible:ring-2) en el switch accesible', async () => {
      const mockBridge = {
        fetchRaw: vi.fn().mockImplementation((path: string) => {
          if (path === '/api/v1/config') return Promise.resolve({ low_memory_mode: false });
          if (path === '/api/v1/config/schema') return Promise.resolve({ fields: [] });
          if (path === '/health') return Promise.resolve({ status: 'ok', version: '0.17.2' });
          return Promise.resolve({});
        }),
      } as unknown as NexusBridge;

      const { container } = render(
        <GeneralSettingsPanel bridge={mockBridge} notify={vi.fn()} />
      );

      const checkbox = await screen.findByRole('checkbox', { name: /alternar modo de bajo consumo/i });
      expect(checkbox).toBeDefined();

      // El elemento decorativo adyacente debe contener los anillos peer-focus-visible para WCAG 2.4.7
      const visualTrack = container.querySelector('.peer-focus-visible\\:ring-2');
      expect(visualTrack).not.toBeNull();
      expect(visualTrack?.className).toContain('peer-focus-visible:ring-primary');
    });
  });

  describe('Fallo 3: SecuritySettingsPanel — Botón de Saltos de Secuencia (Disclosure WAI-ARIA)', () => {
    it('dispone de aria-expanded, aria-controls y focus-visible:ring-2 en el botón de detalles', async () => {
      const mockBridge = {
        fetchRaw: vi.fn().mockImplementation((path: string) => {
          if (path === '/api/v1/audit/verify') {
            return Promise.resolve({
              ok: true,
              status: 'broken',
              valid_count: 100,
              hash_inconsistent_count: 0,
              link_break_count: 3,
              link_break_ids: [10, 20, 30],
              message: 'ok',
            });
          }
          if (path === '/api/v1/config') {
            return Promise.resolve({ security: { acl: { default_role: 'admin', roles: {} } } });
          }
          return Promise.resolve({});
        }),
      } as unknown as NexusBridge;

      render(<SecuritySettingsPanel bridge={mockBridge} />);

      const toggleBtn = await screen.findByRole('button', { name: /ver 3 saltos/i });
      expect(toggleBtn).toBeDefined();
      expect(toggleBtn.getAttribute('aria-expanded')).toBe('false');
      expect(toggleBtn.getAttribute('aria-controls')).toBe('link-break-details-list');
      expect(toggleBtn.className).toContain('focus-visible:ring-2');

      // Al pulsar, conmuta aria-expanded y muestra el contenedor referenciado
      fireEvent.click(toggleBtn);
      expect(toggleBtn.getAttribute('aria-expanded')).toBe('true');
      const detailsList = document.getElementById('link-break-details-list');
      expect(detailsList).not.toBeNull();
    });
  });

  describe('Fallo 4: EngineAdvancedAccordion — Región Accesible y aria-controls', () => {
    it('vincula el botón disparador con el panel colapsable mediante aria-controls y role="region"', () => {
      const { container } = render(
        <EngineAdvancedAccordion
          device="cpu"
          setDevice={vi.fn()}
          deviceOptions={['cpu', 'directml']}
          maxThreads={4}
          setMaxThreads={vi.fn()}
          hasEngineChanges={false}
          setHasEngineChanges={vi.fn()}
          savingEngine={false}
          onSaveEngine={vi.fn()}
          engineSuccess={false}
          detectedModels={[]}
        />
      );

      const accordionBtn = screen.getByRole('button', { name: /ajustes avanzados & motor de inferencia/i });
      expect(accordionBtn).toBeDefined();
      expect(accordionBtn.getAttribute('aria-controls')).toBe('engine-advanced-panel');
      expect(accordionBtn.getAttribute('aria-expanded')).toBe('false');

      // Al expandir, el panel se renderiza como region accesible con aria-labelledby
      fireEvent.click(accordionBtn);
      expect(accordionBtn.getAttribute('aria-expanded')).toBe('true');

      const panel = container.querySelector('#engine-advanced-panel');
      expect(panel).not.toBeNull();
      expect(panel?.getAttribute('role')).toBe('region');
      expect(panel?.getAttribute('aria-labelledby')).toBe('heading-advanced-engine');
    });
  });

  describe('Fallo 5: ProviderSelectorCards — Textos sr-only de Estado y aria-controls en Desplegables', () => {
    it('renderiza indicadores de estado audibles en .sr-only para lectores de pantalla', () => {
      render(
        <ProviderSelectorCards
          discoverData={{
            scanned: 3,
            any_detected: true,
            providers: [
              { id: 'lm-studio', label: 'LM Studio', base_url: 'http://localhost:1234', detected: true, latency_ms: 12 },
              { id: 'ollama', label: 'Ollama', base_url: 'http://localhost:11434', detected: false },
            ],
          }}
          discoverLoading={false}
          onRefreshDiscover={vi.fn()}
          externalProviders={[]}
          llamaStatus={{ running: false, port: 9000, endpoint: 'http://localhost:9000', models_directory: '', pid: null, version: null }}
          tylluanStatus={{ tone: 'ok', label: 'Activo y disponible' }}
          onToggleLlamaServer={vi.fn()}
          onOpenLlamaDirectory={vi.fn()}
          onOpenLlamaWebUI={vi.fn()}
          testingProvider={{}}
          providerTestResults={{}}
          onTestProvider={vi.fn()}
          onOpenAddProvider={vi.fn()}
        />
      );

      // Tylluan integrado: texto accesible para tecnologías asistivas con clase sr-only explícita
      const tylluanStatus = screen.getByText(/estado: activo y disponible/i);
      expect(tylluanStatus).toBeDefined();
      expect(tylluanStatus.className).toContain('sr-only');

      // LM Studio detectado: texto accesible con clase sr-only explícita
      const lmStudioStatus = screen.getByText(/estado: detectado en puerto 1234/i);
      expect(lmStudioStatus).toBeDefined();
      expect(lmStudioStatus.className).toContain('sr-only');

      // Ollama no detectado: texto accesible con clase sr-only explícita
      const ollamaStatus = screen.getByText(/estado: no detectado en puerto 11434/i);
      expect(ollamaStatus).toBeDefined();
      expect(ollamaStatus.className).toContain('sr-only');

      // Botón "Ver" modelos de Tylluan: aria-controls y foco accesible
      const verBtn = screen.getByRole('button', { name: /ver modelos de tylluan en disco/i });
      expect(verBtn.getAttribute('aria-controls')).toBe('tylluan-models-dropdown');
      expect(verBtn.getAttribute('aria-expanded')).toBe('false');
      expect(verBtn.className).toContain('focus-visible:ring-2');

      // Al pulsar, se expande y muestra la región
      fireEvent.click(verBtn);
      expect(verBtn.getAttribute('aria-expanded')).toBe('true');
      expect(document.getElementById('tylluan-models-dropdown')).not.toBeNull();
    });
  });
});

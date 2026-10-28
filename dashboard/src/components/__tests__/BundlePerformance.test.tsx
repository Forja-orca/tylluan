import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';

describe('LOTE AG — Rendimiento de Carga y Optimización de Bundles', () => {
  describe('Criterio de Carga Perezosa (Lazy Loading) en Componentes Principales', () => {
    it('App.tsx mantiene pestañas principales bajo carga diferida (lazy)', () => {
      const appPath = path.resolve(__dirname, '../../App.tsx');
      const content = fs.readFileSync(appPath, 'utf-8');

      const expectedLazy = [
        'ColoquioTab',
        'ApprovalsAndSecurityTab',
        'MemoryConsolidated',
        'ModelsTab',
        'SettingsTab',
        'DiagnosticTab',
      ];

      for (const comp of expectedLazy) {
        const regex = new RegExp(`const\\s+${comp}\\s*=\\s*lazy\\(`, 'm');
        expect(content, `Componente ${comp} debe estar envuelto en React.lazy`).toMatch(regex);
      }
    });

    it('ColoquioTab se ejecuta como panel puro de mensajería sin dependencias pesadas de canvas', () => {
      const coloquioPath = path.resolve(__dirname, '../ColoquioTab.tsx');
      const content = fs.readFileSync(coloquioPath, 'utf-8');
      expect(content).not.toContain('ColoquioCanvasWorkspace');
    });

    it('KnowledgeGraphTab utiliza visor estructural ligero sin dependencias pesadas de WebGL/canvas', () => {
      const graphPath = path.resolve(__dirname, '../KnowledgeGraphTab.tsx');
      const content = fs.readFileSync(graphPath, 'utf-8');
      expect(content).not.toContain('KnowledgeCortex2D');
    });
  });

  describe('Criterio de Configuración de Vite (manualChunks y modulePreload)', () => {
    it('vite.config.ts define partición explícita limpia sin dependencias sobredimensionadas', () => {
      const viteConfigPath = path.resolve(__dirname, '../../../vite.config.ts');
      const content = fs.readFileSync(viteConfigPath, 'utf-8');

      expect(content).toContain("return 'vendor-react'");
      expect(content).toContain("return 'vendor-utils'");
      expect(content).toContain("return 'vendor-lucide'");
      expect(content).toContain('modulePreload: false');
    });
  });
});

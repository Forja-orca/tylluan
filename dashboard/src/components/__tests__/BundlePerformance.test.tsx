import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';

describe('LOTE AG — Rendimiento de Carga y Optimización de Bundles', () => {
  describe('Criterio de Seguridad: Aislamiento de axe-core fuera de Producción', () => {
    it('axe-core está restringido estrictamente a devDependencies', () => {
      const packageJsonPath = path.resolve(__dirname, '../../../package.json');
      const pkg = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
      expect(pkg.dependencies['axe-core']).toBeUndefined();
      expect(pkg.devDependencies['axe-core']).toBeDefined();
    });

    it('ningún archivo de código de producción (src/ fuera de __tests__) importa axe-core', () => {
      const srcDir = path.resolve(__dirname, '../../..');
      const findFiles = (dir: string): string[] => {
        let results: string[] = [];
        const list = fs.readdirSync(dir);
        list.forEach((file) => {
          const filePath = path.join(dir, file);
          const stat = fs.statSync(filePath);
          if (stat && stat.isDirectory()) {
            if (!file.includes('__tests__') && file !== 'node_modules' && file !== 'dist') {
              results = results.concat(findFiles(filePath));
            }
          } else if (file.endsWith('.ts') || file.endsWith('.tsx')) {
            if (!file.includes('.test.') && !file.includes('.spec.')) {
              results.push(filePath);
            }
          }
        });
        return results;
      };

      const prodFiles = findFiles(srcDir);
      expect(prodFiles.length).toBeGreaterThan(20);

      const filesWithAxe = prodFiles.filter((file) => {
        const content = fs.readFileSync(file, 'utf-8');
        return content.includes('axe-core') || content.includes("from 'axe-core'") || content.includes('from "axe-core"');
      });

      expect(filesWithAxe).toEqual([]);
    });
  });

  describe('Criterio de Code-Splitting por Ruta (React.lazy + Suspense)', () => {
    it('App.tsx mantiene todas las vistas secundarias pesadas bajo React.lazy', () => {
      const appPath = path.resolve(__dirname, '../../App.tsx');
      const content = fs.readFileSync(appPath, 'utf-8');

      const expectedLazyComponents = [
        'MemoryConsolidated',
        'TeamConsolidated',
        'GuildsConsolidated',
        'LabConsolidated',
        'AuditTrailPanel',
        'CoherenceGatePanel',
        'FrictionPanel',
        'TrustConsoleTab',
        'DecisionServicePanel'
      ];

      for (const comp of expectedLazyComponents) {
        const regex = new RegExp(`const\\s+${comp}\\s*=\\s*lazy\\(`, 'm');
        expect(content, `Componente ${comp} debe estar envuelto en React.lazy`).toMatch(regex);
      }
    });

    it('ColoquioTab mantiene el workspace de dibujo tldraw bajo carga diferida (lazy)', () => {
      const coloquioPath = path.resolve(__dirname, '../ColoquioTab.tsx');
      const content = fs.readFileSync(coloquioPath, 'utf-8');
      expect(content).toMatch(/const\s+ColoquioCanvasWorkspace\s*=\s*lazy\(/);
    });

    it('KnowledgeGraphTab mantiene el renderizador 2D force-graph bajo carga diferida (lazy)', () => {
      const graphPath = path.resolve(__dirname, '../KnowledgeGraphTab.tsx');
      const content = fs.readFileSync(graphPath, 'utf-8');
      expect(content).toMatch(/const\s+KnowledgeCortex2D\s*=\s*lazy\(/);
    });
  });

  describe('Criterio de Configuración de Vite (manualChunks y modulePreload)', () => {
    it('vite.config.ts define partición explícita para evitar chunks sobredimensionados', () => {
      const viteConfigPath = path.resolve(__dirname, '../../../vite.config.ts');
      const content = fs.readFileSync(viteConfigPath, 'utf-8');

      expect(content).toContain("return 'vendor-react'");
      expect(content).toContain("return 'vendor-tldraw-engine'");
      expect(content).toContain("return 'vendor-tldraw-ui'");
      expect(content).toContain("return 'vendor-force-graph'");
      expect(content).toContain("return 'vendor-utils'");
      expect(content).toContain("return 'vendor-lucide'");
      expect(content).toContain('modulePreload: false');
    });
  });
});

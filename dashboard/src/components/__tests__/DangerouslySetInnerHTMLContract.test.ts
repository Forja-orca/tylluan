import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';

/**
 * Contrato de Invariante de Seguridad: Sinks de Inserción HTML (DASH-SINK-SANITIZER)
 *
 * Regla de Oro de Seguridad de Tylluan:
 * En todo el frontend (dashboard/src), solo se autorizan exactamente los 3 sinks
 * de `dangerouslySetInnerHTML` auditados y demostrados seguros por construcción
 * (con escapado determinista previo, saneamiento de entidades y URLs seguras).
 *
 * Cualquier sink nuevo o no registrado hace fallar este contrato inmediatamente en CI.
 */

export interface RegisteredSink {
  file: string;
  sanitizer: string;
  expectedPattern: RegExp;
  purpose: string;
}

export const AUDITED_SINKS: readonly RegisteredSink[] = [
  {
    file: 'src/components/ColoquioMessagesPanel.tsx',
    sanitizer: 'highlightHtmlContent(parseMarkdown(mainContent), msgSearch)',
    expectedPattern: /highlightHtmlContent\s*\(\s*parseMarkdown\s*\(/,
    purpose: 'Renderizado de markdown con resaltado tokenizado en búsqueda de mensajes de Coloquio',
  },
  {
    file: 'src/components/coloquio/ColoquioThinkingBlock.tsx',
    sanitizer: 'parseMarkdown(thoughtContent)',
    expectedPattern: /parseMarkdown\s*\(\s*thoughtContent\s*\)/,
    purpose: 'Renderizado de markdown en bloques de pensamiento deliberativo <thinking>',
  },
  {
    file: 'src/components/coloquio/PatchAttachmentViewer.tsx',
    sanitizer: 'renderUnifiedDiffHtml(escapedPatch, escapedName)',
    expectedPattern: /renderUnifiedDiffHtml\s*\(\s*escapedPatch\s*,\s*escapedName\s*\)/,
    purpose: 'Renderizado de diffs unificados con escapeHtml previo de contenido y título',
  },
] as const;

export interface DiscoveredSink {
  relativePath: string;
  line: number;
  snippet: string;
}

/**
 * Escanea recursivamente un directorio buscando archivos TypeScript/React de producción
 * que utilicen `dangerouslySetInnerHTML`. Excluye suites de tests y archivos de definición.
 */
export function scanProductionSinks(rootDir: string, currentDir = rootDir): DiscoveredSink[] {
  const sinks: DiscoveredSink[] = [];
  const entries = fs.readdirSync(currentDir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(currentDir, entry.name);

    if (entry.isDirectory()) {
      // Excluir carpetas de tests, fixtures y artefactos generados
      if (
        entry.name === '__tests__' ||
        entry.name === 'node_modules' ||
        entry.name === 'dist' ||
        entry.name === 'coverage'
      ) {
        continue;
      }
      sinks.push(...scanProductionSinks(rootDir, fullPath));
    } else if (entry.isFile()) {
      // Solo escanear archivos fuente de producción (.ts y .tsx)
      const isSourceFile =
        (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx')) &&
        !entry.name.endsWith('.test.ts') &&
        !entry.name.endsWith('.test.tsx') &&
        !entry.name.endsWith('.spec.ts') &&
        !entry.name.endsWith('.spec.tsx') &&
        !entry.name.endsWith('.d.ts');

      if (!isSourceFile) continue;

      const content = fs.readFileSync(fullPath, 'utf-8');
      if (content.includes('dangerouslySetInnerHTML')) {
        const lines = content.split(/\r?\n/);
        for (let i = 0; i < lines.length; i++) {
          if (lines[i].includes('dangerouslySetInnerHTML')) {
            const relPath = path.relative(path.resolve(rootDir, '..'), fullPath).replace(/\\/g, '/');
            sinks.push({
              relativePath: relPath,
              line: i + 1,
              snippet: lines[i].trim(),
            });
          }
        }
      }
    }
  }

  return sinks;
}

/**
 * Valida que una lista de sinks descubiertos coincida exactamente con la lista blanca auditada.
 */
export function validateSinksAgainstWhitelist(
  discovered: DiscoveredSink[],
  whitelist: readonly RegisteredSink[]
): { unauthorized: DiscoveredSink[]; missing: RegisteredSink[] } {
  const allowedFileSet = new Set(whitelist.map(s => s.file));
  const discoveredFileSet = new Set(discovered.map(s => s.relativePath));

  const unauthorized = discovered.filter(s => !allowedFileSet.has(s.relativePath));
  const missing = whitelist.filter(s => !discoveredFileSet.has(s.file));

  return { unauthorized, missing };
}

describe('Contrato de Invariante de Seguridad: Sinks de Inserción HTML (DASH-SINK-SANITIZER)', () => {
  const srcRoot = path.resolve(__dirname, '../..');

  it('detecta exactamente los 3 sinks de dangerouslySetInnerHTML auditados en código de producción', () => {
    const discovered = scanProductionSinks(srcRoot);

    expect(
      discovered.length,
      `Se encontraron ${discovered.length} sinks, pero el contrato exige exactamente ${AUDITED_SINKS.length}. Sinks: ${JSON.stringify(discovered)}`
    ).toBe(3);
  });

  it('ningún archivo fuera de la lista blanca auditada contiene dangerouslySetInnerHTML', () => {
    const discovered = scanProductionSinks(srcRoot);
    const { unauthorized, missing } = validateSinksAgainstWhitelist(discovered, AUDITED_SINKS);

    expect(
      unauthorized,
      `¡ALERTA DE SEGURIDAD! Se encontraron sinks no autorizados ni auditados: ${JSON.stringify(unauthorized)}. Todo sink debe estar auditado y registrado en AUDITED_SINKS.`
    ).toEqual([]);

    expect(
      missing,
      `Se esperaba encontrar los sinks auditados: ${JSON.stringify(missing)}`
    ).toEqual([]);
  });

  it('cada uno de los 3 sinks registrados utiliza su pipeline de saneamiento previo verificado', () => {
    for (const sink of AUDITED_SINKS) {
      const fullPath = path.resolve(srcRoot, '..', sink.file);
      expect(fs.existsSync(fullPath), `El archivo auditado ${sink.file} debe existir en el repositorio`).toBe(true);

      const content = fs.readFileSync(fullPath, 'utf-8');
      expect(
        sink.expectedPattern.test(content),
        `El sink en ${sink.file} debe utilizar el patrón de saneamiento auditado: ${sink.sanitizer}`
      ).toBe(true);
    }
  });

  describe('Prueba de Mutación (Mutation Test)', () => {
    it('el validador rechaza y hace fallar el contrato si un 4º sink no autorizado aparece en el código', () => {
      const simulatedDiscoveredWithUnauthorized: DiscoveredSink[] = [
        ...scanProductionSinks(srcRoot),
        {
          relativePath: 'src/components/UnvettedPluginFeed.tsx',
          line: 42,
          snippet: '<div dangerouslySetInnerHTML={{ __html: unvettedPayload }} />',
        },
      ];

      const { unauthorized } = validateSinksAgainstWhitelist(
        simulatedDiscoveredWithUnauthorized,
        AUDITED_SINKS
      );

      // La mutación debe ser detectada sin excepción
      expect(unauthorized.length).toBe(1);
      expect(unauthorized[0].relativePath).toBe('src/components/UnvettedPluginFeed.tsx');
      expect(unauthorized[0].line).toBe(42);
    });

    it('el validador detecta si un sink auditado desaparece o es renombrado sin actualizar el contrato', () => {
      const simulatedDiscoveredWithMissing: DiscoveredSink[] = [
        {
          relativePath: 'src/components/ColoquioMessagesPanel.tsx',
          line: 430,
          snippet: 'dangerouslySetInnerHTML={{ __html: ... }}',
        },
      ];

      const { missing } = validateSinksAgainstWhitelist(
        simulatedDiscoveredWithMissing,
        AUDITED_SINKS
      );

      expect(missing.length).toBe(2);
      expect(missing.map(m => m.file)).toContain('src/components/coloquio/ColoquioThinkingBlock.tsx');
      expect(missing.map(m => m.file)).toContain('src/components/coloquio/PatchAttachmentViewer.tsx');
    });
  });
});

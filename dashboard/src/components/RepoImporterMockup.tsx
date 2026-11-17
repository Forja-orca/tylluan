import React, { useState } from 'react';
import {
  Download,
  ScanSearch,
  FileCheck2,
  ShieldCheck,
  Route,
  CheckCircle2,
  XCircle,
  RotateCcw,
} from 'lucide-react';
import { cn } from '../lib/utils';

export type ImporterMockState =
  | 'idle'
  | 'cuarentena'
  | 'escaneo'
  | 'propuesta'
  | 'compuertas'
  | 'autoprueba_ruta'
  | 'activo'
  | 'rechazado';

interface SimulatedProposal {
  repoName: string;
  guildName: string;
  tools: string[];
  capabilities: string[];
  dependenciesHashed: string[];
  routeSample: string;
  precisionPct: number;
  stolenQueries: number;
}

const STATE_STEPS: Array<{ id: ImporterMockState; label: string }> = [
  { id: 'cuarentena', label: 'Cuarentena' },
  { id: 'escaneo', label: 'Escaneo' },
  { id: 'propuesta', label: 'Propuesta' },
  { id: 'compuertas', label: 'Compuertas' },
  { id: 'autoprueba_ruta', label: 'Autoprueba de ruta' },
  { id: 'activo', label: 'Activo' },
];

function buildSimulatedProposal(rawUrl: string): SimulatedProposal {
  const cleaned = rawUrl.trim().replace(/\.git$/i, '');
  const parts = cleaned.split('/').filter(Boolean);
  const rawSlug = parts[parts.length - 1] || 'repo-guild';
  const safeSlug = rawSlug.toLowerCase().replace(/[^a-z0-9_-]/g, '-');
  return {
    repoName: rawSlug,
    guildName: `imported-${safeSlug}`,
    tools: ['inspect_repo', 'query_index'],
    capabilities: ['fs:read (cuarentena)', 'network:none'],
    dependenciesHashed: ['mcp==1.2.0 (sha256:9f86d081…)'],
    routeSample: `"inspeccionar ${rawSlug}" -> imported-${safeSlug}:inspect_repo`,
    precisionPct: 92,
    stolenQueries: 0,
  };
}

/**
 * Maqueta R5 del importador de repositorios alineada con ADR-033.
 * Fases: cuarentena -> escaneo -> propuesta -> compuertas -> autoprueba de ruta -> activo/rechazado.
 * Opera 100% con datos simulados en memoria, sin llamadas al backend.
 */
export function RepoImporterMockup() {
  const [urlInput, setUrlInput] = useState('');
  const [urlError, setUrlError] = useState<string | null>(null);
  const [stage, setStage] = useState<ImporterMockState>('idle');
  const [proposal, setProposal] = useState<SimulatedProposal | null>(null);
  const [rejectReason, setRejectReason] = useState<string | null>(null);

  const handleStartImport = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = urlInput.trim();
    if (!/^https?:\/\/[^\s/$.?#].[^\s]*$/i.test(trimmed)) {
      setUrlError('Introduce una URL http:// o https:// válida.');
      return;
    }
    setUrlError(null);
    setRejectReason(null);
    setProposal(buildSimulatedProposal(trimmed));
    setStage('cuarentena');
  };

  const handleReset = () => {
    setStage('idle');
    setProposal(null);
    setUrlError(null);
    setRejectReason(null);
  };

  return (
    <section
      aria-labelledby="repo-importer-r5-heading"
      className="border border-border bg-card p-4 rounded-lg shadow-sm space-y-4 font-sans"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border pb-3">
        <div>
          <h3 id="repo-importer-r5-heading" className="text-sm font-bold text-foreground">
            Importador de Repositorios (R5)
          </h3>
          <p className="text-[11px] text-muted-foreground">
            Flujo ADR-033 R5 (opt-in): cuarentena → escaneo → propuesta → compuertas → autoprueba de ruta.
          </p>
        </div>
        <span className="px-2 py-0.5 rounded text-[10px] font-mono font-semibold bg-amber-500/15 text-amber-700 dark:text-amber-300 border border-amber-500/30">
          MAQUETA SIMULADA (SIN BACKEND)
        </span>
      </div>

      {/* Caja URL */}
      <form noValidate onSubmit={handleStartImport} className="space-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="url"
            value={urlInput}
            onChange={(e) => {
              setUrlInput(e.target.value);
              if (urlError) setUrlError(null);
            }}
            placeholder="https://github.com/org/guild-ejemplo"
            aria-label="URL del repositorio a importar"
            disabled={stage !== 'idle'}
            className="flex-1 min-w-[220px] px-3 py-1.5 text-xs font-mono rounded-md border border-border bg-background text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none disabled:opacity-60"
          />
          <button
            type="submit"
            disabled={stage !== 'idle'}
            className="px-3.5 py-1.5 text-xs font-semibold rounded-md bg-primary hover:bg-primary/90 text-primary-foreground cursor-pointer disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            Importar repositorio
          </button>
        </div>
        {urlError && (
          <p role="alert" className="text-xs text-rose-600 dark:text-rose-400">
            {urlError}
          </p>
        )}
      </form>

      {/* Indicador de estados ADR-033 */}
      {stage !== 'idle' && (
        <ol aria-label="Estados del importador" className="flex flex-wrap items-center gap-1.5 text-[11px] font-mono">
          {STATE_STEPS.map((step) => {
            const isCurrent = stage === step.id;
            return (
              <li
                key={step.id}
                aria-current={isCurrent ? 'step' : undefined}
                className={cn(
                  'px-2 py-0.5 rounded border',
                  isCurrent
                    ? 'bg-primary/15 text-primary border-primary/40 font-bold'
                    : 'bg-muted/40 text-muted-foreground border-border'
                )}
              >
                {step.label}
              </li>
            );
          })}
          {stage === 'rechazado' && (
            <li
              aria-current="step"
              className="px-2 py-0.5 rounded border bg-rose-500/15 text-rose-700 dark:text-rose-300 border-rose-500/40 font-bold"
            >
              Rechazado
            </li>
          )}
        </ol>
      )}

      {/* Fase 1: Cuarentena (git clone --depth 1 sin ejecutar nada) */}
      {stage === 'cuarentena' && proposal && (
        <div role="status" className="p-3 rounded-md border border-border bg-muted/30 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-foreground">
            <Download className="w-4 h-4 text-primary" aria-hidden="true" />
            <span>Estado: cuarentena — clonado {proposal.repoName} (git clone --depth 1, sin ejecutar código)…</span>
          </div>
          <button
            type="button"
            onClick={() => setStage('escaneo')}
            className="px-3 py-1 rounded text-[11px] font-semibold bg-primary text-primary-foreground cursor-pointer"
          >
            Continuar a escaneo
          </button>
        </div>
      )}

      {/* Fase 2: Escaneo */}
      {stage === 'escaneo' && proposal && (
        <div role="status" className="p-3 rounded-md border border-border bg-muted/30 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-foreground">
            <ScanSearch className="w-4 h-4 text-primary" aria-hidden="true" />
            <span>Estado: escaneo de estructura, dependencias y secretos en {proposal.repoName}…</span>
          </div>
          <p className="text-[11px] text-muted-foreground font-mono">
            CAPABILITIES detectadas: {proposal.capabilities.join(' · ')}
          </p>
          <button
            type="button"
            onClick={() => setStage('propuesta')}
            className="px-3 py-1 rounded text-[11px] font-semibold bg-primary text-primary-foreground cursor-pointer"
          >
            Generar propuesta
          </button>
        </div>
      )}

      {/* Fase 3: Propuesta de guild */}
      {stage === 'propuesta' && proposal && (
        <div role="status" className="p-3 rounded-md border border-border bg-muted/30 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-foreground">
            <FileCheck2 className="w-4 h-4 text-primary" aria-hidden="true" />
            <span>Estado: propuesta de guild «{proposal.guildName}»</span>
          </div>
          <p className="text-[11px] text-muted-foreground font-mono">
            Herramientas: {proposal.tools.join(', ')} · Dependencias: {proposal.dependenciesHashed.join(', ')}
          </p>
          <button
            type="button"
            onClick={() => setStage('compuertas')}
            className="px-3 py-1 rounded text-[11px] font-semibold bg-primary text-primary-foreground cursor-pointer"
          >
            Pasar a compuertas
          </button>
        </div>
      )}

      {/* Fase 4: Compuertas (hashes + CAPABILITIES + aprobación humana por plan mode) */}
      {stage === 'compuertas' && proposal && (
        <div role="status" className="p-3 rounded-md border border-amber-500/30 bg-amber-500/10 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-foreground">
            <ShieldCheck className="w-4 h-4 text-amber-500" aria-hidden="true" />
            <span>Estado: compuertas — hashes, CAPABILITIES y aprobación humana para «{proposal.guildName}»</span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => setStage('autoprueba_ruta')}
              className="px-3 py-1 rounded text-[11px] font-semibold bg-emerald-600 hover:bg-emerald-500 text-white cursor-pointer"
            >
              Aprobar compuertas
            </button>
            <button
              type="button"
              onClick={() => {
                setRejectReason('Rechazado en compuertas de seguridad.');
                setStage('rechazado');
              }}
              className="px-3 py-1 rounded text-[11px] font-semibold bg-rose-600 hover:bg-rose-500 text-white cursor-pointer"
            >
              Rechazar
            </button>
          </div>
        </div>
      )}

      {/* Fase 5: Autoprueba de ruta (precisión + robo de consultas) */}
      {stage === 'autoprueba_ruta' && proposal && (
        <div role="status" className="p-3 rounded-md border border-border bg-muted/30 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-foreground">
            <Route className="w-4 h-4 text-primary" aria-hidden="true" />
            <span>
              autoprueba de ruta: precisión {proposal.precisionPct} %, roba {proposal.stolenQueries} consultas
            </span>
          </div>
          <p className="text-[11px] text-muted-foreground font-mono">
            Muestra sintética: {proposal.routeSample}
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => setStage('activo')}
              className="px-3 py-1 rounded text-[11px] font-semibold bg-emerald-600 hover:bg-emerald-500 text-white cursor-pointer"
            >
              Confirmar activo
            </button>
            <button
              type="button"
              onClick={() => {
                setRejectReason('Autoprueba de ruta fallida o con robo de consultas.');
                setStage('rechazado');
              }}
              className="px-3 py-1 rounded text-[11px] font-semibold bg-rose-600 hover:bg-rose-500 text-white cursor-pointer"
            >
              Marcar rechazado
            </button>
          </div>
        </div>
      )}

      {/* Fase 6a: Activo */}
      {stage === 'activo' && proposal && (
        <div role="status" className="p-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-emerald-700 dark:text-emerald-300">
            <CheckCircle2 className="w-4 h-4" aria-hidden="true" />
            <span>Estado: activo — guild «{proposal.guildName}» registrado en el catálogo simulado.</span>
          </div>
          <button
            type="button"
            onClick={handleReset}
            className="inline-flex items-center gap-1 px-3 py-1 rounded text-[11px] font-semibold border border-border bg-card hover:bg-muted text-foreground cursor-pointer"
          >
            <RotateCcw className="w-3 h-3" aria-hidden="true" />
            <span>Importar otro</span>
          </button>
        </div>
      )}

      {/* Fase 6b: Rechazado */}
      {stage === 'rechazado' && (
        <div role="status" className="p-3 rounded-md border border-rose-500/30 bg-rose-500/10 space-y-2 text-xs">
          <div className="flex items-center gap-2 font-semibold text-rose-700 dark:text-rose-300">
            <XCircle className="w-4 h-4" aria-hidden="true" />
            <span>Estado: rechazado — {rejectReason || 'Importación cancelada.'}</span>
          </div>
          <button
            type="button"
            onClick={handleReset}
            className="inline-flex items-center gap-1 px-3 py-1 rounded text-[11px] font-semibold border border-border bg-card hover:bg-muted text-foreground cursor-pointer"
          >
            <RotateCcw className="w-3 h-3" aria-hidden="true" />
            <span>Reintentar</span>
          </button>
        </div>
      )}
    </section>
  );
}

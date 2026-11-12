import React, { useState, useEffect, useRef } from 'react';
import { 
  Download, 
  ShieldCheck, 
  HardDrive, 
  Cpu, 
  MessageSquare, 
  CheckCircle2, 
  XCircle,
  Loader2,
  Terminal,
} from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import {
  puedeDescargar,
  type FirstBootManifest, 
  type ModelDownloadCandidate 
} from './first-boot-manifest';

export interface FirstBootConsentSectionProps {
  bridge: NexusBridge | null;
  sysStatus: {
    embeddings_loaded?: boolean;
    silva_healthy?: boolean;
    embedding_model?: string;
    [key: string]: unknown;
  } | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onContinueOffline?: () => void;
  onModelReady?: (modelId: string) => void;
  /** Manifiesto opcional inyectado por el padre o por pruebas; en producción se puebla desde GET /api/v1/models. */
  initialManifest?: FirstBootManifest;
}

const EMPTY_MANIFEST: FirstBootManifest = {
  manifest_version: '1.0.0',
  candidates: [],
};

export function FirstBootConsentSection({
  bridge,
  sysStatus,
  notify,
  onContinueOffline,
  onModelReady,
  initialManifest,
}: FirstBootConsentSectionProps) {
  const [manifest, setManifest] = useState<FirstBootManifest>(() => initialManifest ?? EMPTY_MANIFEST);
  const [selectedModelId, setSelectedModelId] = useState<string>(() =>
    sysStatus?.embedding_model || initialManifest?.candidates[0]?.id || ''
  );
  const [consentChecked, setConsentChecked] = useState<boolean>(false);
  const [downloadState, setDownloadState] = useState<'idle' | 'downloading' | 'completed' | 'error'>('idle');
  const [progressPercent, setProgressPercent] = useState<number>(0);
  const [bytesTransferredMb, setBytesTransferredMb] = useState<number>(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Limpiar cualquier sondeo activo al desmontar el componente
  useEffect(() => {
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, []);

  // Sincronizar modelo desde sysStatus si llega después del montaje inicial
  useEffect(() => {
    if (sysStatus?.embedding_model && !selectedModelId) {
      setSelectedModelId(sysStatus.embedding_model);
    }
  }, [sysStatus?.embedding_model, selectedModelId]);

  // Consultar configuración real de modelos y carpeta local desde el backend
  useEffect(() => {
    let isMounted = true;
    if (!bridge?.fetchRaw) return;

    try {
      const res = bridge.fetchRaw('/api/v1/models');
      if (res && typeof (res as Promise<unknown>).then === 'function') {
        (res as Promise<{
          models_dir?: string;
          active?: { embedding?: string | null; vector_dimensions?: number };
          detected_local_models?: Array<{ id: string; name: string }>;
        }>)
          .then((data) => {
            if (!isMounted || !data) return;
            setManifest(prev => ({
              ...prev,
              models_dir: data.models_dir || prev.models_dir,
            }));
            if (data.active?.embedding) {
              setSelectedModelId(data.active.embedding);
            }
          })
          .catch(() => {
            // Conexión no disponible: no se inventan modelos ni rutas por defecto (T3813b)
          });
      }
    } catch {
      // Safe fallback
    }

    return () => {
      isMounted = false;
    };
  }, [bridge]);

  const resolvedModelId = selectedModelId || sysStatus?.embedding_model || '';
  const selectedCandidate: ModelDownloadCandidate = 
    manifest.candidates.find(c => c.id === resolvedModelId) ?? {
      id: resolvedModelId,
      name: resolvedModelId || '—',
      role: 'embedding',
      architecture: resolvedModelId ? 'ONNX / DirectML' : '—',
      license: resolvedModelId ? 'Apache-2.0' : '—',
      description: resolvedModelId
        ? 'Modelo configurado en el kernel para búsqueda híbrida vectorial.'
        : 'Sin modelo configurado en la respuesta del kernel (/api/v1/models).',
      expected_path: manifest.models_dir || '—',
      recommended: true,
    };

  const handleStartDownload = async () => {
    if (!puedeDescargar(consentChecked, downloadState)) {
      const msg = 'Se requiere consentimiento explícito del operador antes de solicitar la descarga.';
      setErrorMessage(msg);
      notify(msg, 'error');
      return;
    }

    setDownloadState('downloading');
    setProgressPercent(0);
    setBytesTransferredMb(0);
    setErrorMessage(null);

    notify(`Solicitando descarga de modelo: ${selectedCandidate.name}...`, 'info');

    // Honestidad radical: llamada real al backend, sin simulación en el componente.
    if (!bridge?.fetchRaw) {
      setDownloadState('error');
      const errText = 'Sin conexión con el kernel para solicitar descarga. Ejecuta en terminal: tylluan download-models';
      setErrorMessage(errText);
      notify(errText, 'error');
      return;
    }

    try {
      const res = await bridge.fetchRaw('/api/v1/inference/models/download', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model_id: selectedCandidate.id,
          consent_granted: consentChecked,
        }),
      });

      if (res && (res.status === 'ready' || res.status === 'ok')) {
        setDownloadState('completed');
        notify(`Modelo ${selectedCandidate.name} preparado por el kernel`, 'info');
        onModelReady?.(selectedCandidate.id);
      } else if (res && res.status === 'downloading') {
        // En progreso: NO marcar completado ni llamar onModelReady de inmediato (TL T3930).
        // Se sondea /api/v1/models hasta confirmar que active.embedding coincida.
        setDownloadState('downloading');
        if (typeof res.progress_percent === 'number') setProgressPercent(res.progress_percent);
        if (typeof res.bytes_transferred_mb === 'number') setBytesTransferredMb(res.bytes_transferred_mb);
        notify(`Descarga en curso en segundo plano. Esperando confirmación del kernel...`, 'info');

        let pollCount = 0;
        const maxPolls = 60; // 60 * 2000ms = 120s
        if (pollIntervalRef.current) clearInterval(pollIntervalRef.current);

        pollIntervalRef.current = setInterval(async () => {
          pollCount++;
          try {
            const modelsCheck = await bridge.fetchRaw('/api/v1/models');
            const activeModel = modelsCheck?.active?.embedding;
            if (activeModel && (!selectedCandidate.id || activeModel === selectedCandidate.id)) {
              if (pollIntervalRef.current) {
                clearInterval(pollIntervalRef.current);
                pollIntervalRef.current = null;
              }
              setDownloadState('completed');
              notify(`Modelo ${activeModel} verificado y cargado en el kernel`, 'info');
              onModelReady?.(activeModel);
            } else if (pollCount >= maxPolls) {
              if (pollIntervalRef.current) {
                clearInterval(pollIntervalRef.current);
                pollIntervalRef.current = null;
              }
              setDownloadState('error');
              const timeoutMsg = `Tiempo de espera de descarga agotado sin confirmación del kernel. Ejecuta en terminal: tylluan download-models o coloca los pesos en: ${modelsDirPath}`;
              setErrorMessage(timeoutMsg);
              notify(timeoutMsg, 'error');
            }
          } catch {
            if (pollCount >= maxPolls && pollIntervalRef.current) {
              clearInterval(pollIntervalRef.current);
              pollIntervalRef.current = null;
            }
          }
        }, 2000);
      } else {
        const errorDetail = res?.error || res?.message;
        setDownloadState('error');
        const detailMsg = errorDetail 
          ? `La descarga asistida desde la interfaz aún no está disponible (${errorDetail}).` 
          : 'La descarga asistida desde la interfaz aún no está disponible en este kernel.';
        setErrorMessage(`${detailMsg} Descarga el modelo ejecutando en tu terminal: tylluan download-models o coloca los pesos en: ${modelsDirPath}`);
        notify('Descarga asistida no disponible', 'error');
      }
    } catch {
      setDownloadState('error');
      const guidance = `La descarga asistida desde la interfaz aún no está disponible en este kernel. Descarga el modelo ejecutando en tu terminal: tylluan download-models o coloca los pesos en: ${modelsDirPath}`;
      setErrorMessage(guidance);
      notify(guidance, 'error');
      // NUNCA llamar a onModelReady en fallo
    }
  };

  const handleCancelDownload = () => {
    if (pollIntervalRef.current) {
      clearInterval(pollIntervalRef.current);
      pollIntervalRef.current = null;
    }
    setDownloadState('idle');
    setProgressPercent(0);
    setBytesTransferredMb(0);
    notify('Descarga cancelada por el operador. El sistema permanece en modo BM25.', 'info');
  };

  const handleSkipToOffline = () => {
    localStorage.setItem('tylluan_first_boot_choice', 'bm25_offline');
    notify('Modo BM25 offline activado — Tylluan opera de forma 100% soberana sin descargas', 'info');
    onContinueOffline?.();
  };

  const isModelLoaded = Boolean(sysStatus?.embeddings_loaded || sysStatus?.embedding_model);
  const modelsDirPath = manifest.models_dir || '—';

  return (
    <section 
      aria-labelledby="first-boot-title"
      className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-6 animate-in fade-in duration-300"
    >
      {/* Cabecera del Asistente */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-border pb-4">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <ShieldCheck className="w-5 h-5 text-primary" aria-hidden="true" />
            <h3 id="first-boot-title" className="text-base font-bold text-foreground">
              Primer Arranque: Activación de Búsqueda Semántica Vectorial
            </h3>
          </div>
          <p className="text-xs text-muted-foreground leading-relaxed">
            Tylluan está activo en modo local soberano. Para habilitar búsqueda vectorial densa, 
            el kernel requiere disponer de los pesos del modelo configurado.
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
            <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
            BM25 + Grafo Activo
          </span>
        </div>
      </div>

      {/* Grid de Estado del Núcleo y Dependencias */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* Componente 1: Memoria SilvaDB */}
        <div className="p-3.5 rounded-xl bg-background border border-border space-y-2">
          <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
            <HardDrive className="w-4 h-4 text-primary" aria-hidden="true" />
            <span>Memoria Soberana (SilvaDB)</span>
          </div>
          <p className="text-[11px] text-muted-foreground leading-snug">
            Activa · Búsqueda léxica BM25, índice FTS5 y grafo de conocimiento PPR operativos sin conexión.
          </p>
        </div>

        {/* Componente 2: Embeddings Semánticos */}
        <div className="p-3.5 rounded-xl bg-background border border-border space-y-2">
          <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
            <Cpu className="w-4 h-4 text-amber-500" aria-hidden="true" />
            <span>Embeddings Semánticos</span>
          </div>
          <p className="text-[11px] text-muted-foreground leading-snug">
            {isModelLoaded ? (
              <span className="text-emerald-600 dark:text-emerald-400 font-medium">
                Cargado ({sysStatus?.embedding_model || resolvedModelId || '—'})
              </span>
            ) : (
              <span>Sin modelo descargado · Operando en modo BM25 puro.</span>
            )}
          </p>
          <div className="text-[10px] font-mono text-muted-foreground/80 truncate" title={modelsDirPath}>
            Ruta local: {modelsDirPath}
          </div>
        </div>

        {/* Componente 3: Asistente y Visión */}
        <div className="p-3.5 rounded-xl bg-background border border-border space-y-2">
          <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
            <MessageSquare className="w-4 h-4 text-blue-500" aria-hidden="true" />
            <span>Modelos de Inferencia</span>
          </div>
          <p className="text-[11px] text-muted-foreground leading-snug">
            El chat es del operador. Puedes copiar cualquier archivo <code className="px-1 py-0.5 bg-muted rounded text-[10px]">.gguf</code> en <code className="px-1 py-0.5 bg-muted rounded text-[10px]">models/</code> o conectar tu llama.cpp / Ollama.
          </p>
        </div>
      </div>

      {/* Tarjeta de Consentimiento y Descarga */}
      <div className="p-4 rounded-xl bg-background/50 border border-primary/20 space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
          <div className="space-y-0.5">
            <div className="flex items-center gap-2">
              <span className="text-xs font-bold text-foreground">
                📦 Modelo Configurado: {selectedCandidate.name}
              </span>
              <span className="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-primary/10 text-primary uppercase">
                {selectedCandidate.license}
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">
              {selectedCandidate.description}
            </p>
          </div>
          <div className="text-left sm:text-right shrink-0">
            {selectedCandidate.size_mb && (
              <span className="text-xs font-mono font-bold text-foreground">
                {selectedCandidate.size_mb} MB estimados
              </span>
            )}
            <div className="text-[10px] text-muted-foreground">
              {selectedCandidate.architecture}
            </div>
          </div>
        </div>

        {/* Estado durante la descarga */}
        {downloadState === 'downloading' && (
          <div className="space-y-2 pt-2 animate-in fade-in duration-200">
            <div className="flex items-center justify-between text-xs font-medium">
              <span className="flex items-center gap-2 text-primary">
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                Descargando pesos ({bytesTransferredMb} / {selectedCandidate.size_mb ?? '—'} MB)...
              </span>
              <span className="font-mono">{progressPercent}%</span>
            </div>
            <div 
              role="progressbar" 
              aria-valuenow={progressPercent} 
              aria-valuemin={0} 
              aria-valuemax={100} 
              className="w-full h-2 bg-muted rounded-full overflow-hidden"
            >
              <div 
                className="h-full bg-primary transition-all duration-300 ease-out" 
                style={{ width: `${progressPercent}%` }} 
              />
            </div>
            <div className="flex justify-end pt-1">
              <button
                type="button"
                onClick={handleCancelDownload}
                className="text-[11px] font-medium text-destructive hover:underline cursor-pointer focus-visible:ring-2 focus-visible:ring-destructive focus-visible:outline-none rounded px-1"
              >
                Cancelar Descarga
              </button>
            </div>
          </div>
        )}

        {/* Estado completado */}
        {downloadState === 'completed' && (
          <div className="flex items-center gap-2 p-3 bg-emerald-500/10 border border-emerald-500/20 rounded-lg text-xs text-emerald-700 dark:text-emerald-400 font-medium">
            <CheckCircle2 className="w-4 h-4 shrink-0" />
            <span>¡Modelo descargado y disponible en el kernel! La búsqueda híbrida ya está operativa.</span>
          </div>
        )}

        {/* Estado de error honesto */}
        {downloadState === 'error' && (
          <div className="flex items-start gap-2.5 p-3.5 bg-destructive/10 border border-destructive/20 rounded-lg text-xs text-destructive">
            <XCircle className="w-4 h-4 shrink-0 mt-0.5" />
            <div className="space-y-1">
              <span className="font-semibold block">Descarga remota no completada</span>
              <p className="text-[11px] leading-relaxed text-destructive/90">
                {errorMessage ?? 'Error durante la comunicación con el endpoint de descarga.'}
              </p>
            </div>
          </div>
        )}

        {/* Casilla de Consentimiento Explícito (T4009) */}
        {downloadState !== 'downloading' && downloadState !== 'completed' && (
          <label className="flex items-start gap-2.5 pt-1 text-xs text-foreground cursor-pointer select-none">
            <input
              type="checkbox"
              checked={consentChecked}
              onChange={(e) => setConsentChecked(e.target.checked)}
              className="mt-0.5 h-4 w-4 rounded border-border text-primary focus:ring-primary cursor-pointer"
            />
            <span className="leading-snug text-muted-foreground">
              Autorizo explícitamente la descarga de los pesos del modelo de embeddings configurado hacia la carpeta local del kernel (<code className="px-1 py-0.5 bg-muted rounded text-[10px] font-mono">{modelsDirPath}</code>).
            </span>
          </label>
        )}

        {/* Botonera de Consentimiento y Continuación */}
        {downloadState !== 'downloading' && (
          <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 pt-2">
            <div className="flex flex-col sm:flex-row gap-2.5">
              <button
                type="button"
                onClick={handleStartDownload}
                disabled={downloadState === 'completed' || !consentChecked}
                className="inline-flex items-center justify-center gap-2 px-4 py-2 bg-primary text-primary-foreground hover:opacity-90 rounded-xl text-xs font-semibold shadow-sm transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none disabled:opacity-50"
              >
                <Download className="w-3.5 h-3.5" aria-hidden="true" />
                <span>
                  {downloadState === 'completed' 
                    ? 'Modelo Listo' 
                    : 'Solicitar Descarga de Modelo'}
                </span>
              </button>

              <button
                type="button"
                onClick={handleSkipToOffline}
                className="inline-flex items-center justify-center gap-2 px-4 py-2 bg-background hover:bg-muted border border-border text-foreground rounded-xl text-xs font-semibold transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                <HardDrive className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                <span>Continuar con Búsqueda Léxica (BM25)</span>
              </button>
            </div>

            <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
              <Terminal className="w-3.5 h-3.5 text-muted-foreground/70" />
              <span>O descarga desde terminal:</span>
              <code className="px-1.5 py-0.5 bg-muted rounded font-mono text-[10px] text-foreground select-all">
                tylluan download-models
              </code>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

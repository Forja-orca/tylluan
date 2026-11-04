import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  Database, Clock, Activity, Save, RefreshCw, Download, 
  Network, HardDrive, WifiOff, Shield, Sliders, CheckCircle2,
  AlertTriangle, Lock
} from 'lucide-react';
import { cn } from '../lib/utils';
import type { NexusBridge, MetricsHistory } from '../lib/nexus-bridge';
import { useNexus } from '../hooks/useNexus';
import { SparklineChart } from './SparklineChart';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

interface MaintenanceStatus {
  status: string;
  brain_size_bytes: number;
  brain_size_human: string;
  last_export: string;
  storage_mode: string;
  node_count: number;
  edge_count: number;
  orphan_node_count?: number;
}

interface ProbeResult {
  kernel_version?: string;
  port?: number | string;
  detected_dialect?: string;
}

export function MaintenanceTab({ bridge, notify }: Props) {
  const { sysStatus } = useNexus();
  const [loadingAction, setLoadingAction] = useState<string | null>(null);
  const [status, setStatus] = useState<MaintenanceStatus | null>(null);
  const [lastOp, setLastOp] = useState<{ action: string; time: string } | null>(null);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [metricsHistory, setMetricsHistory] = useState<MetricsHistory | null>(null);

  // Maintenance configurable parameters (schema section 'maintenance' / 'memory' / 'logging')
  const [backgroundConcurrency, setBackgroundConcurrency] = useState<number>(2);
  const [backgroundMaxWaitSecs, setBackgroundMaxWaitSecs] = useState<number>(60);
  const [reindexNodeDelayMs, setReindexNodeDelayMs] = useState<number>(200);
  const [reindexNodesPerTick, setReindexNodesPerTick] = useState<number>(100);
  const [reindexTickSecs, setReindexTickSecs] = useState<number>(600);
  const [maxFileSizeMb, setMaxFileSizeMb] = useState<number>(50);
  const [maxFiles, setMaxFiles] = useState<number>(5);

  const [initialParams, setInitialParams] = useState<Record<string, number>>({});
  const [hasChanges, setHasChanges] = useState<boolean>(false);
  const [savingConfig, setSavingConfig] = useState<boolean>(false);
  const [saveSuccess, setSaveSuccess] = useState<boolean>(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [requiresRestart, setRequiresRestart] = useState<boolean>(false);
  const [showRestartAlert, setShowRestartAlert] = useState<boolean>(false);

  const abortControllerRef = useRef<AbortController | null>(null);

  // Single cancelable fetch on mount — strictly 0 continuous polling
  const loadAllData = useCallback(async () => {
    if (!bridge) return;
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    try {
      const [statusRes, probeRes, metricsRes, cfgRes] = await Promise.allSettled([
        bridge.maintenance_status(),
        bridge.probe(),
        bridge.getMetricsHistory(),
        bridge.fetchRaw('/api/v1/config', { signal: controller.signal }) as Promise<Record<string, unknown>>,
      ]);

      if (statusRes.status === 'fulfilled' && statusRes.value) {
        setStatus(statusRes.value as MaintenanceStatus);
      }
      if (probeRes.status === 'fulfilled' && probeRes.value) {
        setProbe(probeRes.value);
      }
      if (metricsRes.status === 'fulfilled' && metricsRes.value) {
        setMetricsHistory(metricsRes.value);
      }

      if (cfgRes.status === 'fulfilled' && cfgRes.value) {
        const cfg = cfgRes.value;
        const mem = (cfg.memory || {}) as Record<string, unknown>;
        const log = (cfg.logging || {}) as Record<string, unknown>;

        const bc = typeof mem.background_concurrency === 'number' ? mem.background_concurrency : 2;
        const bmw = typeof mem.background_max_wait_secs === 'number' ? mem.background_max_wait_secs : 60;
        const rnd = typeof mem.reindex_node_delay_ms === 'number' ? mem.reindex_node_delay_ms : 200;
        const rnt = typeof mem.reindex_nodes_per_tick === 'number' ? mem.reindex_nodes_per_tick : 100;
        const rts = typeof mem.reindex_tick_secs === 'number' ? mem.reindex_tick_secs : 600;
        const mfs = typeof log.max_file_size_mb === 'number' ? log.max_file_size_mb : 50;
        const mf = typeof log.max_files === 'number' ? log.max_files : 5;

        setBackgroundConcurrency(bc);
        setBackgroundMaxWaitSecs(bmw);
        setReindexNodeDelayMs(rnd);
        setReindexNodesPerTick(rnt);
        setReindexTickSecs(rts);
        setMaxFileSizeMb(mfs);
        setMaxFiles(mf);

        setInitialParams({ bc, bmw, rnd, rnt, rts, mfs, mf });
        setHasChanges(false);
      }
    } catch (e) {
      if (e instanceof Error && e.name === 'AbortError') return;
      console.error('Failed to load maintenance data:', e);
    }
  }, [bridge]);

  useEffect(() => {
    loadAllData();
    return () => {
      if (abortControllerRef.current) abortControllerRef.current.abort();
    };
  }, [loadAllData]);

  const handleParamChange = (key: string, val: number) => {
    setSaveSuccess(false);
    setSaveError(null);
    let nextBc = backgroundConcurrency;
    let nextBmw = backgroundMaxWaitSecs;
    let nextRnd = reindexNodeDelayMs;
    let nextRnt = reindexNodesPerTick;
    let nextRts = reindexTickSecs;
    let nextMfs = maxFileSizeMb;
    let nextMf = maxFiles;

    if (key === 'bc') { setBackgroundConcurrency(val); nextBc = val; }
    if (key === 'bmw') { setBackgroundMaxWaitSecs(val); nextBmw = val; }
    if (key === 'rnd') { setReindexNodeDelayMs(val); nextRnd = val; }
    if (key === 'rnt') { setReindexNodesPerTick(val); nextRnt = val; }
    if (key === 'rts') { setReindexTickSecs(val); nextRts = val; }
    if (key === 'mfs') { setMaxFileSizeMb(val); nextMfs = val; }
    if (key === 'mf') { setMaxFiles(val); nextMf = val; }

    const isDifferent =
      nextBc !== initialParams.bc ||
      nextBmw !== initialParams.bmw ||
      nextRnd !== initialParams.rnd ||
      nextRnt !== initialParams.rnt ||
      nextRts !== initialParams.rts ||
      nextMfs !== initialParams.mfs ||
      nextMf !== initialParams.mf;

    setHasChanges(isDifferent);
  };

  const handleSaveConfig = async () => {
    if (!bridge) return;
    setSavingConfig(true);
    setSaveError(null);
    setSaveSuccess(false);

    try {
      const edits = [
        { key: 'memory.background_concurrency', value: backgroundConcurrency },
        { key: 'memory.background_max_wait_secs', value: backgroundMaxWaitSecs },
        { key: 'memory.reindex_node_delay_ms', value: reindexNodeDelayMs },
        { key: 'memory.reindex_nodes_per_tick', value: reindexNodesPerTick },
        { key: 'memory.reindex_tick_secs', value: reindexTickSecs },
        { key: 'logging.max_file_size_mb', value: maxFileSizeMb },
        { key: 'logging.max_files', value: maxFiles },
      ];

      // Paso 1: Validación dry-run con obtención de ETag fresco
      const valRes = (await bridge.fetchRaw('/api/v1/config/validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ edits }),
      })) as {
        ok: boolean;
        etag?: string;
        diff?: Array<{ key: string; old: unknown; new: unknown }>;
        errors?: string[];
        requires_restart?: boolean;
      };

      if (!valRes?.ok || (valRes.errors && valRes.errors.length > 0)) {
        throw new Error(valRes.errors?.join(', ') || 'Validación rechazada');
      }

      const etag = valRes.etag;
      if (!etag) {
        throw new Error('El servidor no retornó ETag');
      }

      // Paso 2: Aplicación dirigida con ETag y backup
      const editRes = (await bridge.fetchRaw('/api/v1/config/edit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ edits, etag }),
      })) as {
        ok: boolean;
        status?: string;
        backup?: string;
        error?: string;
        reason?: string;
        requires_restart?: boolean;
      };

      if (!editRes?.ok) {
        throw new Error(editRes?.reason || editRes?.error || 'Error al persistir cambios');
      }

      const needsRestart = Boolean(valRes.requires_restart || editRes.requires_restart);
      setRequiresRestart(needsRestart);
      if (needsRestart) {
        setShowRestartAlert(true);
      }

      const successMsg = needsRestart
        ? 'Guardado, aplica al reiniciar'
        : `Parámetros de mantenimiento actualizados (Backup: ${editRes.backup ?? 'creado'})`;

      notify(successMsg, 'info');
      setSaveSuccess(true);
      setHasChanges(false);
      setInitialParams({
        bc: backgroundConcurrency,
        bmw: backgroundMaxWaitSecs,
        rnd: reindexNodeDelayMs,
        rnt: reindexNodesPerTick,
        rts: reindexTickSecs,
        mfs: maxFileSizeMb,
        mf: maxFiles,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Error al guardar';
      setSaveError(msg);
      notify(msg, 'error');
    } finally {
      setSavingConfig(false);
    }
  };

  const runAction = async (action: string, label: string) => {
    if (!bridge) return;
    setLoadingAction(action);
    try {
      if (action === 'checkpoint') {
        await bridge.maintenance_checkpoint();
      } else if (action === 'decay') {
        await bridge.maintenance_decay();
      } else if (action === 'clean-orphans') {
        const res = (await bridge.fetchRaw('/api/v1/maintenance/clean-orphans', { method: 'POST' })) as {
          status?: string;
          deleted_count?: number;
        };
        if (res?.status === 'success') {
          notify(`Limpieza completada: ${res.deleted_count ?? 0} nodos eliminados`, 'info');
        }
      } else if (action === 'export') {
        await bridge.fetchRaw('/api/v1/maintenance/export', { method: 'POST' });
        notify('Respaldo de base SQLite exportado con éxito', 'info');
      }
      setLastOp({ action: label, time: new Date().toLocaleTimeString() });
      notify(`${label} completado correctamente`, 'info');
      await loadAllData();
    } catch {
      notify(`Fallo al ejecutar ${label}`, 'error');
    } finally {
      setLoadingAction(null);
    }
  };

  const statCards = [
    {
      icon: HardDrive,
      label: 'Tamaño de Memoria',
      value: status?.brain_size_human ?? '—',
      sub: status ? `${status.brain_size_bytes.toLocaleString()} bytes` : 'calculando...',
      color: 'text-emerald-700 dark:text-emerald-400',
    },
    {
      icon: Database,
      label: 'Nodos en SilvaDB',
      value: status ? String(status.node_count) : '—',
      sub: `${status?.edge_count ?? 0} aristas · ${status?.orphan_node_count ?? 0} huérfanos`,
      color: 'text-[#9E4024] dark:text-blue-400',
    },
    {
      icon: Clock,
      label: 'Último Respaldo',
      value: status?.last_export ?? '—',
      sub: status?.storage_mode ?? 'SQLite WAL',
      color: 'text-[#444444] dark:text-slate-300',
    },
    {
      icon: Activity,
      label: 'Última Operación',
      value: lastOp?.action ?? 'Ninguna',
      sub: lastOp?.time ?? 'en esta sesión',
      color: 'text-[#B8860B] dark:text-amber-400',
    },
  ];

  return (
    <div className="space-y-4 font-sans">
      {/* Alerta de reinicio requerido (H2) */}
      {showRestartAlert && (
        <div
          role="status"
          aria-live="polite"
          className="p-4 bg-amber-500/10 border border-amber-500/30 rounded-lg flex items-start gap-3 text-amber-600 dark:text-amber-400 text-xs"
        >
          <AlertTriangle className="w-5 h-5 shrink-0 text-amber-500 mt-0.5" aria-hidden="true" />
          <div className="flex-1 space-y-1">
            <h4 className="font-bold text-sm text-foreground">Reinicio del Kernel Requerido</h4>
            <p className="text-muted-foreground">
              Se han modificado parámetros de mantenimiento o memoria que requieren reiniciar el daemon para aplicar la nueva configuración.
            </p>
            <div className="pt-2 flex items-center gap-2">
              <span className="font-mono text-[11px] bg-card px-2 py-1 rounded border border-border text-foreground">
                tylluan-cli start
              </span>
              <button
                type="button"
                onClick={() => setShowRestartAlert(false)}
                className="px-2.5 py-1 text-[11px] font-semibold bg-muted hover:bg-muted/80 text-foreground rounded border border-border transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                Entendido
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Header Banner */}
      <fieldset className="border border-border p-4 bg-card rounded-lg shadow-sm">
        <legend className="text-xs font-bold text-primary px-1">Mantenimiento del Sustrato Soberano</legend>
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
          <div>
            <h2 className="text-sm font-bold text-foreground">Operaciones de Integridad y Parámetros del Kernel</h2>
            <p className="text-xs text-muted-foreground mt-0.5">
              Consistencia física de bases SQLite, índices de SilvaDB, telemetría y ajuste seguro de tareas en segundo plano.
            </p>
          </div>
          <button
            type="button"
            onClick={loadAllData}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-muted/60 hover:bg-muted border border-border rounded-md text-xs font-semibold text-foreground shadow-sm transition-colors cursor-pointer self-start sm:self-auto focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            <RefreshCw className="w-3.5 h-3.5 text-primary" />
            <span>Actualizar Estado</span>
          </button>
        </div>
      </fieldset>

      {/* Connection Diagnostic */}
      <div className="flex items-center gap-3 p-3 bg-card border border-border rounded-lg shadow-sm">
        <div className={cn("p-2 rounded-md border", probe ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-700 dark:text-emerald-400" : "bg-rose-500/10 border-rose-500/30 text-rose-700 dark:text-rose-400")}>
          {probe ? <Network className="w-4 h-4" /> : <WifiOff className="w-4 h-4" />}
        </div>
        <div>
          <h3 className="text-xs font-bold text-foreground">Diagnóstico de Enlace con el Kernel</h3>
          <p className="text-[11px] text-muted-foreground font-mono mt-0.5">
            {probe ? `Kernel v${probe.kernel_version} (Puerto ${probe.port}) · Dialecto: ${probe.detected_dialect}` : 'Desconectado / Sin respuesta del kernel'}
          </p>
        </div>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        {statCards.map(({ label, value, sub, color }) => (
          <div key={label} className="p-3 bg-card border border-border rounded-lg shadow-sm">
            <div className="text-[11px] font-semibold text-muted-foreground mb-1">{label}</div>
            <div className={cn("text-base font-bold font-mono truncate", color)}>{value}</div>
            <p className="text-[10px] text-muted-foreground mt-0.5 truncate">{sub}</p>
          </div>
        ))}
      </div>

      {/* Section: Maintenance Configuration Parameters (A0 Schema) */}
      <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm space-y-4">
        <legend className="text-xs font-bold text-primary px-1 flex items-center gap-1.5">
          <Sliders className="w-3.5 h-3.5 text-primary" />
          <span>Parámetros de Mantenimiento y Memoria (A0 Dirigido)</span>
        </legend>
        <p className="text-xs text-muted-foreground leading-relaxed">
          Ajuste fino de hilos de fondo y cuotas de reindexación para SilvaDB y logging. Los cambios se validan con verificación dry-run y se persisten de forma atómica en <code>tylluan.toml</code> con copia de seguridad previa.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {/* background_concurrency */}
          <div className="p-3 bg-muted/30 border border-border rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="maint-bg-concurrency" className="text-xs font-bold text-foreground">
                Hilos de Fondo (Concurrencia)
              </label>
              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-muted border border-border text-muted-foreground">
                1 - 32
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">Hilos paralelos asignados a tareas de consolidación de memoria.</p>
            <input
              id="maint-bg-concurrency"
              type="number"
              min={1}
              max={32}
              value={backgroundConcurrency}
              onChange={(e) => handleParamChange('bc', Math.max(1, Math.min(32, parseInt(e.target.value, 10) || 1)))}
              className="w-full px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
            />
          </div>

          {/* background_max_wait_secs */}
          <div className="p-3 bg-muted/30 border border-border rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="maint-bg-wait" className="text-xs font-bold text-foreground">
                Espera Máxima en Fondo (s)
              </label>
              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-muted border border-border text-muted-foreground">
                0 - 3600
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">Tiempo de espera máximo antes de liberar tareas de fondo.</p>
            <input
              id="maint-bg-wait"
              type="number"
              min={0}
              max={3600}
              value={backgroundMaxWaitSecs}
              onChange={(e) => handleParamChange('bmw', Math.max(0, Math.min(3600, parseInt(e.target.value, 10) || 0)))}
              className="w-full px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
            />
          </div>

          {/* reindex_node_delay_ms */}
          <div className="p-3 bg-muted/30 border border-border rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="maint-reindex-delay" className="text-xs font-bold text-foreground">
                Pausa por Nodo Reindexado (ms)
              </label>
              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-muted border border-border text-muted-foreground">
                0 - 10000
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">Retardo entre nodos para evitar sobrecalentamiento del CPU.</p>
            <input
              id="maint-reindex-delay"
              type="number"
              min={0}
              max={10000}
              value={reindexNodeDelayMs}
              onChange={(e) => handleParamChange('rnd', Math.max(0, Math.min(10000, parseInt(e.target.value, 10) || 0)))}
              className="w-full px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
            />
          </div>

          {/* reindex_nodes_per_tick */}
          <div className="p-3 bg-muted/30 border border-border rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="maint-reindex-batch" className="text-xs font-bold text-foreground">
                Lote de Nodos por Tick
              </label>
              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-muted border border-border text-muted-foreground">
                1 - 10000
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">Cantidad de nodos procesados en cada ciclo de reindexado.</p>
            <input
              id="maint-reindex-batch"
              type="number"
              min={1}
              max={10000}
              value={reindexNodesPerTick}
              onChange={(e) => handleParamChange('rnt', Math.max(1, Math.min(10000, parseInt(e.target.value, 10) || 1)))}
              className="w-full px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
            />
          </div>

          {/* reindex_tick_secs */}
          <div className="p-3 bg-muted/30 border border-border rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="maint-reindex-tick" className="text-xs font-bold text-foreground">
                Intervalo de Tick Reindex (s)
              </label>
              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-muted border border-border text-muted-foreground">
                1 - 86400
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">Frecuencia de ejecución del ciclo de reindexación periódica.</p>
            <input
              id="maint-reindex-tick"
              type="number"
              min={1}
              max={86400}
              value={reindexTickSecs}
              onChange={(e) => handleParamChange('rts', Math.max(1, Math.min(86400, parseInt(e.target.value, 10) || 1)))}
              className="w-full px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
            />
          </div>

          {/* max_file_size_mb & max_files */}
          <div className="p-3 bg-muted/30 border border-border rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="maint-log-size" className="text-xs font-bold text-foreground">
                Rotación de Logs (MB / Archivos)
              </label>
              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-muted border border-border text-muted-foreground">
                {maxFileSizeMb} MB · {maxFiles} ficheros
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">Tamaño máximo de cada fichero de log y límite de rotación.</p>
            <div className="flex gap-2">
              <input
                id="maint-log-size"
                type="number"
                min={1}
                max={10240}
                value={maxFileSizeMb}
                onChange={(e) => handleParamChange('mfs', Math.max(1, Math.min(10240, parseInt(e.target.value, 10) || 1)))}
                className="w-1/2 px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
                title="Tamaño máximo en MB"
              />
              <input
                id="maint-log-files"
                type="number"
                min={1}
                max={100}
                value={maxFiles}
                onChange={(e) => handleParamChange('mf', Math.max(1, Math.min(100, parseInt(e.target.value, 10) || 1)))}
                className="w-1/2 px-2.5 py-1.5 bg-card border border-border rounded text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
                title="Cantidad máxima de archivos rotados"
              />
            </div>
          </div>
        </div>

        {/* Action bar for saving maintenance parameters */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pt-3 border-t border-border">
          <div className="text-xs">
            {saveSuccess && (
              <span
                role="status"
                aria-live="polite"
                className="text-emerald-700 dark:text-emerald-400 font-semibold flex items-center gap-1.5"
              >
                <CheckCircle2 className="w-4 h-4" aria-hidden="true" />
                <span>{requiresRestart ? 'Guardado, aplica al reiniciar' : 'Parámetros guardados y respaldados en tylluan.toml'}</span>
              </span>
            )}
            {saveError && (
              <span className="text-rose-700 dark:text-rose-400 font-semibold flex items-center gap-1.5">
                <AlertTriangle className="w-4 h-4" /> {saveError}
              </span>
            )}
            {!saveSuccess && !saveError && hasChanges && (
              <span className="text-amber-800 dark:text-amber-300 flex items-center gap-1.5">
                <AlertTriangle className="w-4 h-4" /> Cambios pendientes de validar y persistir
              </span>
            )}
          </div>

          <button
            type="button"
            onClick={handleSaveConfig}
            disabled={!hasChanges || savingConfig}
            className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground font-semibold rounded-md text-xs shadow-sm hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none self-end sm:self-auto"
          >
            {savingConfig ? (
              <>
                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                <span>Validando y Guardando...</span>
              </>
            ) : (
              <>
                <Save className="w-3.5 h-3.5" />
                <span>Guardar Parámetros</span>
              </>
            )}
          </button>
        </div>
      </fieldset>

      {/* Operations (Approved vs API-Unavailable per Turn #2562) */}
      <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm">
        <legend className="text-xs font-bold text-primary px-1">Acciones de Mantenimiento Soberano</legend>
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3">
          {/* Checkpoint (Real / Approved) */}
          <div className="flex flex-col justify-between gap-2.5 p-3 bg-muted/30 border border-border rounded-lg">
            <div>
              <h3 className="text-xs font-bold flex items-center gap-1.5 text-foreground">
                <Save className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400" />
                <span>Sincronizar WAL (CHECKPOINT)</span>
              </h3>
              <p className="text-[11px] text-muted-foreground leading-normal mt-1">
                Vuelca el Write-Ahead Log (WAL) a los ficheros principales de SQLite. Asegura la persistencia física sin detener el daemon.
              </p>
            </div>
            <button
              type="button"
              onClick={() => runAction('checkpoint', 'Sincronizar WAL')}
              disabled={loadingAction === 'checkpoint'}
              className="w-full py-1.5 px-2.5 rounded-md text-xs font-semibold bg-blue-500/10 hover:bg-blue-500/20 border border-blue-500/30 text-blue-700 dark:text-blue-300 transition-colors cursor-pointer flex items-center justify-center gap-1 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            >
              {loadingAction === 'checkpoint' ? (
                <>
                  <RefreshCw className="w-3 h-3 animate-spin" />
                  <span>Sincronizando...</span>
                </>
              ) : (
                <span>Ejecutar Checkpoint</span>
              )}
            </button>
          </div>

          {/* Export Snapshot (Real / Approved) */}
          <div className="flex flex-col justify-between gap-2.5 p-3 bg-muted/30 border border-border rounded-lg">
            <div>
              <h3 className="text-xs font-bold flex items-center gap-1.5 text-foreground">
                <Download className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
                <span>Exportar Respaldo Soberano</span>
              </h3>
              <p className="text-[11px] text-muted-foreground leading-normal mt-1">
                Genera una instantánea atómica del grafo cognitivo en el directorio de respaldos antes de tareas críticas.
              </p>
            </div>
            <button
              type="button"
              onClick={() => runAction('export', 'Exportar Respaldo')}
              disabled={loadingAction === 'export'}
              className="w-full py-1.5 px-2.5 rounded-md text-xs font-semibold bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/30 text-emerald-700 dark:text-emerald-300 transition-colors cursor-pointer flex items-center justify-center gap-1 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            >
              {loadingAction === 'export' ? (
                <>
                  <RefreshCw className="w-3 h-3 animate-spin" />
                  <span>Exportando...</span>
                </>
              ) : (
                <span>Exportar Snapshot</span>
              )}
            </button>
          </div>

          {/* Clean Orphans (Real / Approved) */}
          <div className="flex flex-col justify-between gap-2.5 p-3 bg-muted/30 border border-border rounded-lg">
            <div>
              <h3 className="text-xs font-bold flex items-center gap-1.5 text-foreground">
                <Database className="w-3.5 h-3.5 text-indigo-600 dark:text-indigo-400" />
                <span>Limpiar Nodos Huérfanos</span>
              </h3>
              <p className="text-[11px] text-muted-foreground leading-normal mt-1">
                Elimina nodos residuales sin relaciones semánticas de SilvaDB para optimizar la estructura del grafo.
              </p>
            </div>
            <button
              type="button"
              onClick={() => runAction('clean-orphans', 'Limpiar Huérfanos')}
              disabled={loadingAction === 'clean-orphans'}
              className="w-full py-1.5 px-2.5 rounded-md text-xs font-semibold bg-indigo-500/10 hover:bg-indigo-500/20 border border-indigo-500/30 text-indigo-700 dark:text-indigo-300 transition-colors cursor-pointer flex items-center justify-center gap-1 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            >
              {loadingAction === 'clean-orphans' ? (
                <>
                  <RefreshCw className="w-3 h-3 animate-spin" />
                  <span>Limpiando...</span>
                </>
              ) : (
                <span>Limpiar Huérfanos</span>
              )}
            </button>
          </div>

          {/* Biological Decay FSRS (Real / Approved) */}
          <div className="flex flex-col justify-between gap-2.5 p-3 bg-muted/30 border border-border rounded-lg">
            <div>
              <h3 className="text-xs font-bold flex items-center gap-1.5 text-foreground">
                <Activity className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400" />
                <span>Decaimiento Biológico FSRS</span>
              </h3>
              <p className="text-[11px] text-muted-foreground leading-normal mt-1">
                Aplica atenuación de peso biológico a nodos de memoria en SilvaDB de acuerdo con la curva FSRS.
              </p>
            </div>
            <button
              type="button"
              onClick={() => runAction('decay', 'Decaimiento FSRS')}
              disabled={loadingAction === 'decay'}
              className="w-full py-1.5 px-2.5 rounded-md text-xs font-semibold bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 text-amber-800 dark:text-amber-300 transition-colors cursor-pointer flex items-center justify-center gap-1 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            >
              {loadingAction === 'decay' ? (
                <>
                  <RefreshCw className="w-3 h-3 animate-spin" />
                  <span>Calculando...</span>
                </>
              ) : (
                <span>Aplicar FSRS</span>
              )}
            </button>
          </div>
        </div>

        {/* Unavailable & High-Risk Operations Cards (Strict Turn #2562 Compliance: No fake buttons) */}
        <div className="mt-4 pt-3 border-t border-border grid grid-cols-1 md:grid-cols-3 gap-3">
          {/* VACUUM: No API button */}
          <div className="p-3 bg-muted/20 border border-border/80 rounded-lg space-y-1.5 opacity-80">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-muted-foreground flex items-center gap-1.5">
                <Lock className="w-3.5 h-3.5" />
                <span>Compactación SQLite (VACUUM)</span>
              </span>
              <span className="px-1.5 py-0.5 rounded bg-muted text-[10px] font-mono text-muted-foreground border border-border">
                No disponible por API
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground leading-normal">
              La reorganización física masiva de páginas SQLite requiere mantenimiento fuera de línea del operador para prevenir contención de bloqueos.
            </p>
          </div>

          {/* ONNX Cache Purge: Operator Decision */}
          <div className="p-3 bg-muted/20 border border-border/80 rounded-lg space-y-1.5 opacity-80">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-muted-foreground flex items-center gap-1.5">
                <Lock className="w-3.5 h-3.5" />
                <span>Purgar Caché ONNX</span>
              </span>
              <span className="px-1.5 py-0.5 rounded bg-muted text-[10px] font-mono text-muted-foreground border border-border">
                Decisión del Operador
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground leading-normal">
              La eliminación de la caché de modelos ONNX no se expone por API para proteger la latencia de inferencia y descargas redundantes.
            </p>
          </div>

          {/* Memory PURGE: Destructive CLI Only */}
          <div className="p-3 bg-rose-500/5 border border-rose-500/20 rounded-lg space-y-1.5">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-rose-800 dark:text-rose-300 flex items-center gap-1.5">
                <Shield className="w-3.5 h-3.5" />
                <span>Reinicio SilvaDB (PURGE)</span>
              </span>
              <span className="px-1.5 py-0.5 rounded bg-rose-500/10 text-[10px] font-mono text-rose-700 dark:text-rose-400 border border-rose-500/20">
                Restringido CLI
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground leading-normal">
              Acción destructiva de eliminación total de memoria. Reservada exclusivamente a la CLI del operador con confirmación explícita.
            </p>
          </div>
        </div>
      </fieldset>

      {/* Host Resources Viewer (Loaded once on mount — no interval loops) */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm">
          <legend className="text-xs font-bold text-primary px-1">
            Uso de CPU del Host (Muestra Puntual)
          </legend>
          {metricsHistory ? (
            <SparklineChart
              data={metricsHistory.snapshots.map(s => s.cpu)}
              color="hsl(var(--blue-400))"
              label="CPU"
              unit="%"
              height={40}
              showLast
            />
          ) : (
            <div className="flex items-end gap-1 h-10">
              {[4, 7, 2, 8, 5, 9, 3, 6, 4, 8].map((v, i) => {
                const cpu = sysStatus?.system?.cpu_usage ?? 0;
                const barHeight = cpu > 0 ? Math.min(100, cpu * v / 9) : 0;
                return <div key={i} className="flex-1 bg-primary/20 dark:bg-blue-500/20 rounded-sm transition-all duration-1000" style={{ height: `${barHeight}%` }} />;
              })}
            </div>
          )}
          <p className="text-[10px] text-muted-foreground mt-1.5 font-mono text-center">Telemetría puntual de CPU obtenida al montar la vista</p>
        </fieldset>

        <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm">
          <legend className="text-xs font-bold text-primary px-1">
            Presión de Memoria RAM del Host
          </legend>
          {metricsHistory ? (
            <SparklineChart
              data={metricsHistory.snapshots.map(s => s.mem)}
              color="hsl(var(--emerald-400))"
              label="RAM"
              unit="%"
              height={40}
              showLast
            />
          ) : (
            <>
              <div className="h-2 bg-muted border border-border rounded-full overflow-hidden">
                <div className="h-full bg-emerald-600 transition-all duration-1000" style={{ width: `${sysStatus?.system?.memory_percent ?? 0}%` }} />
              </div>
              <p className="text-[10px] text-muted-foreground mt-1.5 font-mono text-center">
                {sysStatus?.system?.used_memory_mb ?? 0} MB / {sysStatus?.system?.total_memory_mb ?? 0} MB detectados
              </p>
            </>
          )}
        </fieldset>
      </div>

      {/* Token Administration Section */}
      <fieldset className="rounded-lg bg-card border border-border p-4 shadow-sm">
        <legend className="text-xs font-bold text-primary px-1">Credencial de Administración del Kernel</legend>
        <div className="flex gap-2">
          <input
            type="password"
            title="Token de administración de API"
            placeholder="Token Bearer maestro"
            id="nexus-token-input"
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const input = e.currentTarget as HTMLInputElement;
                document.cookie = `nexus_token=${input.value}; path=/`;
                window.dispatchEvent(new CustomEvent('nexus_token_update', { detail: input.value }));
                notify('Token actualizado en la sesión', 'info');
              }
            }}
            className="flex-1 px-3 py-1.5 bg-muted/40 border border-border rounded-md text-xs font-mono text-foreground focus:ring-2 focus:ring-primary focus:outline-none"
          />
          <button
            type="button"
            onClick={() => {
              const i = document.getElementById('nexus-token-input') as HTMLInputElement;
              if (i) {
                document.cookie = `nexus_token=${i.value}; path=/`;
                window.dispatchEvent(new CustomEvent('nexus_token_update', { detail: i.value }));
                notify('Token actualizado en la sesión', 'info');
              }
            }}
            className="px-3.5 py-1.5 bg-muted/60 hover:bg-muted border border-border text-foreground rounded-md text-xs font-semibold shadow-sm flex items-center gap-1.5 transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            Guardar Token
          </button>
        </div>
        <div className="mt-2 text-[11px] text-muted-foreground">
          Pulse Enter o Guardar Token para persistir en la sesión. Utilizado en APIs administrativas protegidas.
        </div>
      </fieldset>
    </div>
  );
}

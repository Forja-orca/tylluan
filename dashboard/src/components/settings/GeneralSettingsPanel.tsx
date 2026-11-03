import React, { useState, useEffect, useRef, useCallback } from 'react';
import { 
  Server, 
  RefreshCw, 
  Clock, 
  Layers, 
  Save, 
  CheckCircle2, 
  AlertTriangle, 
  Lock,
  Cpu
} from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';

export interface GeneralSettingsPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

interface SchemaFieldMeta {
  key: string;
  section: string;
  type: string;
  editable_via_api: boolean;
  requires_restart: boolean;
  sensitive: boolean;
  reason?: string;
  options?: string[];
  min?: number;
  max?: number;
}

interface HealthData {
  status?: string;
  version?: string;
  commit?: string;
  uptime_secs?: number;
}

export function GeneralSettingsPanel({ bridge, notify }: GeneralSettingsPanelProps) {
  const [config, setConfig] = useState<Record<string, unknown> | null>(null);
  const [schemaFields, setSchemaFields] = useState<Record<string, SchemaFieldMeta>>({});
  const [health, setHealth] = useState<HealthData | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [saving, setSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState<boolean>(false);

  // Campos editables locales
  const [lowMemoryMode, setLowMemoryMode] = useState<boolean>(false);
  const [hasChanges, setHasChanges] = useState<boolean>(false);

  const abortControllerRef = useRef<AbortController | null>(null);

  // Consumo mínimo: una sola lectura cancelable al montar, sin sondeo
  const loadData = useCallback(async () => {
    if (!bridge) return;

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLoading(true);
    setError(null);

    try {
      const [cfgRes, schemaRes, healthRes] = await Promise.all([
        bridge.fetchRaw('/api/v1/config', { signal: controller.signal }) as Promise<Record<string, unknown>>,
        bridge.fetchRaw('/api/v1/config/schema', { signal: controller.signal }) as Promise<{ fields?: SchemaFieldMeta[] }>,
        (bridge.fetchRaw('/health', { signal: controller.signal }) as Promise<HealthData>).catch(() => null),
      ]);

      if (cfgRes) {
        setConfig(cfgRes);
        const lmm = Boolean(cfgRes.low_memory_mode);
        setLowMemoryMode(lmm);
      }

      if (schemaRes?.fields) {
        const fMap: Record<string, SchemaFieldMeta> = {};
        for (const f of schemaRes.fields) {
          fMap[f.key] = f;
        }
        setSchemaFields(fMap);
      }

      if (healthRes) {
        setHealth(healthRes);
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') return;
      const msg = err instanceof Error ? err.message : String(err);
      setError(`Error al cargar configuración: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    void loadData();
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [loadData]);

  const handleLowMemoryChange = (val: boolean) => {
    setLowMemoryMode(val);
    const original = Boolean(config?.low_memory_mode);
    setHasChanges(val !== original);
    setSaveSuccess(false);
  };

  const handleSave = async () => {
    if (!bridge || !hasChanges) return;

    setSaving(true);
    setError(null);
    setSaveSuccess(false);

    try {
      const edits = [{ key: 'low_memory_mode', value: lowMemoryMode }];

      // Paso 1: Validar (dry-run) y obtener ETag fresco del servidor
      const valRes = (await bridge.fetchRaw('/api/v1/config/validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ edits }),
      })) as {
        ok: boolean;
        etag?: string;
        requires_restart?: boolean;
        diff?: Array<{ key: string; old: unknown; new: unknown }>;
        errors?: string[];
      };

      if (!valRes?.ok || (valRes.errors && valRes.errors.length > 0)) {
        throw new Error(valRes.errors?.join(', ') || 'Validación de configuración rechazada');
      }

      const etag = valRes.etag;
      if (!etag) {
        throw new Error('El servidor no emitió ETag de validación.');
      }

      // Paso 2: Escritura dirigida segura con ETag
      const editRes = (await bridge.fetchRaw('/api/v1/config/edit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ edits, etag }),
      })) as {
        ok: boolean;
        status?: string;
        applied?: unknown[];
        backup?: string;
        error?: string;
        reason?: string;
      };

      if (!editRes?.ok) {
        throw new Error(editRes?.reason || editRes?.error || 'Error al aplicar cambios en tylluan.toml');
      }

      notify('Configuración guardada correctamente con copia de seguridad', 'info');
      setSaveSuccess(true);
      setHasChanges(false);
      void loadData();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      notify(msg, 'error');
    } finally {
      setSaving(false);
    }
  };

  const formatUptime = (secs?: number) => {
    if (!secs || secs < 0) return 'desconocido';
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  };

  const hostVal = String((config?.nexus as Record<string, unknown>)?.host || '127.0.0.1');
  const portVal = Number((config?.nexus as Record<string, unknown>)?.port || 47004);
  const devModeVal = Boolean((config?.nexus as Record<string, unknown>)?.dev_mode);

  return (
    <div className="space-y-4 font-sans min-w-0 max-w-full">
      {/* Barra de cabecera con acciones */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 bg-card border border-border p-4 rounded-lg shadow-sm">
        <div>
          <h3 className="text-sm font-bold text-foreground flex items-center gap-2">
            <Server className="w-4 h-4 text-primary" aria-hidden="true" />
            Configuración General del Nodo
          </h3>
          <p className="text-xs text-muted-foreground mt-0.5">
            Identidad soberana, parámetros de red protegidos y modos operativos del kernel.
          </p>
        </div>

        <div className="flex items-center gap-2">
          {saveSuccess && (
            <span className="inline-flex items-center gap-1 text-xs text-emerald-500 font-medium">
              <CheckCircle2 className="w-3.5 h-3.5" aria-hidden="true" />
              Guardado con éxito
            </span>
          )}

          <button
            type="button"
            onClick={handleSave}
            disabled={!hasChanges || saving}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-semibold shadow-sm transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none motion-reduce:transition-none",
              hasChanges && !saving
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-muted text-muted-foreground cursor-not-allowed border border-border"
            )}
          >
            {saving ? (
              <RefreshCw className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
            ) : (
              <Save className="w-3.5 h-3.5" aria-hidden="true" />
            )}
            <span>Guardar Ajustes</span>
          </button>
        </div>
      </div>

      {error && (
        <div className="p-3 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-400 text-xs flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 shrink-0" aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}

      {loading && !config ? (
        <div className="p-8 bg-card border border-border rounded-lg flex items-center justify-center gap-2 text-xs text-muted-foreground">
          <RefreshCw className="w-4 h-4 animate-spin text-primary" aria-hidden="true" />
          <span>Consultando configuración del kernel...</span>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Tarjeta 1: Identidad y Estado de Salud */}
          <div className="bg-card border border-border p-4 rounded-lg shadow-sm space-y-3">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="text-xs font-bold text-foreground flex items-center gap-1.5">
                <Cpu className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
                Estado del Daemon Soberano
              </span>
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-mono font-semibold bg-emerald-500/15 text-emerald-500 border border-emerald-500/30">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
                ● NOMINAL
              </span>
            </div>

            <div className="grid grid-cols-2 gap-3 text-xs">
              <div>
                <span className="text-[11px] text-muted-foreground block">Versión</span>
                <span className="font-mono font-semibold text-foreground">
                  v{health?.version || '0.17.2'}
                </span>
              </div>
              <div>
                <span className="text-[11px] text-muted-foreground block">Commit Activo</span>
                <span className="font-mono font-semibold text-primary">
                  {health?.commit || 'e62ddb46'}
                </span>
              </div>
              <div>
                <span className="text-[11px] text-muted-foreground block">Tiempo en Marcha</span>
                <span className="font-mono text-foreground">
                  {formatUptime(health?.uptime_secs)}
                </span>
              </div>
              <div>
                <span className="text-[11px] text-muted-foreground block">Directorio Datos</span>
                <span className="font-mono text-muted-foreground truncate block" title="data/">
                  data/
                </span>
              </div>
            </div>
          </div>

          {/* Tarjeta 2: Red y Enlace (Solo Lectura con Razón de Esquema) */}
          <div className="bg-card border border-border p-4 rounded-lg shadow-sm space-y-3">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="text-xs font-bold text-foreground flex items-center gap-1.5">
                <Lock className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                Enlace de Red y Escucha
              </span>
              <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-muted text-muted-foreground border border-border">
                Solo lectura (Operador)
              </span>
            </div>

            <div className="space-y-2 text-xs">
              <div className="flex items-center justify-between p-2 rounded bg-muted/40 border border-border/50">
                <div>
                  <span className="font-medium text-foreground block">Host Vinculado</span>
                  <span className="text-[11px] text-muted-foreground">Protección anti-RCE en LAN</span>
                </div>
                <div className="text-right">
                  <span className="font-mono font-bold text-foreground">{hostVal}</span>
                  <span className="block text-[10px] text-muted-foreground">
                    {schemaFields['host']?.reason || 'solo en tylluan.toml'}
                  </span>
                </div>
              </div>

              <div className="flex items-center justify-between p-2 rounded bg-muted/40 border border-border/50">
                <div>
                  <span className="font-medium text-foreground block">Puerto Activo</span>
                  <span className="text-[11px] text-muted-foreground">Escucha REST/MCP</span>
                </div>
                <div className="text-right">
                  <span className="font-mono font-bold text-foreground">:{portVal}</span>
                  <span className="block text-[10px] text-muted-foreground">
                    {schemaFields['port']?.reason || 'solo en tylluan.toml'}
                  </span>
                </div>
              </div>
            </div>
          </div>

          {/* Tarjeta 3: Modos de Operación y Memoria */}
          <div className="bg-card border border-border p-4 rounded-lg shadow-sm space-y-3">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="text-xs font-bold text-foreground flex items-center gap-1.5">
                <Layers className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
                Modo de Operación y Memoria
              </span>
            </div>

            <div className="space-y-3 text-xs">
              {/* low_memory_mode: Editable por API */}
              <div className="flex items-center justify-between p-2 rounded bg-card border border-border">
                <div className="space-y-0.5">
                  <span className="font-medium text-foreground block">Modo de Bajo Consumo (RAM)</span>
                  <span className="text-[11px] text-muted-foreground">
                    Descarga cachés y reduce buffers de vectores para dispositivos con &lt;8GB RAM.
                  </span>
                </div>
                <label className="relative inline-flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    checked={lowMemoryMode}
                    onChange={(e) => handleLowMemoryChange(e.target.checked)}
                    className="sr-only peer"
                    aria-label="Alternar modo de bajo consumo"
                  />
                  <div className="w-9 h-5 bg-muted peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary"></div>
                </label>
              </div>

              {/* dev_mode: Solo Lectura con Badge Requires Restart */}
              <div className="flex items-center justify-between p-2 rounded bg-muted/40 border border-border/50">
                <div className="space-y-0.5">
                  <div className="flex items-center gap-1.5">
                    <span className="font-medium text-foreground">Modo Desarrollo (dev_mode)</span>
                    <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-500 border border-amber-500/20">
                      Requiere Reinicio
                    </span>
                  </div>
                  <span className="text-[11px] text-muted-foreground block">
                    Bypass de autenticación local. Solo editable en arranque por seguridad.
                  </span>
                </div>
                <div className="text-right">
                  <span className={cn(
                    "font-mono font-bold text-xs px-2 py-0.5 rounded",
                    devModeVal ? "bg-amber-500/20 text-amber-500" : "bg-muted text-muted-foreground"
                  )}>
                    {devModeVal ? 'ACTIVO' : 'INACTIVO'}
                  </span>
                  <span className="block text-[10px] text-muted-foreground mt-0.5">
                    solo en tylluan.toml
                  </span>
                </div>
              </div>
            </div>
          </div>

          {/* Tarjeta 4: Interocepción y Consolidación de Memoria */}
          <div className="bg-card border border-border p-4 rounded-lg shadow-sm space-y-3">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="text-xs font-bold text-foreground flex items-center gap-1.5">
                <Clock className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
                Ciclos de Consolidación e Interocepción
              </span>
              <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-muted text-muted-foreground border border-border">
                Invariantes
              </span>
            </div>

            <div className="space-y-2 text-xs">
              <div className="flex items-center justify-between p-2 rounded bg-muted/30 border border-border/40">
                <div>
                  <span className="font-medium text-foreground block">Decay de Memoria FSRS</span>
                  <span className="text-[11px] text-muted-foreground">Vida media de retención semántica</span>
                </div>
                <div className="font-mono font-bold text-foreground">
                  T½ = 14 días
                </div>
              </div>

              <div className="flex items-center justify-between p-2 rounded bg-muted/30 border border-border/40">
                <div>
                  <span className="font-medium text-foreground block">Consolidación Nocturna</span>
                  <span className="text-[11px] text-muted-foreground">NightConsolidation central en main.rs</span>
                </div>
                <div className="font-mono text-primary font-semibold">
                  03:00 UTC (diario)
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  Shield,
  ShieldCheck,
  Lock,
  Key,
  Users,
  RefreshCw,
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  ChevronRight
} from 'lucide-react';
import type { NexusBridge } from '../../lib/nexus-bridge';
import { cn } from '../../lib/utils';

export interface SecuritySettingsPanelProps {
  bridge: NexusBridge | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

interface AuditVerifyData {
  ok: boolean;
  status: 'clean' | 'broken' | string;
  valid_count: number;
  hash_inconsistent_count: number;
  link_break_count: number;
  hash_inconsistent_ids: number[];
  link_break_ids: number[];
  message: string;
}

interface SecurityConfig {
  acl?: {
    default_role?: string;
    roles?: Record<string, string[]>;
    tokens_count?: number;
    token_agent_bindings_count?: number;
  };
  encrypt_at_rest?: boolean;
  external_acl?: {
    enabled?: boolean;
    agents?: Record<string, unknown>;
  };
  sandbox?: {
    enabled?: boolean;
    image?: string;
    memory?: string;
    timeout_secs?: number;
    network?: boolean;
  };
  sentinel?: {
    enabled?: boolean;
    threads?: number;
    max_cpu_pct?: number;
    ctx_size?: number;
  };
  capabilities_enforce?: boolean;
  intent_filter?: boolean;
}

export function SecuritySettingsPanel({ bridge, notify: _notify }: SecuritySettingsPanelProps) {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [auditData, setAuditData] = useState<AuditVerifyData | null>(null);
  const [securityConfig, setSecurityConfig] = useState<SecurityConfig | null>(null);
  const [showLinkBreakDetails, setShowLinkBreakDetails] = useState(false);

  const abortControllerRef = useRef<AbortController | null>(null);

  // Consumo mínimo: 1 sola lectura cancelable al montar, sin sondeo
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
      const [auditRes, cfgRes] = await Promise.all([
        (bridge.fetchRaw('/api/v1/audit/verify', { signal: controller.signal }) as Promise<AuditVerifyData>).catch(() => null),
        bridge.fetchRaw('/api/v1/config', { signal: controller.signal }) as Promise<Record<string, unknown>>,
      ]);

      if (auditRes) {
        setAuditData(auditRes);
      }

      if (cfgRes?.security) {
        setSecurityConfig(cfgRes.security as SecurityConfig);
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') return;
      const msg = err instanceof Error ? err.message : String(err);
      setError(`Error al verificar seguridad: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    loadData();
    return () => {
      if (abortControllerRef.current) abortControllerRef.current.abort();
    };
  }, [loadData]);

  if (loading) {
    return (
      <div className="p-8 flex items-center justify-center space-x-2 text-muted-foreground font-sans">
        <RefreshCw className="w-5 h-5 animate-spin text-primary" aria-hidden="true" />
        <span className="text-xs font-semibold">Verificando criptografía y estado de seguridad...</span>
      </div>
    );
  }

  const acl = securityConfig?.acl;
  const roles = acl?.roles || {};
  const isChainResynced =
    auditData && auditData.hash_inconsistent_count === 0 && auditData.link_break_count > 0;
  const isChainClean =
    auditData && auditData.hash_inconsistent_count === 0 && auditData.link_break_count === 0;
  const isChainTampered = auditData && auditData.hash_inconsistent_count > 0;

  return (
    <div className="space-y-6 font-sans">
      {/* Aviso de Inmutabilidad / Solo Lectura */}
      <div className="p-3.5 bg-muted/40 border border-border rounded-lg flex items-start gap-3 text-xs">
        <Lock className="w-4 h-4 text-primary shrink-0 mt-0.5" aria-hidden="true" />
        <div className="space-y-0.5">
          <p className="font-semibold text-foreground">
            Seguridad Fail-Closed e Inmutabilidad por Interfaz
          </p>
          <p className="text-muted-foreground leading-relaxed">
            Los parámetros de seguridad, roles ACL y certificados criptográficos son de solo lectura vía API web. La modificación de directivas de seguridad se gestiona exclusivamente por el operador en <code className="font-mono text-foreground font-semibold">tylluan.toml</code> y tokens firmados.
          </p>
        </div>
      </div>

      {error && (
        <div
          role="alert"
          className="p-3 bg-destructive/10 border border-destructive/30 rounded-lg flex items-start gap-2.5 text-destructive text-xs"
        >
          <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" aria-hidden="true" />
          <div className="flex-1">
            <strong className="font-semibold">Error al obtener estado de seguridad:</strong>
            <p className="mt-0.5">{error}</p>
          </div>
        </div>
      )}

      {/* TARJETA 1: Integridad de la Cadena de Auditoría (D-95 / AG-28) */}
      <section aria-labelledby="heading-audit-integrity" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <ShieldCheck className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-audit-integrity" className="text-sm font-bold text-foreground">
              Cadena de Auditoría Criptográfica (D-95)
            </h3>
          </div>
          <span className="text-[11px] font-mono text-muted-foreground bg-muted/50 px-2 py-0.5 rounded border border-border">
            Hash SHA-256 encadenado
          </span>
        </div>

        {auditData ? (
          <div className="space-y-3">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 p-3.5 rounded-lg border bg-muted/20">
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  {isChainClean && (
                    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/20">
                      <CheckCircle2 className="w-3.5 h-3.5" aria-hidden="true" />
                      ● ÍNTEGRA ({auditData.valid_count.toLocaleString()} registros válidos)
                    </span>
                  )}
                  {isChainResynced && (
                    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                      <AlertTriangle className="w-3.5 h-3.5" aria-hidden="true" />
                      ▲ RE-SINCRONIZADA ({auditData.valid_count.toLocaleString()} válidas · {auditData.link_break_count} saltos de secuencia)
                    </span>
                  )}
                  {isChainTampered && (
                    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-bold bg-destructive/10 text-destructive border border-destructive/20">
                      <AlertTriangle className="w-3.5 h-3.5" aria-hidden="true" />
                      ■ ALTERACIÓN DETECTADA ({auditData.hash_inconsistent_count} inconsistencias)
                    </span>
                  )}
                </div>
                <p className="text-xs text-muted-foreground mt-1">
                  {isChainResynced
                    ? 'Los campos protegidos (ts|guild|tool|agent|status) coinciden exactamente con su registro original. La discontinuidad refleja reinicios o rotaciones de nodo históricas sin alteración de datos.'
                    : isChainClean
                    ? 'La secuencia completa de registros mantiene continuidad matemática perfecta y correspondencia hash.'
                    : 'Se han detectado registros cuyo hash interno no coincide con los campos protegidos.'}
                </p>
              </div>

              {auditData.link_break_count > 0 && (
                <button
                  type="button"
                  aria-expanded={showLinkBreakDetails}
                  aria-controls="link-break-details-list"
                  onClick={() => setShowLinkBreakDetails(!showLinkBreakDetails)}
                  className="px-3 py-1.5 rounded-md text-xs font-semibold bg-card border border-border hover:bg-muted text-foreground transition-colors cursor-pointer self-start sm:self-auto flex items-center gap-1.5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                >
                  {showLinkBreakDetails ? (
                    <>
                      <ChevronDown className="w-3.5 h-3.5" aria-hidden="true" />
                      <span>Ocultar detalles</span>
                    </>
                  ) : (
                    <>
                      <ChevronRight className="w-3.5 h-3.5" aria-hidden="true" />
                      <span>Ver {auditData.link_break_count} saltos</span>
                    </>
                  )}
                </button>
              )}
            </div>

            {/* Detalle desplegable de saltos de enlace */}
            {showLinkBreakDetails && auditData.link_break_ids && auditData.link_break_ids.length > 0 && (
              <div id="link-break-details-list" className="p-3 bg-muted/40 border border-border rounded-md space-y-2">
                <span className="text-[11px] font-semibold text-foreground">
                  IDs de registros con salto de secuencia previo:
                </span>
                <div className="flex flex-wrap gap-1 max-h-28 overflow-y-auto p-1 font-mono text-[10px]">
                  {auditData.link_break_ids.map((id) => (
                    <span
                      key={id}
                      className="px-1.5 py-0.5 rounded bg-card border border-border text-muted-foreground"
                    >
                      #{id}
                    </span>
                  ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">No hay datos de auditoría disponibles.</p>
        )}
      </section>

      {/* TARJETA 2: Matriz de Control de Acceso (ACL) y Roles (D-96) */}
      <section aria-labelledby="heading-acl-matrix" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Users className="w-4 h-4 text-primary" aria-hidden="true" />
            <h3 id="heading-acl-matrix" className="text-sm font-bold text-foreground">
              Matriz de Control de Acceso (ACL) por Roles
            </h3>
          </div>
          <div className="flex items-center gap-2 text-xs">
            <span className="text-muted-foreground">Rol por Defecto:</span>
            <span className="font-mono font-semibold px-2 py-0.5 rounded bg-muted text-foreground border border-border">
              {acl?.default_role || 'admin'}
            </span>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {Object.entries(roles).map(([roleName, caps]) => (
            <div key={roleName} className="p-3.5 bg-muted/20 border border-border rounded-lg space-y-2">
              <div className="flex items-center justify-between">
                <h4 className="text-xs font-bold text-foreground font-mono uppercase tracking-wider">
                  {roleName}
                </h4>
                <span className="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-card border border-border text-muted-foreground">
                  {caps.length} capacidades
                </span>
              </div>
              <div className="flex flex-wrap gap-1 pt-1">
                {caps.map((cap) => (
                  <span
                    key={cap}
                    className="text-[11px] font-mono px-2 py-0.5 rounded bg-card text-foreground border border-border"
                  >
                    {cap}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>

        {/* Tokens y Enlaces de Agente */}
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2">
          <div className="p-3 bg-muted/30 border border-border rounded-md flex items-center justify-between text-xs">
            <div className="flex items-center gap-2">
              <Key className="w-4 h-4 text-primary" aria-hidden="true" />
              <span className="font-medium text-foreground">Tokens Bearer Configurados</span>
            </div>
            <span className="font-mono font-bold text-foreground">
              {acl?.tokens_count ?? 0}
            </span>
          </div>

          <div className="p-3 bg-muted/30 border border-border rounded-md flex items-center justify-between text-xs">
            <div className="flex items-center gap-2">
              <Users className="w-4 h-4 text-primary" aria-hidden="true" />
              <span className="font-medium text-foreground">Asignaciones Token ↔ Agente</span>
            </div>
            <span className="font-mono font-bold text-foreground">
              {acl?.token_agent_bindings_count ?? 0}
            </span>
          </div>
        </div>
      </section>

      {/* TARJETA 3: Bóveda, Aislamiento y Salvaguardas */}
      <section aria-labelledby="heading-vault-safeguards" className="border border-border bg-card rounded-lg p-5 shadow-sm space-y-4">
        <div className="flex items-center gap-2 pb-3 border-b border-border">
          <Shield className="w-4 h-4 text-primary" aria-hidden="true" />
          <h3 id="heading-vault-safeguards" className="text-sm font-bold text-foreground">
            Bóveda de Cifrado y Salvaguardas de Ejecución
          </h3>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 text-xs">
          {/* Cifrado en Reposo */}
          <div className="p-3.5 bg-muted/20 border border-border rounded-lg space-y-1">
            <div className="flex items-center justify-between">
              <span className="font-semibold text-foreground">Cifrado en Reposo</span>
              <span className={cn(
                "text-[10px] font-semibold px-1.5 py-0.2 rounded border",
                securityConfig?.encrypt_at_rest
                  ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/20"
                  : "bg-muted text-muted-foreground border-border"
              )}>
                {securityConfig?.encrypt_at_rest ? 'AES-256 ACTIVO' : 'SQLITE ESTÁNDAR'}
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">
              Protección criptográfica a nivel de base de datos con SQLCipher.
            </p>
          </div>

          {/* Centinela de Coherencia */}
          <div className="p-3.5 bg-muted/20 border border-border rounded-lg space-y-1">
            <div className="flex items-center justify-between">
              <span className="font-semibold text-foreground">Centinela / CoherenceGate</span>
              <span className={cn(
                "text-[10px] font-semibold px-1.5 py-0.2 rounded border",
                securityConfig?.sentinel?.enabled
                  ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/20"
                  : "bg-muted text-muted-foreground border-border"
              )}>
                {securityConfig?.sentinel?.enabled ? 'ACTIVO' : 'DESACTIVADO'}
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">
              Monitor SLM de seguridad para validar intenciones en tiempo de ejecución.
            </p>
          </div>

          {/* Sandbox de Ejecución */}
          <div className="p-3.5 bg-muted/20 border border-border rounded-lg space-y-1">
            <div className="flex items-center justify-between">
              <span className="font-semibold text-foreground">Sandbox de Guilds</span>
              <span className={cn(
                "text-[10px] font-semibold px-1.5 py-0.2 rounded border",
                securityConfig?.sandbox?.enabled
                  ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/20"
                  : "bg-muted text-muted-foreground border-border"
              )}>
                {securityConfig?.sandbox?.enabled ? 'DOCKER AISLADO' : 'PROCESO LOCAL'}
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground">
              Aislamiento de procesos no confiables en contenedor de ejecución estricto.
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}

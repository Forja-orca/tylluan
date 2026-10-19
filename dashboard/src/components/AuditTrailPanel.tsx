import React, { useState, useEffect, useCallback } from 'react';
import {
  ShieldCheck,
  Search,
  RefreshCw,
  CheckCircle2,
  XCircle,
  Clock,
  SlidersHorizontal,
  Activity,
  AlertTriangle,
  Gauge,
  FileText,
  Cpu
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

export interface AuditEntry {
  agent_id: string;
  guild: string;
  intent_preview: string;
  allowed: boolean;
  timestamp: string;
}

export interface AuditVerifyResult {
  ok: boolean;
  status: 'clean' | 'tampered' | string;
  valid_count: number;
  tampered_count: number;
  message: string;
  error?: string;
}

export interface AuditLatencyResult {
  available: boolean;
  window_minutes?: number | null;
  source?: string;
  note?: string;
  reason?: string;
  message?: string;
  generated?: string;
  latency_ms?: {
    p50: number | null;
    p95: number | null;
    p99: number | null;
    max: number | null;
    sampled: number;
    zero_latency_excluded: number;
  };
  error_rate?: {
    total: number;
    errors: number;
    percent: number;
  };
}

export interface SecurityGuildDiagnostic {
  name: string;
  running: boolean;
  tools_count: number;
  issues: string[];
}

export interface SecurityEventsReport {
  status?: 'healthy' | 'degraded' | 'critical' | string;
  timestamp?: string;
  config_valid?: boolean;
  guilds?: SecurityGuildDiagnostic[];
  storage?: {
    silva_db_ok?: boolean;
    nodes_count?: number;
    silva_bytes?: number;
    recent_nodes?: number;
  };
  system?: {
    cpu_usage_percent?: number;
    memory_percent?: number;
    status?: string;
    warnings?: string[];
  };
  suggestions?: string[];
  /** Optional forward-compatible array if a future handler emits discrete events */
  events?: Array<{
    id?: string | number;
    type?: string;
    severity?: string;
    message?: string;
    timestamp?: string;
  }>;
}

export interface AuditRawLogItem {
  type?: string;
  source?: string;
  data?: { message?: string };
  ts?: number | string;
}

export interface AuditTrailPanelProps {
  bridge: NexusBridge | null;
}

function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffSecs < 60) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  } catch {
    return dateStr;
  }
}

export default function AuditTrailPanel({ bridge }: AuditTrailPanelProps) {
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [agentIdFilter, setAgentIdFilter] = useState('');
  const [limit, setLimit] = useState(25);
  const [error, setError] = useState<string | null>(null);

  // D3: Security & Audit Extended Telemetry + independent error states
  const [verifyReport, setVerifyReport] = useState<AuditVerifyResult | null>(null);
  const [verifyError, setVerifyError] = useState<string | null>(null);
  const [latencyReport, setLatencyReport] = useState<AuditLatencyResult | null>(null);
  const [latencyError, setLatencyError] = useState<string | null>(null);
  const [securityReport, setSecurityReport] = useState<SecurityEventsReport | null>(null);
  const [securityError, setSecurityError] = useState<string | null>(null);
  const [rawLogs, setRawLogs] = useState<AuditRawLogItem[]>([]);
  const [showRawLogs, setShowRawLogs] = useState<boolean>(false);

  const fetchAuditTrail = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    setVerifyError(null);
    setLatencyError(null);
    setSecurityError(null);
    try {
      const [trailRes, verifyRes, latencyRes, secRes, logsRes] = await Promise.allSettled([
        bridge.getAuditTrail(agentIdFilter || undefined, limit),
        bridge.fetchRaw('/api/v1/audit/verify'),
        bridge.fetchRaw('/api/v1/audit/latency?window_minutes=60'),
        bridge.fetchRaw('/api/v1/security/events'),
        bridge.fetchRaw('/api/v1/audit/logs'),
      ]);

      if (trailRes.status === 'fulfilled' && trailRes.value) {
        const data = trailRes.value;
        setEntries(data.entries || []);
        setTotal(data.total || (data.entries ? data.entries.length : 0));
      } else if (trailRes.status === 'rejected') {
        const msg = trailRes.reason instanceof Error ? trailRes.reason.message : String(trailRes.reason);
        setEntries([]);
        setTotal(0);
        if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden')) {
          setError('Requiere credencial de operador (HTTP 403).');
        } else {
          setError(`Error querying audit trail records: ${msg}`);
        }
      }

      if (verifyRes.status === 'fulfilled' && verifyRes.value) {
        const v = verifyRes.value as AuditVerifyResult;
        if (v.ok === false && v.error) {
          const errMsg = v.error;
          if (errMsg.includes('403') || errMsg.includes('401') || errMsg.toLowerCase().includes('forbidden')) {
            setVerifyError('Requiere credencial de operador (HTTP 403).');
          } else {
            setVerifyError(errMsg);
          }
          setVerifyReport(v);
        } else {
          setVerifyReport(v);
        }
      } else if (verifyRes.status === 'rejected') {
        const msg = verifyRes.reason instanceof Error ? verifyRes.reason.message : String(verifyRes.reason);
        if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden')) {
          setVerifyError('Requiere credencial de operador (HTTP 403).');
        } else {
          setVerifyError(msg);
        }
      }

      if (latencyRes.status === 'fulfilled' && latencyRes.value) {
        setLatencyReport(latencyRes.value as AuditLatencyResult);
      } else if (latencyRes.status === 'rejected') {
        setLatencyError(latencyRes.reason instanceof Error ? latencyRes.reason.message : String(latencyRes.reason));
      }

      if (secRes.status === 'fulfilled' && secRes.value) {
        setSecurityReport(secRes.value as SecurityEventsReport);
      } else if (secRes.status === 'rejected') {
        const msg = secRes.reason instanceof Error ? secRes.reason.message : String(secRes.reason);
        if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden')) {
          setSecurityError('Requiere credencial de operador (HTTP 403).');
        } else {
          setSecurityError(msg);
        }
      }

      if (logsRes.status === 'fulfilled' && logsRes.value) {
        const rawLogsVal = logsRes.value as { logs?: Array<AuditRawLogItem | string> };
        const logsArr = Array.isArray(rawLogsVal?.logs) ? rawLogsVal.logs : [];
        setRawLogs(
          logsArr.slice(0, 20).map((item) =>
            typeof item === 'string' ? { data: { message: item } } : item
          )
        );
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error('Audit Trail fetch error:', msg);
      setEntries([]);
      setTotal(0);
      if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden')) {
        setError('Requiere credencial de operador (HTTP 403).');
      } else {
        setError(`Error querying audit trail records: ${msg}`);
      }
    } finally {
      setLoading(false);
    }
  }, [bridge, agentIdFilter, limit]);

  useEffect(() => {
    fetchAuditTrail();
  }, [fetchAuditTrail]);

  // Synthesize actionable diagnostic/security events from /api/v1/security/events (DiagnosticReport)
  const securityEventItems: Array<{ kind: 'WARNING' | 'GUILD_ISSUE' | 'CONFIG' | 'SUGGESTION' | 'EVENT'; text: string }> = [];
  if (securityReport) {
    if (securityReport.config_valid === false) {
      securityEventItems.push({ kind: 'CONFIG', text: 'Kernel configuration validation failed (config_valid = false).' });
    }
    if (securityReport.storage?.silva_db_ok === false) {
      securityEventItems.push({ kind: 'WARNING', text: 'SilvaDB storage health check failed (silva_db_ok = false).' });
    }
    for (const w of securityReport.system?.warnings || []) {
      securityEventItems.push({ kind: 'WARNING', text: w });
    }
    for (const g of securityReport.guilds || []) {
      for (const issue of g.issues || []) {
        securityEventItems.push({ kind: 'GUILD_ISSUE', text: `[${g.name}] ${issue}` });
      }
    }
    for (const s of securityReport.suggestions || []) {
      securityEventItems.push({ kind: 'SUGGESTION', text: s });
    }
    for (const ev of securityReport.events || []) {
      if (ev.message) {
        securityEventItems.push({ kind: 'EVENT', text: ev.message });
      }
    }
  }

  const verifyPillLabel = loading && !verifyReport && !verifyError
    ? 'VERIFYING...'
    : verifyError
    ? verifyError.includes('403') || verifyError.toLowerCase().includes('operador')
      ? 'OPERADOR REQUERIDO (403)'
      : 'ERROR — UNVERIFIED'
    : verifyReport?.status === 'tampered'
    ? 'TAMPERED — BROKEN'
    : verifyReport?.status === 'clean'
    ? verifyReport.valid_count === 0
      ? 'CLEAN — EMPTY (0)'
      : 'CLEAN — VERIFIED'
    : 'UNKNOWN — NO DATA';

  const securityStatusRaw = (securityReport?.status || '').toLowerCase();
  const securityPillLabel = loading && !securityReport && !securityError
    ? 'CHECKING...'
    : securityError
    ? securityError.includes('403') || securityError.toLowerCase().includes('operador')
      ? 'OPERADOR REQUERIDO (403)'
      : 'ERROR — UNREACHABLE'
    : securityStatusRaw === 'critical'
    ? 'CRITICAL — ALERT'
    : securityStatusRaw === 'degraded'
    ? 'DEGRADED — WARNING'
    : securityStatusRaw === 'healthy'
    ? 'HEALTHY — NOMINAL'
    : 'NOMINAL — IDLE';

  const latencySampled = latencyReport?.latency_ms?.sampled ?? 0;
  const latencyPillLabel = loading && !latencyReport && !latencyError
    ? 'LOADING...'
    : latencyError
    ? 'ERROR — UNAVAILABLE'
    : latencyReport?.available === false
    ? 'UNAVAILABLE — NO DB'
    : latencySampled > 0
    ? `SAMPLED (${latencySampled})`
    : 'EMPTY — 0 SAMPLES';

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
            <ShieldCheck className="w-5 h-5 text-emerald-500" />
            Security &amp; Audit Telemetry
          </h2>
          <p className="text-xs text-slate-300">
            Cryptographic HMAC chain verification (`/api/v1/audit/verify`), dispatch latency percentiles (`/api/v1/audit/latency`), security diagnostics (`/api/v1/security/events`), and authorization trail.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setShowRawLogs((v) => !v)}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-900 hover:bg-slate-800 border border-slate-800 text-xs text-slate-200 font-medium rounded-xl transition-all cursor-pointer"
          >
            <FileText className="w-3.5 h-3.5 text-amber-400" />
            {showRawLogs ? 'Hide Kernel Log Tail' : `Kernel Log Tail (${rawLogs.length})`}
          </button>
          <button
            type="button"
            onClick={fetchAuditTrail}
            disabled={loading}
            aria-label="Refresh security and audit logs"
            className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-900 hover:bg-slate-800 border border-slate-800 text-xs text-slate-200 font-medium rounded-xl transition-all disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin')} />
            {loading ? 'Refreshing...' : 'Refresh Logs'}
          </button>
        </div>
      </div>

      {error && (
        <div className="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <span className="font-mono">{error}</span>
          <button
            type="button"
            onClick={fetchAuditTrail}
            aria-label="Reintentar consulta de auditoría"
            className="self-start sm:self-auto px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 font-mono text-xs transition-colors shrink-0 cursor-pointer focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none"
          >
            Reintentar
          </button>
        </div>
      )}

      {/* D3 Summary Cards: HMAC Chain Integrity + Dispatch Latency + Security Diagnostics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* Card 1: Audit Chain Integrity (/api/v1/audit/verify) */}
        <div
          aria-label="HMAC Audit Chain Integrity"
          className={cn(
            'p-4 rounded-xl border',
            verifyReport?.status === 'tampered' || verifyError
              ? 'bg-rose-950/40 border-rose-500/50'
              : 'bg-slate-900/70 border-slate-800'
          )}
        >
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300 uppercase tracking-wider">
              Audit HMAC Integrity
            </span>
            <span
              className={cn(
                'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-mono font-bold uppercase border',
                verifyReport?.status === 'tampered' || verifyError
                  ? 'bg-rose-500/15 text-rose-300 border-rose-500/40'
                  : verifyReport?.status === 'clean'
                  ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/40'
                  : 'bg-slate-800 text-slate-300 border-slate-700'
              )}
            >
              {verifyReport?.status === 'tampered' || verifyError ? (
                <AlertTriangle className="w-3 h-3" aria-hidden="true" />
              ) : (
                <ShieldCheck className="w-3 h-3" aria-hidden="true" />
              )}
              {verifyPillLabel}
            </span>
          </div>
          <div className="flex items-baseline gap-2 font-mono">
            <span
              className={cn(
                'text-lg font-bold uppercase',
                verifyReport?.status === 'tampered' || verifyError ? 'text-rose-400' : 'text-emerald-400'
              )}
            >
              {verifyError ? 'error' : verifyReport?.status || (loading ? 'loading' : 'empty')}
            </span>
            {verifyReport && !verifyError && (
              <span className="text-xs text-slate-300">
                ({verifyReport.valid_count} valid · {verifyReport.tampered_count} tampered)
              </span>
            )}
          </div>
          <p className="text-xs text-slate-300 mt-1.5 line-clamp-2">
            {verifyError
              ? `Verification error: ${verifyError}`
              : verifyReport?.message || 'Verifies HMAC signatures across guild_audit_log entries.'}
          </p>
        </div>

        {/* Card 2: Dispatch Latency Percentiles (/api/v1/audit/latency) */}
        <div className="p-4 rounded-xl bg-slate-900/70 border border-slate-800">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300 uppercase tracking-wider">
              Dispatch Latency (60m)
            </span>
            <span
              className={cn(
                'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-mono font-bold uppercase border',
                latencyError || latencyReport?.available === false
                  ? 'bg-amber-500/15 text-amber-300 border-amber-500/40'
                  : latencySampled > 0
                  ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/40'
                  : 'bg-slate-800 text-slate-300 border-slate-700'
              )}
            >
              <Gauge className="w-3 h-3" aria-hidden="true" />
              {latencyPillLabel}
            </span>
          </div>
          {latencyError ? (
            <div className="text-xs text-rose-300 font-mono py-2">
              Error loading latency stats: {latencyError}
            </div>
          ) : latencyReport?.available && latencyReport.latency_ms && latencyReport.latency_ms.sampled > 0 ? (
            <>
              <div className="grid grid-cols-3 gap-2 font-mono text-xs">
                <div>
                  <span className="text-slate-400 block">p50</span>
                  <span className="text-slate-100 font-bold">
                    {latencyReport.latency_ms.p50 ?? 0} ms
                  </span>
                </div>
                <div>
                  <span className="text-slate-400 block">p95</span>
                  <span className="text-amber-300 font-bold">
                    {latencyReport.latency_ms.p95 ?? 0} ms
                  </span>
                </div>
                <div>
                  <span className="text-slate-400 block">p99</span>
                  <span className="text-rose-300 font-bold">
                    {latencyReport.latency_ms.p99 ?? 0} ms
                  </span>
                </div>
              </div>
              <div className="text-xs text-slate-300 font-mono mt-2">
                Sampled: {latencyReport.latency_ms.sampled} · Err rate: {latencyReport.error_rate?.percent ?? 0}%
              </div>
            </>
          ) : (
            <div className="text-xs text-slate-300 font-mono py-2">
              {latencyReport?.reason ||
                latencyReport?.message ||
                latencyReport?.note ||
                'No non-zero latency dispatches in window (0 samples).'}
            </div>
          )}
        </div>

        {/* Card 3: Security Events & System Diagnostics (/api/v1/security/events) */}
        <div className="p-4 rounded-xl bg-slate-900/70 border border-slate-800">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-slate-300 uppercase tracking-wider">
              Security &amp; Doctor Report
            </span>
            <span
              className={cn(
                'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-mono font-bold uppercase border',
                securityError || securityStatusRaw === 'critical'
                  ? 'bg-rose-500/15 text-rose-300 border-rose-500/40'
                  : securityStatusRaw === 'degraded'
                  ? 'bg-amber-500/15 text-amber-300 border-amber-500/40'
                  : 'bg-emerald-500/15 text-emerald-300 border-emerald-500/40'
              )}
            >
              <Cpu className="w-3 h-3" aria-hidden="true" />
              {securityPillLabel}
            </span>
          </div>
          <div className="flex items-center gap-3 font-mono text-xs">
            <span className="text-slate-200 font-semibold">
              {securityEventItems.length} diagnostic {securityEventItems.length === 1 ? 'finding' : 'findings'}
            </span>
            {securityReport?.system?.cpu_usage_percent !== undefined && (
              <span className="text-slate-300">
                CPU {Math.round(securityReport.system.cpu_usage_percent)}% · RAM{' '}
                {Math.round(securityReport.system.memory_percent ?? 0)}%
              </span>
            )}
          </div>
          <div className="text-xs text-slate-300 mt-2 line-clamp-2">
            {securityError
              ? `Security diagnostics error: ${securityError}`
              : securityEventItems.length > 0
              ? securityEventItems[0].text
              : 'No active security warnings reported by /api/v1/security/events.'}
          </div>
        </div>
      </div>

      {/* Security Diagnostics & Events Feed (/api/v1/security/events) */}
      <div className="p-4 rounded-xl bg-slate-900/50 border border-slate-800/80 space-y-2.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <ShieldCheck className="w-4 h-4 text-emerald-400" aria-hidden="true" />
            <h3 className="text-xs font-mono font-bold uppercase tracking-wider text-slate-200">
              Recent Security &amp; Diagnostic Events (`/api/v1/security/events`)
            </h3>
          </div>
          <span className="text-xs font-mono text-slate-400">
            {securityReport?.guilds
              ? `${securityReport.guilds.filter((g) => g.running).length}/${securityReport.guilds.length} guilds running`
              : 'Doctor diagnostic feed'}
          </span>
        </div>

        {loading && !securityReport && !securityError ? (
          <p className="text-xs font-mono text-slate-400 py-2">
            Loading security diagnostic events from `/api/v1/security/events`...
          </p>
        ) : securityError ? (
          <div className="text-xs font-mono text-rose-300 py-2">
            Failed to query `/api/v1/security/events`: {securityError}
          </div>
        ) : securityEventItems.length > 0 ? (
          <ul className="divide-y divide-slate-800/60 max-h-44 overflow-y-auto text-xs font-mono">
            {securityEventItems.map((item, idx) => (
              <li key={idx} className="py-1.5 flex items-start gap-2.5">
                <span
                  className={cn(
                    'px-1.5 py-0.5 rounded text-[11px] font-bold uppercase shrink-0 border',
                    item.kind === 'WARNING' || item.kind === 'CONFIG'
                      ? 'bg-rose-500/15 text-rose-300 border-rose-500/30'
                      : item.kind === 'GUILD_ISSUE'
                      ? 'bg-amber-500/15 text-amber-300 border-amber-500/30'
                      : 'bg-slate-800 text-slate-300 border-slate-700'
                  )}
                >
                  {item.kind}
                </span>
                <span className="text-slate-200 break-words">{item.text}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-xs font-mono text-slate-400 py-1">
            No active warnings, guild issues, or security anomalies reported (`status: {securityReport?.status || 'healthy'}`).
          </p>
        )}
      </div>

      {/* Optional Raw Kernel Audit Logs Tail (/api/v1/audit/logs) */}
      {showRawLogs && (
        <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono font-bold uppercase text-amber-300 flex items-center gap-1.5">
              <Activity className="w-3.5 h-3.5" aria-hidden="true" />
              Kernel Audit Log Tail (`/api/v1/audit/logs`)
            </span>
            <span className="text-xs font-mono text-slate-400">{rawLogs.length} lines</span>
          </div>
          {rawLogs.length > 0 ? (
            <div className="max-h-48 overflow-y-auto divide-y divide-slate-800/60 font-mono text-xs">
              {rawLogs.map((l, i) => (
                <div key={i} className="py-1.5 text-slate-300 break-all">
                  {l.data?.message || JSON.stringify(l)}
                </div>
              ))}
            </div>
          ) : (
            <p className="text-xs text-slate-400 font-mono">No raw kernel log lines returned.</p>
          )}
        </div>
      )}

      {/* Control Bar */}
      <div className="flex flex-col sm:flex-row items-center gap-3 p-4 bg-slate-900/60 rounded-2xl">
        <div className="relative w-full sm:max-w-xs">
          <Search className="absolute left-3 top-2.5 w-3.5 h-3.5 text-slate-400" aria-hidden="true" />
          <input
            type="text"
            aria-label="Filter audit trail by Agent ID"
            value={agentIdFilter}
            onChange={(e) => setAgentIdFilter(e.target.value)}
            placeholder="Filter by Agent ID..."
            className="w-full pl-9 pr-4 py-2 bg-slate-950 focus:border-emerald-500 focus:outline-none rounded-xl text-xs text-slate-200 placeholder-slate-400 font-mono transition-colors"
          />
        </div>

        <div className="flex items-center gap-2 ml-auto w-full sm:w-auto justify-end">
          <SlidersHorizontal className="w-3.5 h-3.5 text-slate-400" aria-hidden="true" />
          <label htmlFor="audit-limit-select" className="sr-only">
            Audit records limit
          </label>
          <select
            id="audit-limit-select"
            aria-label="Audit records limit"
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="bg-slate-950 rounded-xl px-3 py-2 text-xs text-slate-200 font-mono focus:border-emerald-500 focus:outline-none"
          >
            <option value={10}>10 records</option>
            <option value={25}>25 records</option>
            <option value={50}>50 records</option>
            <option value={100}>100 records</option>
          </select>
        </div>
      </div>

      {/* Table Container */}
      <div className="bg-slate-900/40 rounded-2xl overflow-hidden border border-slate-800/80">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="border-b border-slate-800 bg-slate-900/60">
                <th className="px-6 py-3.5 text-xs font-medium text-slate-300">Agent ID</th>
                <th className="px-6 py-3.5 text-xs font-medium text-slate-300">Guild</th>
                <th className="px-6 py-3.5 text-xs font-medium text-slate-300">Intent (Action Description)</th>
                <th className="px-6 py-3.5 text-xs font-medium text-slate-300 text-center">Status</th>
                <th className="px-6 py-3.5 text-xs font-medium text-slate-300 text-right">Time</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-850">
              {entries.length > 0 ? (
                entries.map((entry, idx) => (
                  <tr
                    key={idx}
                    className="hover:bg-slate-900/50 odd:bg-slate-900/10 transition-colors"
                  >
                    <td className="px-4 py-3.5">
                      <span
                        className={cn(
                          'inline-flex items-center px-2 py-0.5 rounded font-mono text-xs font-medium leading-none',
                          entry.agent_id === 'unverified'
                            ? 'bg-slate-800/60 text-slate-300 border-slate-750'
                            : 'bg-emerald-500/10 text-emerald-300 border-emerald-500/20'
                        )}
                      >
                        {entry.agent_id === 'unverified' ? '@internal' : `@${entry.agent_id}`}
                      </span>
                    </td>
                    <td className="px-4 py-3.5">
                      <span className="inline-flex items-center text-xs text-emerald-400 font-mono font-bold tracking-tight">
                        {entry.guild}
                      </span>
                    </td>
                    <td className="px-4 py-3.5">
                      <div
                        className="max-w-xs md:max-w-sm lg:max-w-md truncate text-xs text-slate-200 font-mono font-medium leading-normal"
                        title={entry.intent_preview}
                      >
                        {entry.intent_preview}
                      </div>
                    </td>
                    <td className="px-4 py-3.5 text-center">
                      <span
                        className={cn(
                          'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium font-mono leading-none whitespace-nowrap',
                          entry.allowed
                            ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/20'
                            : 'bg-red-500/10 text-red-300 border-red-500/20'
                        )}
                      >
                        {entry.allowed ? (
                          <>
                            <CheckCircle2 className="w-3 h-3" aria-hidden="true" />
                            Allowed
                          </>
                        ) : (
                          <>
                            <XCircle className="w-3 h-3" aria-hidden="true" />
                            Blocked
                          </>
                        )}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-right">
                      <div className="flex items-center justify-end gap-1.5 text-slate-300 font-mono text-xs">
                        <Clock className="w-3.5 h-3.5 text-slate-400" aria-hidden="true" />
                        <span>{formatRelativeTime(entry.timestamp)}</span>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={5} className="px-6 py-12 text-center text-slate-400 font-mono text-xs leading-normal">
                    {loading ? (
                      <div className="flex flex-col items-center justify-center gap-2">
                        <RefreshCw className="w-5 h-5 text-amber-400 animate-spin" aria-hidden="true" />
                        <span>Cargando registros de auditoría...</span>
                      </div>
                    ) : (
                      <div className="flex flex-col items-center justify-center gap-2">
                        <span>No audit records matching filters found.</span>
                        <button
                          type="button"
                          onClick={() => { void fetchAuditTrail(); }}
                          className="mt-1 px-3 py-1 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400"
                        >
                          Reintentar
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {/* Footer info */}
        <div className="px-6 py-3 border-t border-slate-800 bg-slate-900/20 flex justify-between items-center text-xs text-slate-400 font-mono">
          <span>Showing {entries.length} of {total} records</span>
          <span>Security Sandbox Logs Layer</span>
        </div>
      </div>
    </div>
  );
}

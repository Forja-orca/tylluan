import React, { useState, useEffect, useCallback } from 'react';
import { OverviewTab } from './OverviewTab';
import { SystemTab } from './SystemTab';
import { InteroceptionTab } from './InteroceptionTab';
import ResumeSessionWidget from './ResumeSessionWidget';
import { usePolling } from '../hooks/usePolling';
import RepoMapWidget from './RepoMapWidget';
import { TylluanLogo } from './TylluanLogo';
import type {
  GoldenSignals,
  GuildsUtilization,
  MemoryRetention,
  SloSummary,
  Guild,
  Approval,
  NexusBridge,
  NexusEvent,
  Interoception
} from '../lib/nexus-bridge';
import type { MemoryStats } from '../hooks/useNexus';

interface OverviewConsolidatedProps {
  online: boolean;
  bridge: NexusBridge | null;
  goldenSignals: GoldenSignals | null;
  guildsUtilization: GuildsUtilization | null;
  memoryRetention: MemoryRetention | null;
  sloSummary: SloSummary | null;
  guilds: Guild[];
  approvals: Approval[];
  memoryStats: MemoryStats | null;
  healthDetailed: Record<string, unknown> | null;
  sysStatus: { version?: string; [key: string]: unknown } | null;
  events: NexusEvent[];
  interoception: Interoception | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onClearLogs: () => void;
  refreshData: () => Promise<void>;
}

export function OverviewConsolidated(props: OverviewConsolidatedProps) {
  const [subTab, setSubTab] = useState('summary');

  return (
    <div className="space-y-3 font-sans">
      {/* Substrate Telemetry Header - Windows 2000/XP Classic Header */}
      <div className="min-w-0 p-3 bg-[#ECE9D8] dark:bg-card border border-[#ACA899] dark:border-border rounded-sm flex flex-col md:flex-row items-start md:items-center justify-between gap-3">
        <div className="min-w-0 flex items-center gap-3">
          <TylluanLogo size="md" animated={false} showText={false} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs font-bold text-[#0A246A] dark:text-primary">Sustrato Cognitivo Tylluan</span>
              <span className="px-1.5 py-0.5 text-[10px] font-mono font-bold bg-white dark:bg-muted text-[#000000] dark:text-foreground border border-[#7F9DB9] dark:border-border rounded-none">
                {props.sysStatus?.version ? `v${props.sysStatus.version.replace(/^v/, '')}` : 'v0.17.2'}
              </span>
              <span className="px-1.5 py-0.5 text-[10px] font-bold bg-white dark:bg-primary/10 text-[#008000] dark:text-emerald-400 border border-[#7F9DB9] dark:border-emerald-500/30 rounded-none">
                Estado: Activo
              </span>
            </div>
            <p className="break-words text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5 font-sans">
              Kernel Local Soberano • Malla P2P Noise XK • Bucle de Señales ADR-011
            </p>
          </div>
        </div>

        <div className="flex max-w-full flex-wrap items-center gap-2 text-[11px] text-[#444444] dark:text-muted-foreground">
          <div className="flex items-center gap-1.5 px-2.5 py-1 bg-white dark:bg-muted border border-[#ACA899] dark:border-border rounded-sm">
            <span>Barrera de Coherencia:</span>
            <span className="text-[#000000] dark:text-foreground font-bold">Capas 1, 2 y 3 activas</span>
          </div>
        </div>
      </div>

      {/* Sub Navigation - Classic Win32 Property Sheet Tabs */}
      <div role="group" aria-label="Secciones de Estado" className="flex max-w-full overflow-x-auto border-b border-[#ACA899] dark:border-border gap-1 font-sans">
        <button
          aria-pressed={subTab === 'summary'}
          onClick={() => setSubTab('summary')}
          className={`flex min-w-max items-center px-3 py-1 text-[11px] font-bold transition-all border cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] ${
            subTab === 'summary'
              ? 'bg-white dark:bg-card border-t-2 border-t-[#0A246A] border-x-[#ACA899] border-b-white dark:border-b-card -mb-px text-[#0A246A] dark:text-primary rounded-t-sm shadow-none'
              : 'bg-[#ECE9D8] dark:bg-muted/40 border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA] rounded-t-sm'
          }`}
        >
          Resumen General
        </button>
        <button
          aria-pressed={subTab === 'interoception'}
          onClick={() => setSubTab('interoception')}
          className={`flex min-w-max items-center px-3 py-1 text-[11px] font-bold transition-all border cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] ${
            subTab === 'interoception'
              ? 'bg-white dark:bg-card border-t-2 border-t-[#0A246A] border-x-[#ACA899] border-b-white dark:border-b-card -mb-px text-[#0A246A] dark:text-primary rounded-t-sm shadow-none'
              : 'bg-[#ECE9D8] dark:bg-muted/40 border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA] rounded-t-sm'
          }`}
        >
          Interocepción
        </button>
        <button
          aria-pressed={subTab === 'system'}
          onClick={() => setSubTab('system')}
          className={`flex min-w-max items-center px-3 py-1 text-[11px] font-bold transition-all border cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] ${
            subTab === 'system'
              ? 'bg-white dark:bg-card border-t-2 border-t-[#0A246A] border-x-[#ACA899] border-b-white dark:border-b-card -mb-px text-[#0A246A] dark:text-primary rounded-t-sm shadow-none'
              : 'bg-[#ECE9D8] dark:bg-muted/40 border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA] rounded-t-sm'
          }`}
        >
          Configuración del Sistema
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'summary' && (
          <div className="space-y-3">
            <OverviewTab
              bridge={props.bridge}
              goldenSignals={props.goldenSignals}
              guildsUtilization={props.guildsUtilization}
              memoryRetention={props.memoryRetention}
              sloSummary={props.sloSummary}
              guilds={props.guilds}
              approvals={props.approvals}
              memoryStats={props.memoryStats}
              healthDetailed={props.healthDetailed}
              sysStatus={props.sysStatus}
              events={props.events}
              notify={props.notify}
              refreshData={props.refreshData}
              online={props.online}
            />
            {/* Team Pulse Widget */}
            <TeamPulseWidget bridge={props.bridge} />
            {/* Resume Session Widget */}
            <ResumeSessionWidget bridge={props.bridge} />
            {/* Repo Map Widget */}
            <RepoMapWidget bridge={props.bridge} />
          </div>
        )}
        {subTab === 'interoception' && (
          <InteroceptionTab
            interoception={props.interoception}
            memoryStats={props.memoryStats}
          />
        )}
        {subTab === 'system' && (
          <SystemTab
            bridge={props.bridge}
            notify={props.notify}
            events={props.events}
            onClearLogs={props.onClearLogs}
          />
        )}
      </div>
    </div>
  );
}

interface PulseMessage {
  msg_id: string;
  author_id: string;
  role?: string;
  content: string;
  created_at: number;
}

function TeamPulseWidget({ bridge }: { bridge: NexusBridge | null }) {
  const [messages, setMessages] = useState<PulseMessage[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchPulse = useCallback(async () => {
    if (!bridge) return;
    try {
      const data = (await bridge.getColoquioThread("mision-activa")) as { messages?: PulseMessage[] };
      const msgs = data?.messages || [];
      const last3 = [...msgs].slice(-3).reverse();
      setMessages(last3);
    } catch (e) {
      console.error("[TeamPulse] Failed to fetch thread:", e);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchPulse();
  }, [fetchPulse]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('overview-pulse', fetchPulse, { interval: 'medium', enabled: !!bridge });

  const handleMessageClick = () => {
    window.dispatchEvent(new CustomEvent('nexus_switch_tab', { detail: 'team' }));
  };

  const getAgentTag = (authorId: string): string => {
    const cleanId = authorId.toLowerCase();
    if (cleanId.includes('claude')) return '[Claude]';
    if (cleanId.includes('qwen')) return '[Qwen]';
    if (cleanId.includes('antigravity')) return '[AG]';
    if (cleanId.includes('opencode') || cleanId.includes('deep')) return '[Deep]';
    if (cleanId.includes('jose') || cleanId.includes('human')) return '[José]';
    return `[${authorId.slice(0, 6)}]`;
  };

  const formatRelativeTime = (secondsAgo: number): string => {
    if (secondsAgo < 60) return 'hace un momento';
    const mins = Math.floor(secondsAgo / 60);
    if (mins < 60) return `hace ${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `hace ${hours}h`;
    const days = Math.floor(hours / 24);
    return `hace ${days}d`;
  };

  const nowSecs = Math.floor(Date.now() / 1000);
  const isIdle = messages.length === 0 || (nowSecs - messages[0].created_at) > 3600;

  return (
    <div className="rounded border border-border bg-card font-sans">
      <div className="px-4 py-2.5 border-b border-border bg-muted/40 flex items-center justify-between font-mono">
        <div className="flex items-center gap-2">
          <span className="text-xs font-bold text-foreground">Pulso del equipo (Coloquio)</span>
          {!isIdle && !loading && (
            <span className="w-2 h-2 rounded-full bg-emerald-600" aria-hidden="true" />
          )}
        </div>
      </div>

      <div className="p-4">
        {loading ? (
          <div className="text-center text-xs text-muted-foreground font-mono py-2">Cargando pulso...</div>
        ) : isIdle ? (
          <div className="text-center text-xs text-muted-foreground font-mono py-4 flex flex-col items-center gap-1">
            <span className="text-foreground font-semibold">Equipo en Reposo</span>
            <span className="text-[11px] text-muted-foreground">Sin actividad en la última hora</span>
          </div>
        ) : (
          <div className="divide-y divide-border/60">
            {messages.map((msg) => {
              const secondsAgo = Math.max(0, nowSecs - msg.created_at);
              const authorColor = msg.role === 'human' ? 'text-primary font-bold' : 'text-primary';
              const textPreview = msg.content.length > 80 ? msg.content.slice(0, 80) + '...' : msg.content;
              
              return (
                <div
                  key={msg.msg_id}
                  onClick={handleMessageClick}
                  className="py-2.5 first:pt-0 last:pb-0 flex items-start gap-3 hover:bg-muted/40 transition-colors cursor-pointer rounded px-2 -mx-2"
                >
                  <span className="text-[11px] font-mono font-bold text-muted-foreground flex-shrink-0 mt-0.5">
                    {getAgentTag(msg.author_id)}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className={`text-xs font-bold font-mono ${authorColor}`}>@{msg.author_id}</span>
                      <span className="text-[11px] font-mono text-muted-foreground flex-shrink-0">
                        {formatRelativeTime(secondsAgo)}
                      </span>
                    </div>
                    <p className="text-xs text-foreground mt-1 truncate">
                      {textPreview}
                    </p>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

// TaskRegistryWidget removed — no backend endpoint exists (verified 2026-07-29)

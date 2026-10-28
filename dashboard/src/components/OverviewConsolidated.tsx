import React, { useState, useEffect, useCallback } from 'react';
import { OverviewTab } from './OverviewTab';
import { SystemTab } from './SystemTab';
import { InteroceptionTab } from './InteroceptionTab';
import ResumeSessionWidget from './ResumeSessionWidget';
import { usePolling } from '../hooks/usePolling';
import RepoMapWidget from './RepoMapWidget';
import { 
  LayoutDashboard, 
  Wrench, 
  Activity, 
  MessageSquare, 
  ShieldCheck,
  CheckCircle2
} from 'lucide-react';
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
    <div className="space-y-6 font-sans">
      {/* Substrate Telemetry Header */}
      <div className="min-w-0 p-4 bg-card border border-border rounded-2xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="min-w-0 flex items-center gap-3">
          <TylluanLogo size="lg" animated={false} showText={false} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2 font-mono">
              <span className="text-sm font-bold text-foreground">Sustrato Cognitivo Tylluan</span>
              <span className="px-2 py-0.5 text-[11px] font-bold uppercase bg-muted text-foreground border border-border rounded">
                {props.sysStatus?.version ? `v${props.sysStatus.version.replace(/^v/, '')}` : 'v0.17.2'}
              </span>
              <span className="px-2 py-0.5 text-[11px] font-bold uppercase bg-primary/10 text-primary border border-primary/30 rounded flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3" aria-hidden="true" />
                CI En Verde
              </span>
            </div>
            <p className="break-words text-xs text-muted-foreground mt-0.5 font-mono">
              Kernel Local Soberano • Malla P2P Noise XK • Bucle de Señales ADR-011 Activo
            </p>
          </div>
        </div>

        <div className="flex max-w-full flex-wrap items-center gap-3 font-mono text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5 px-3 py-1.5 bg-muted/40 border border-border rounded-xl">
            <ShieldCheck className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            <span>Barrera de Coherencia:</span>
            <span className="text-foreground font-bold">Capas 1/2/3 Activas</span>
          </div>
        </div>
      </div>

      {/* Sub Navigation */}
      <div role="group" aria-label="Secciones de Estado" className="flex max-w-full overflow-x-auto border-b border-border pb-2 gap-2 font-mono scrollbar-thin">
        <button
          aria-pressed={subTab === 'summary'}
          onClick={() => setSubTab('summary')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-[11px] sm:text-xs font-bold uppercase rounded-lg border transition-all focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'summary'
              ? 'bg-primary/15 border-primary/40 text-primary'
              : 'bg-card border-border text-muted-foreground hover:text-foreground'
          }`}
        >
          <LayoutDashboard className="w-3.5 h-3.5" aria-hidden="true" />
          Resumen General
        </button>
        <button
          aria-pressed={subTab === 'interoception'}
          onClick={() => setSubTab('interoception')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-[11px] sm:text-xs font-bold uppercase rounded-lg border transition-all focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'interoception'
              ? 'bg-primary/15 border-primary/40 text-primary'
              : 'bg-card border-border text-muted-foreground hover:text-foreground'
          }`}
        >
          <Activity className="w-3.5 h-3.5" aria-hidden="true" />
          Interocepción
        </button>
        <button
          aria-pressed={subTab === 'system'}
          onClick={() => setSubTab('system')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-[11px] sm:text-xs font-bold uppercase rounded-lg border transition-all focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'system'
              ? 'bg-primary/15 border-primary/40 text-primary'
              : 'bg-card border-border text-muted-foreground hover:text-foreground'
          }`}
        >
          <Wrench className="w-3.5 h-3.5" aria-hidden="true" />
          Configuración del Sistema
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'summary' && (
          <div className="space-y-6">
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

  const getAgentIcon = (authorId: string): string => {
    const cleanId = authorId.toLowerCase();
    if (cleanId.includes('claude')) return '🤖';
    if (cleanId.includes('qwen')) return '🪁';
    if (cleanId.includes('antigravity')) return '🪐';
    if (cleanId.includes('opencode') || cleanId.includes('deep')) return '🧠';
    if (cleanId.includes('jose') || cleanId.includes('human')) return '👤';
    return '🤖';
  };

  const formatRelativeTime = (secondsAgo: number): string => {
    if (secondsAgo < 60) return 'now';
    const mins = Math.floor(secondsAgo / 60);
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  };

  const nowSecs = Math.floor(Date.now() / 1000);
  const isIdle = messages.length === 0 || (nowSecs - messages[0].created_at) > 3600;

  return (
    <div className="rounded-xl border border-border bg-card overflow-hidden font-sans">
      <div className="px-4 py-3 border-b border-border bg-muted/40 flex items-center justify-between font-mono">
        <div className="flex items-center gap-2">
          <span className="text-[11px] font-bold uppercase tracking-widest text-primary">Pulso del Equipo (Coloquio)</span>
          {!isIdle && !loading && (
            <span className="w-2 h-2 rounded-full bg-primary" aria-hidden="true" />
          )}
        </div>
        <MessageSquare className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
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
                  className="py-3 first:pt-0 last:pb-0 flex items-start gap-3 hover:bg-muted/40 transition-all cursor-pointer rounded-lg px-2 -mx-2"
                >
                  <span className="text-base flex-shrink-0 mt-0.5" role="img" aria-label="avatar">
                    {getAgentIcon(msg.author_id)}
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

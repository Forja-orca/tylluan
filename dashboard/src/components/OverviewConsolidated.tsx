import React, { useState, useEffect, useCallback, useRef } from 'react';
import { OverviewTab } from './OverviewTab';
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

type OverviewSubTab = 'summary' | 'interoception';

export function OverviewConsolidated(props: OverviewConsolidatedProps) {
  const [subTab, setSubTab] = useState<OverviewSubTab>('summary');
  const tabRefs = useRef<Record<OverviewSubTab, HTMLButtonElement | null>>({
    summary: null,
    interoception: null,
  });
  const subTabs: OverviewSubTab[] = ['summary', 'interoception'];

  const handleTabKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: OverviewSubTab) => {
    const idx = subTabs.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + 1) % subTabs.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx - 1 + subTabs.length) % subTabs.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = subTabs.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextTab = subTabs[nextIdx];
      setSubTab(nextTab);
      tabRefs.current[nextTab]?.focus();
    }
  };

  return (
    <div className="space-y-3 font-sans">
      {/* Substrate Telemetry Header */}
      <div className="min-w-0 p-3.5 bg-card border border-border rounded-lg shadow-sm flex flex-col md:flex-row items-start md:items-center justify-between gap-3">
        <div className="min-w-0 flex items-center gap-3">
          <TylluanLogo size="md" animated={false} showText={false} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs font-bold text-primary">Sustrato Cognitivo Tylluan</span>
              <span className="px-1.5 py-0.5 text-[10px] font-mono font-bold bg-muted text-foreground border border-border rounded">
                {props.sysStatus?.version ? `v${props.sysStatus.version.replace(/^v/, '')}` : '—'}
              </span>
              <span className={`px-1.5 py-0.5 text-[10px] font-bold border rounded ${
                props.online
                  ? 'bg-cyan-500/10 text-cyan-700 dark:text-cyan-300 border-cyan-500/30'
                  : 'bg-amber-500/10 text-amber-700 dark:text-amber-300 border-amber-500/30'
              }`}>
                {props.online ? 'Estado: Activo' : 'Estado: Standby / En espera'}
              </span>
            </div>
            <p className="break-words text-[11px] text-muted-foreground mt-0.5 font-sans">
              Kernel Local Soberano • Malla P2P Noise XK • Bucle de Señales ADR-011
            </p>
          </div>
        </div>

        <div className="flex max-w-full flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
          <div className="flex items-center gap-1.5 px-2.5 py-1 bg-muted/60 border border-border rounded-md">
            <span>Barrera de Coherencia:</span>
            <span className="text-foreground font-bold">
              {props.online ? 'Activa (en línea)' : 'En espera de kernel'}
            </span>
          </div>
        </div>
      </div>

      {/* Sub Navigation */}
      <div
        role="tablist"
        aria-label="Secciones de Estado y Salud"
        className="flex max-w-full overflow-x-auto border-b border-border gap-1.5 font-sans"
      >
        <button
          ref={(el) => { tabRefs.current.summary = el; }}
          role="tab"
          id="tab-overview-summary"
          aria-controls="panel-overview-summary"
          aria-selected={subTab === 'summary'}
          tabIndex={subTab === 'summary' ? 0 : -1}
          onClick={() => setSubTab('summary')}
          onKeyDown={(e) => handleTabKeyDown(e, 'summary')}
          className={`flex min-w-max items-center px-3.5 py-1.5 text-[11px] font-bold transition-all border cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'summary'
              ? 'bg-card border-t-2 border-t-primary border-x-border border-b-card -mb-px text-primary rounded-t-md shadow-sm'
              : 'bg-muted/50 border-border text-muted-foreground hover:text-foreground hover:bg-muted rounded-t-md'
          }`}
        >
          Resumen General
        </button>
        <button
          ref={(el) => { tabRefs.current.interoception = el; }}
          role="tab"
          id="tab-overview-interoception"
          aria-controls="panel-overview-interoception"
          aria-selected={subTab === 'interoception'}
          tabIndex={subTab === 'interoception' ? 0 : -1}
          onClick={() => setSubTab('interoception')}
          onKeyDown={(e) => handleTabKeyDown(e, 'interoception')}
          className={`flex min-w-max items-center px-3.5 py-1.5 text-[11px] font-bold transition-all border cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'interoception'
              ? 'bg-card border-t-2 border-t-primary border-x-border border-b-card -mb-px text-primary rounded-t-md shadow-sm'
              : 'bg-muted/50 border-border text-muted-foreground hover:text-foreground hover:bg-muted rounded-t-md'
          }`}
        >
          Interocepción
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'summary' && (
          <div
            role="tabpanel"
            id="panel-overview-summary"
            aria-labelledby="tab-overview-summary"
            className="space-y-3"
          >
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
          <div
            role="tabpanel"
            id="panel-overview-interoception"
            aria-labelledby="tab-overview-interoception"
          >
            <InteroceptionTab
              interoception={props.interoception}
              memoryStats={props.memoryStats}
            />
          </div>
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

import React, { useState, useEffect } from 'react';
import { OverviewTab } from './OverviewTab';
import { SystemTab } from './SystemTab';
import { InteroceptionTab } from './InteroceptionTab';
import ResumeSessionWidget from './ResumeSessionWidget';
import RepoMapWidget from './RepoMapWidget';
import { 
  LayoutDashboard, 
  Wrench, 
  Activity, 
  MessageSquare, 
  ShieldCheck,
  CheckCircle2, 
  Terminal,
  Cpu
} from 'lucide-react';
import { TylluanLogo } from './TylluanLogo';

interface OverviewConsolidatedProps {
  bridge: any;
  goldenSignals: any;
  guildsUtilization: any;
  memoryRetention: any;
  sloSummary: any;
  guilds: any[];
  approvals: any[];
  memoryStats: any;
  healthDetailed: any;
  sysStatus: any;
  events: any[];
  interoception: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onClearLogs: () => void;
  refreshData: () => Promise<void>;
}

export function OverviewConsolidated(props: OverviewConsolidatedProps) {
  const [subTab, setSubTab] = useState('summary');

  return (
    <div className="space-y-6 font-sans">
      {/* Sovereign Substrate Telemetry Header */}
      <div className="p-4 bg-[#0B0F17]/90 border border-slate-800/80 rounded-2xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <TylluanLogo size="lg" animated={true} showText={false} />
          <div>
            <div className="flex items-center gap-2 font-mono">
              <span className="text-sm font-bold text-slate-100">Tylluan Cognitive Substrate</span>
              <span className="px-2 py-0.5 text-[9px] font-bold uppercase bg-slate-900 text-[#00F5D4] border border-slate-700 rounded">
                v0.14.0
              </span>
              <span className="px-2 py-0.5 text-[9px] font-bold uppercase bg-cyan-500/10 text-cyan-400 border border-cyan-500/30 rounded flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3" />
                CI Passing
              </span>
            </div>
            <p className="text-xs text-slate-400 mt-0.5 font-mono">
              Sovereign Local Kernel on :4000 • Noise XK P2P Mesh • ADR-011 Signal Loop Active
            </p>
          </div>
        </div>

        <div className="flex items-center gap-3 font-mono text-xs text-slate-400">
          <div className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-950 border border-slate-850 rounded-xl">
            <ShieldCheck className="w-3.5 h-3.5 text-[#00F5D4]" />
            <span>Coherence Gate:</span>
            <span className="text-slate-200 font-bold">Layer 1/2/3 Active</span>
          </div>
        </div>
      </div>

      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2 font-mono">
        <button
          onClick={() => setSubTab('summary')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-bold uppercase rounded-lg border transition-all ${
            subTab === 'summary'
              ? 'bg-[#00F5D4]/10 border-[#00F5D4]/40 text-[#00F5D4]'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <LayoutDashboard className="w-3.5 h-3.5" />
          Summary Cockpit
        </button>
        <button
          onClick={() => setSubTab('interoception')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-bold uppercase rounded-lg border transition-all ${
            subTab === 'interoception'
              ? 'bg-[#00F5D4]/10 border-[#00F5D4]/40 text-[#00F5D4]'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Activity className="w-3.5 h-3.5" />
          Interoception
        </button>
        <button
          onClick={() => setSubTab('system')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-bold uppercase rounded-lg border transition-all ${
            subTab === 'system'
              ? 'bg-[#00F5D4]/10 border-[#00F5D4]/40 text-[#00F5D4]'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Wrench className="w-3.5 h-3.5" />
          System Config
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
            />
            {/* Team Pulse Widget */}
            <TeamPulseWidget bridge={props.bridge} />
            {/* Task Registry Widget */}
            <TaskRegistryWidget bridge={props.bridge} />
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

function TeamPulseWidget({ bridge }: { bridge: any }) {
  const [messages, setMessages] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchPulse = async () => {
    if (!bridge) return;
    try {
      const data = await bridge.getColoquioThread("mision-activa");
      const msgs = data.messages || [];
      const last3 = [...msgs].slice(-3).reverse();
      setMessages(last3);
    } catch (e) {
      console.error("[TeamPulse] Failed to fetch thread:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPulse();
    const interval = setInterval(fetchPulse, 10000);
    return () => clearInterval(interval);
  }, [bridge]);

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
    <div className="rounded-xl border border-slate-800 bg-[#0B0F17]/90 overflow-hidden font-sans">
      <div className="px-4 py-3 border-b border-slate-800 bg-slate-900/60 flex items-center justify-between font-mono">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-bold uppercase tracking-widest text-[#00F5D4]">Team Pulse (Coloquio)</span>
          {!isIdle && !loading && (
            <span className="w-2 h-2 rounded-full bg-[#00F5D4] animate-pulse" />
          )}
        </div>
        <MessageSquare className="w-3.5 h-3.5 text-[#00F5D4]" />
      </div>

      <div className="p-4">
        {loading ? (
          <div className="text-center text-xs text-slate-500 font-mono py-2">Loading pulse...</div>
        ) : isIdle ? (
          <div className="text-center text-xs text-slate-500 font-mono py-4 flex flex-col items-center gap-1">
            <span className="text-slate-400 font-semibold">Team Idle</span>
            <span className="text-[10px] text-slate-600">No activity in the last 1 hour</span>
          </div>
        ) : (
          <div className="divide-y divide-slate-800/40">
            {messages.map((msg: any) => {
              const secondsAgo = Math.max(0, nowSecs - msg.created_at);
              const authorColor = msg.role === 'human' ? 'text-blue-400' : 'text-[#00F5D4]';
              const textPreview = msg.content.length > 80 ? msg.content.slice(0, 80) + '...' : msg.content;
              
              return (
                <div
                  key={msg.msg_id}
                  onClick={handleMessageClick}
                  className="py-3 first:pt-0 last:pb-0 flex items-start gap-3 hover:bg-slate-800/20 transition-all cursor-pointer rounded-lg px-2 -mx-2"
                >
                  <span className="text-base flex-shrink-0 mt-0.5" role="img" aria-label="avatar">
                    {getAgentIcon(msg.author_id)}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className={`text-xs font-bold font-mono ${authorColor}`}>@{msg.author_id}</span>
                      <span className="text-[9px] font-mono text-slate-500 flex-shrink-0">
                        {formatRelativeTime(secondsAgo)}
                      </span>
                    </div>
                    <p className="text-xs text-slate-300 mt-1 truncate">
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

function TaskRegistryWidget({ bridge }: { bridge: any }) {
  const [tasks] = useState<{ completed: string[]; inProgress: string[]; pending: string[] }>({
    completed: [],
    inProgress: [],
    pending: []
  });

  // Task registry backend not yet implemented — widget kept as placeholder
  return (
    <div className="rounded-xl border border-slate-800 bg-[#0B0F17]/90 p-4 space-y-3 font-sans">
      <div className="flex items-center justify-between font-mono">
        <span className="text-[10px] font-bold uppercase tracking-widest text-[#00F5D4] flex items-center gap-1.5">
          <Terminal className="w-3.5 h-3.5" />
          Active Worklog Registry
        </span>
        <span className="text-[10px] text-slate-500 font-mono">Real-time Task Registry</span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs font-mono">
        <div className="p-3 bg-slate-950 border border-slate-850 rounded-xl space-y-1">
          <div className="text-[10px] font-bold text-emerald-400 uppercase tracking-wider">Completed Recent</div>
          {tasks.completed.length === 0 ? (
            <div className="text-[11px] text-slate-600">No completed items listed</div>
          ) : (
            tasks.completed.map((t, idx) => (
              <div key={idx} className="text-[11px] text-slate-300 truncate">✓ {t}</div>
            ))
          )}
        </div>

        <div className="p-3 bg-slate-950 border border-slate-850 rounded-xl space-y-1">
          <div className="text-[10px] font-bold text-[#00F5D4] uppercase tracking-wider">In Progress</div>
          {tasks.inProgress.length === 0 ? (
            <div className="text-[11px] text-slate-600">No tasks currently in progress</div>
          ) : (
            tasks.inProgress.map((t, idx) => (
              <div key={idx} className="text-[11px] text-[#00F5D4] truncate">⚡ {t}</div>
            ))
          )}
        </div>

        <div className="p-3 bg-slate-950 border border-slate-850 rounded-xl space-y-1">
          <div className="text-[10px] font-bold text-amber-400 uppercase tracking-wider">Pending Next</div>
          {tasks.pending.length === 0 ? (
            <div className="text-[11px] text-slate-600">Queue clear</div>
          ) : (
            tasks.pending.map((t, idx) => (
              <div key={idx} className="text-[11px] text-slate-400 truncate">⏳ {t}</div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

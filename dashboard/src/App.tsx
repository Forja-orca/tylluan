import { useState, useEffect, useCallback } from 'react'
import {
  LayoutDashboard,
  Cpu,
  Database,
  Settings,
  Wifi, WifiOff,
  TrendingUp,
  Users,
  Clock,
  AlertTriangle,
  Network,
  Terminal,
  Wrench,
  Zap,
  ShieldCheck,
  MessageSquare,
  Search,
  Radio,
  Camera,
  Activity,
  Plug,
  Beaker,
  Bell,
  Link2,
  RefreshCw
} from 'lucide-react'
import { useNexus } from './hooks/useNexus'
import { useNexusSSE } from './hooks/useNexusSSE'
import { cn } from './lib/utils'
import { ErrorBoundary } from './components/ErrorBoundary'

import React, { lazy, Suspense } from 'react'
import { OverviewConsolidated } from './components/OverviewConsolidated'

const MemoryConsolidated = lazy(() => import('./components/MemoryConsolidated'))
const TeamConsolidated = lazy(() => import('./components/TeamConsolidated'))
const GuildsConsolidated = lazy(() => import('./components/GuildsConsolidated'))
const LabConsolidated = lazy(() => import('./components/LabConsolidated'))

function App() {
  const {
    online, events, guilds, stats, memoryStats, approvals,
    loading, error,
    goldenSignals, guildsUtilization, memoryRetention, sloSummary,
    healthDetailed, sysStatus, interoception,
    bridge, refreshData, clearLogs
  } = useNexus();
  
  const [activeTab, setActiveTab] = useState(() => localStorage.getItem('tylluan_active_tab') || 'overview');
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(() => new Set(['overview', localStorage.getItem('tylluan_active_tab') || 'overview']));
  const handleTabChange = (id: string) => {
    setActiveTab(id);
    localStorage.setItem('tylluan_active_tab', id);
    setMountedTabs(prev => new Set(prev).add(id));
  };

  useEffect(() => {
    const onSwitchTab = (event: Event) => {
      const id = (event as CustomEvent<string>).detail;
      if (id) handleTabChange(id);
    };
    window.addEventListener('nexus_switch_tab', onSwitchTab);
    return () => window.removeEventListener('nexus_switch_tab', onSwitchTab);
  }, []);
  const [lastRefresh, setLastRefresh] = useState(new Date());
  const [toasts, setToasts] = useState<{id: number, msg: string, guild?: string, type: 'info' | 'error'}[]>([]);
  const [kernelUptime, setKernelUptime] = useState(0);
  const [coloquioUnread, setColoquioUnread] = useState(0);
  const [activeMentions, setActiveMentions] = useState<Array<{ id: number, sender: string, channel: string, message: string, ts: Date }>>([]);
  const [showMentionsDropdown, setShowMentionsDropdown] = useState(false);

  const formatUptime = (secs: number) => {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    return `${h}h ${m}m ${s}s`;
  };
  const notify = useCallback((msg: string, type: 'info' | 'error' = 'info', guild?: string) => {
    const id = Date.now();
    setToasts(prev => [{id, msg, type, guild}, ...prev].slice(0, 5));
    setTimeout(() => setToasts(prev => prev.filter(t => t.id !== id)), 5000);
  }, []);

  useEffect(() => {
    const handleMention = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (!detail) return;
      const { agent_id, channel, message, sender } = detail;
      
      if (agent_id === 'user' || agent_id === 'all') {
        const newMention = {
          id: Date.now(),
          sender,
          channel,
          message,
          ts: new Date()
        };
        setActiveMentions(prev => [newMention, ...prev].slice(0, 10));
        notify(
          `@${sender} mentioned you in #${channel}: "${message.length > 60 ? message.slice(0, 60) + '...' : message}"`,
          'info',
          `Mention: @${agent_id}`
        );
      }
    };
    window.addEventListener('coloquio-mention', handleMention);
    return () => window.removeEventListener('coloquio-mention', handleMention);
  }, [notify]);

  // SSE layer — resilient, with error_result → toast routing
  const { connectionStatus, reconnectAttempts } = useNexusSSE(bridge, {
    onError: useCallback((msg: string, guild?: string) => {
      notify(msg, 'error', guild);
    }, [notify]),
    maxEvents: 200,
  });

  useEffect(() => {
    if (online) {
      setLastRefresh(new Date());
    }
  }, [online, stats, memoryStats]);

  useEffect(() => {
    if (online) {
      const interval = setInterval(() => setKernelUptime(prev => prev + 1), 1000);
      return () => clearInterval(interval);
    }
  }, [online]);

  // Poll coloquio unread for sidebar badge
  useEffect(() => {
    if (!bridge || !online) return;
    const poll = async () => {
      try {
        const data = await bridge.getColoquioUnread('user');
        setColoquioUnread(data.total_unread ?? 0);
      } catch {}
    };
    poll();
    const id = setInterval(poll, 5000);
    return () => clearInterval(id);
  }, [bridge, online]);

  const [activeAgents, setActiveAgents] = useState<string[]>([]);

  useEffect(() => {
    if (!bridge || !online) return;
    const fetchActiveAgents = async () => {
      try {
        const data = await bridge.getColoquioThread("mision-activa");
        const msgs = data.messages || [];
        const nowSecs = Math.floor(Date.now() / 1000);
        // unique author ids active in the last 30 minutes
        const active = Array.from(new Set(
          msgs
            .filter((m: any) => (nowSecs - m.created_at) < 1800)
            .map((m: any) => m.author_id)
        )) as string[];
        setActiveAgents(active);
      } catch (err) {
        console.error('Failed to load active agents:', err);
      }
    };

    fetchActiveAgents();
    const interval = setInterval(fetchActiveAgents, 10000);
    return () => clearInterval(interval);
  }, [bridge, online]);

  const getAgentDotColor = (agentId: string): string => {
    const cleanId = agentId.toLowerCase();
    if (cleanId.includes('human') || cleanId.includes('user')) return 'bg-amber-500';
    let hash = 0;
    for (let i = 0; i < cleanId.length; i++) {
      hash = cleanId.charCodeAt(i) + ((hash << 5) - hash);
    }
    const colors = ['bg-orange-500', 'bg-blue-400', 'bg-purple-500', 'bg-cyan-500', 'bg-pink-500', 'bg-indigo-500', 'bg-teal-500', 'bg-lime-500', 'bg-rose-500'];
    return colors[Math.abs(hash) % colors.length];
  };

  const VISION_ENABLED = true;

  const tabs: Array<{
    id: string;
    name: string;
    icon: any;
    badge?: number | null;
  }> = [
    { id: 'overview', name: 'Overview', icon: LayoutDashboard },
    { id: 'memory', name: 'Memory', icon: Database },
    { id: 'team', name: 'Team', icon: Users, badge: coloquioUnread > 0 ? coloquioUnread : null },
    { id: 'guilds', name: 'Guilds', icon: Cpu },
    { id: 'lab', name: 'Laboratory', icon: Beaker },
  ];

  return (
    <div className="min-h-screen bg-slate-950 text-slate-200 font-sans selection:bg-emerald-500/30">
      {/* Top Navbar */}
      <header className="h-16 border-b border-slate-800 bg-slate-950/80 backdrop-blur-md sticky top-0 z-50 px-6 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2 group cursor-pointer" onClick={() => handleTabChange('overview')}>
            <div className="relative">
              <div className="w-8 h-8 bg-gradient-to-br from-emerald-400 to-emerald-600 rounded-lg shadow-lg shadow-emerald-500/20 flex items-center justify-center transform group-hover:rotate-12 transition-transform">
                <LayoutDashboard className="w-5 h-5 text-slate-950" />
              </div>
              <div className="absolute -top-1 -right-1 w-3 h-3 bg-slate-950 rounded-full flex items-center justify-center">
                <div className={cn("w-2 h-2 rounded-full", online ? "bg-emerald-500 animate-pulse" : "bg-red-500")} />
              </div>
            </div>
            <div>
              <h1 className="text-sm font-bold tracking-tight text-white uppercase">Tylluan</h1>
              <div className="flex items-center gap-2 text-[10px] text-slate-500 font-mono">
                <span className="text-slate-400 font-bold">v0.6.0</span>
                <span className="opacity-50">·</span>
                <span className="text-slate-500">Portable Foundation</span>
                <span className="opacity-50">·</span>
                <span className={online ? "text-emerald-500/80" : "text-red-500/80"}>{online ? 'Sovereign' : 'Offline'}</span>
                {(sysStatus?.loading_model || (interoception?.capabilities as any)?.loading_model) && (
                  <>
                    <span className="opacity-50">·</span>
                    <span className="text-amber-400 flex items-center gap-1">
                      <RefreshCw className="w-3 h-3 animate-spin" />
                      Loading {sysStatus?.embedding_model || interoception?.capabilities?.embedding_model || "model"}...
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-6">
          <div className="hidden lg:flex items-center gap-4 text-[10px] font-mono text-slate-500">
             <div className="flex flex-col items-end">
               <span className="uppercase opacity-50 tracking-widest text-[8px]">Uptime</span>
               <span className="text-slate-300">{online ? formatUptime(kernelUptime) : '—'}</span>
             </div>
             <div className="h-6 w-px bg-slate-800" />
             <div className="flex flex-col items-end">
               <span className="uppercase opacity-50 tracking-widest text-[8px]">Last Refresh</span>
               <span className="text-slate-300">{lastRefresh.toLocaleTimeString()}</span>
             </div>
          </div>
          
          {activeAgents.length > 0 && (
            <div className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-slate-900 border border-slate-800 text-[10px] font-mono font-bold uppercase tracking-wider text-slate-400">
              <span className="text-slate-300">{activeAgents.length} {activeAgents.length === 1 ? 'agent' : 'agents'} active</span>
              <div className="flex gap-1">
                {activeAgents.map(agent => (
                  <span
                    key={agent}
                    className={`w-1.5 h-1.5 rounded-full ${getAgentDotColor(agent)}`}
                    title={`@${agent} active`}
                  />
                ))}
              </div>
            </div>
          )}

          <div className={cn(
            'flex items-center gap-2 px-3 py-1.5 rounded-full border transition-colors',
            connectionStatus === 'connected'
              ? 'bg-slate-900 border-slate-800'
              : connectionStatus === 'reconnecting'
              ? 'bg-amber-950/40 border-amber-500/30 animate-pulse'
              : 'bg-red-950/40 border-red-500/30'
          )}>
            {connectionStatus === 'connected'
              ? <Wifi className="w-3 h-3 text-emerald-400" />
              : connectionStatus === 'reconnecting'
              ? <Radio className="w-3 h-3 text-amber-400" />
              : <WifiOff className="w-3 h-3 text-red-400" />}
            <span className="text-[10px] font-bold uppercase tracking-wider">
              {connectionStatus === 'connected'
                ? 'Secure Link'
                : connectionStatus === 'reconnecting'
                ? `Reconnecting (${reconnectAttempts})`
                : 'No Connection'}
            </span>
          </div>
          
          {/* Notifications Bell */}
          <div className="relative">
            <button
              onClick={() => setShowMentionsDropdown(!showMentionsDropdown)}
              className={cn(
                "p-2 rounded-full border transition-all relative cursor-pointer",
                activeMentions.length > 0 
                  ? "bg-amber-500/10 border-amber-500/30 text-amber-400 animate-pulse animate-duration-1000" 
                  : "bg-slate-900 border-slate-800 text-slate-400 hover:text-white"
              )}
              title="Menciones Recientes"
            >
              <Bell className="w-4 h-4" />
              {activeMentions.length > 0 && (
                <span className="absolute -top-1 -right-1 w-2.5 h-2.5 bg-amber-500 rounded-full ring-2 ring-slate-950" />
              )}
            </button>

            {showMentionsDropdown && (
              <div className="absolute right-0 mt-2 w-80 bg-slate-900 border border-slate-800 rounded-xl shadow-2xl z-50 overflow-hidden">
                <div className="px-4 py-3 bg-slate-800/50 border-b border-slate-800 flex items-center justify-between">
                  <span className="text-xs font-bold text-slate-300 uppercase">Menciones (@yo)</span>
                  {activeMentions.length > 0 && (
                    <button 
                      onClick={() => {
                        setActiveMentions([]);
                        setShowMentionsDropdown(false);
                      }}
                      className="text-[10px] text-emerald-400 hover:text-emerald-300 font-bold uppercase cursor-pointer"
                    >
                      Limpiar
                    </button>
                  )}
                </div>
                <div className="max-h-64 overflow-y-auto divide-y divide-slate-800/50">
                  {activeMentions.length === 0 ? (
                    <div className="px-4 py-6 text-center text-xs text-slate-500">
                      Sin menciones recientes
                    </div>
                  ) : (
                    activeMentions.map((m) => (
                      <div key={m.id} className="p-3 hover:bg-slate-800/30 transition-colors">
                        <div className="flex items-center justify-between mb-1">
                          <span className="text-xs font-bold text-emerald-400">@{m.sender}</span>
                          <span className="text-[9px] font-mono text-slate-500">#{m.channel}</span>
                        </div>
                        <p className="text-xs text-slate-300 line-clamp-2">{m.message}</p>
                        <span className="text-[8px] font-mono text-slate-600 block mt-1">
                          {m.ts.toLocaleTimeString()}
                        </span>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>

          {online && healthDetailed && (
            <div 
              className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-slate-900 border border-slate-800 cursor-help group relative"
              title={`Embeddings: ${healthDetailed.components?.embeddings?.ok ? '✓' : '✗'} | Reranker: ${healthDetailed.components?.reranker?.ok ? '✓' : '✗'} | Guilds: ${healthDetailed.components?.guilds?.active}/${healthDetailed.components?.guilds?.total} | Silva: ${healthDetailed.components?.silva?.nodes}n/${healthDetailed.components?.silva?.edges}e`}
            >
              <div className={cn(
                "w-2 h-2 rounded-full",
                healthDetailed.score >= 80 ? "bg-emerald-500" :
                healthDetailed.score >= 50 ? "bg-amber-500" : "bg-red-500"
              )} />
              <span className="text-[10px] font-bold uppercase tracking-wider">
                {healthDetailed.status}
              </span>
            </div>
          )}
        </div>
      </header>

      <div className="flex h-[calc(100vh-64px)] overflow-hidden">
        {/* Sidebar Navigation */}
        <aside className="w-64 border-r border-slate-800 bg-slate-950 flex flex-col shrink-0 overflow-y-auto">
          <div className="p-4 space-y-1">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              const active = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => handleTabChange(tab.id)}
                  className={cn(
                    "w-full flex items-center justify-between px-3 py-2.5 rounded-xl text-sm font-medium transition-all group",
                    active 
                      ? "bg-emerald-500/10 text-emerald-400 shadow-sm shadow-emerald-500/5" 
                      : "text-slate-400 hover:text-slate-100 hover:bg-slate-900"
                  )}
                >
                  <div className="flex items-center gap-3">
                    <Icon className={cn("w-4 h-4 transition-colors", active ? "text-emerald-400" : "text-slate-500 group-hover:text-slate-300")} />
                    <span>{tab.name}</span>
                  </div>
                  {tab.badge && (
                    <span className="px-1.5 py-0.5 rounded-full bg-amber-500/20 text-amber-500 text-[10px] font-bold min-w-[1.2rem] text-center border border-amber-500/30">
                      {tab.badge}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
          
          <div className="mt-auto p-4 border-t border-slate-800 bg-slate-900/20">
             <div className="flex items-center gap-3 mb-3">
               <div className="w-8 h-8 rounded-lg bg-slate-800 flex items-center justify-center text-xs font-bold text-slate-400 border border-slate-700">v0.6</div>
               <div className="overflow-hidden">
                 <p className="text-[10px] font-bold text-slate-300 uppercase truncate">Tylluan Hub</p>
                 <p className="text-[9px] text-slate-500 font-mono truncate">v0.6.0 (Portable)</p>
               </div>
             </div>
             <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                <div className="h-full bg-emerald-500 w-full animate-pulse" />
             </div>
          </div>
        </aside>

        {/* Main Content Area */}
        <main className="flex-1 min-h-0 flex flex-col overflow-hidden bg-slate-950">
          {error && (
            <div className="mb-6 p-4 rounded-2xl bg-red-500/10 border border-red-500/20 flex items-start gap-3">
              <AlertTriangle className="w-5 h-5 text-red-500 shrink-0 mt-0.5" />
              <div>
                <h4 className="text-sm font-bold text-red-400">Communication Error</h4>
                <p className="text-xs text-red-500/80 mt-1">{error}</p>
              </div>
            </div>
          )}

          {/* Consolidated Tab Panels */}
          <Suspense fallback={<div className="flex-1 flex items-center justify-center font-mono text-xs text-slate-500">Loading module...</div>}>
            <div className={cn("flex-1 min-h-0 flex flex-col", (activeTab === 'overview' || activeTab === 'guilds' || activeTab === 'lab') ? "overflow-y-auto p-6" : "p-6")}>
              {!online && (
                <div className="mb-6 p-4 rounded-xl bg-red-950/40 border border-red-500/20 flex flex-col md:flex-row md:items-center justify-between gap-4 text-red-200 animate-pulse shrink-0">
                  <div className="flex items-start md:items-center gap-3">
                    <WifiOff className="w-5 h-5 text-red-400 shrink-0 mt-0.5 md:mt-0" />
                    <div>
                      <h4 className="text-xs font-bold uppercase tracking-wider text-red-400">Kernel Conexión Offline</h4>
                      <p className="text-[11px] text-slate-400 mt-0.5">El microkernel local de Tylluan en 127.0.0.1:3030 no está respondiendo. Ejecuta <code className="bg-slate-950 px-1.5 py-0.5 rounded text-red-300 font-mono text-[10px]">tylluan-cli start</code> localmente para restablecer la comunicación.</p>
                    </div>
                  </div>
                  <div className="text-[10px] font-mono font-bold px-3 py-1 rounded-full bg-red-500/10 text-red-400 border border-red-500/20 shrink-0 w-fit">
                    PORTABLE FOUNDATION v0.6.0
                  </div>
                </div>
              )}
              {activeTab === 'overview' && (
                <ErrorBoundary>
                  <OverviewConsolidated
                    bridge={bridge}
                    goldenSignals={goldenSignals}
                    guildsUtilization={guildsUtilization}
                    memoryRetention={memoryRetention}
                    sloSummary={sloSummary}
                    guilds={guilds}
                    approvals={approvals}
                    memoryStats={memoryStats}
                    healthDetailed={healthDetailed}
                    sysStatus={sysStatus}
                    events={events}
                    interoception={interoception}
                    notify={notify}
                    onClearLogs={clearLogs}
                  />
                </ErrorBoundary>
              )}
              {activeTab === 'memory' && mountedTabs.has('memory') && (
                <ErrorBoundary>
                  <MemoryConsolidated
                    bridge={bridge}
                    notify={notify}
                    memoryStats={memoryStats}
                    online={online}
                  />
                </ErrorBoundary>
              )}
              {activeTab === 'team' && mountedTabs.has('team') && (
                <ErrorBoundary>
                  <TeamConsolidated
                    bridge={bridge}
                  />
                </ErrorBoundary>
              )}
              {activeTab === 'guilds' && mountedTabs.has('guilds') && (
                <ErrorBoundary>
                  <GuildsConsolidated
                    bridge={bridge}
                    notify={notify}
                    events={events}
                    online={online}
                  />
                </ErrorBoundary>
              )}
              {activeTab === 'lab' && mountedTabs.has('lab') && (
                <ErrorBoundary>
                  <LabConsolidated
                    bridge={bridge}
                    notify={notify}
                    events={events}
                    onClearLogs={clearLogs}
                    online={online}
                  />
                </ErrorBoundary>
              )}
            </div>
          </Suspense>
        </main>
      </div>

      {/* Notifications Layer */}
      <div className="fixed bottom-6 right-6 z-[100] flex flex-col gap-3">
        {toasts.map((t) => (
          <div key={t.id} className={cn(
            "p-4 rounded-2xl border shadow-2xl flex items-center gap-3 animate-in slide-in-from-right min-w-[300px]",
            t.type === 'error' ? "bg-red-950 border-red-500/30 text-red-200" : "bg-slate-900 border-slate-800 text-slate-100"
          )}>
            {t.type === 'error' ? <AlertTriangle className="w-5 h-5 text-red-500" /> : <Zap className="w-5 h-5 text-emerald-400" />}
            <div>
              {t.guild && <span className="block text-[10px] font-bold uppercase tracking-widest text-slate-500 mb-0.5">{t.guild}</span>}
              <p className="text-sm">{t.msg}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

export default App
